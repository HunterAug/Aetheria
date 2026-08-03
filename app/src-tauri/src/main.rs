// The desktop shell only renders the UI; all crypto, key storage, and
// Freenet/NWC networking lives in the separate `aetheria-delegate` daemon
// (see delegate/), which the UI talks to over a loopback WebSocket
// (ws://127.0.0.1:47021, see app/src/lib/delegate.ts).
//
// This process is responsible for one extra thing beyond rendering: starting
// and stopping that daemon automatically, bundled as a Tauri "sidecar" (see
// tauri.conf.json's bundle.externalBin and https://v2.tauri.app/develop/sidecar/)
// so the user never has to open a terminal and run it by hand.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_shell::{process::CommandChild, process::CommandEvent, ShellExt};

/// Holds the delegate sidecar's child handle so it can be killed on exit.
/// Spawned once in `.setup()` and never respawned - unlike Freenet (below),
/// the delegate is Aetheria's own binary with no external auto-update
/// mechanism to fight with, so a plain one-shot spawn is enough for it.
struct DelegateChild(Mutex<Option<CommandChild>>);

/// Holds the *current* Freenet sidecar child handle. Unlike the delegate,
/// this gets replaced every time `supervise_freenet` respawns the node (see
/// that function), so the exit handler always kills whichever instance is
/// actually running.
struct FreenetChild(Mutex<Option<CommandChild>>);

/// Set by the exit handler before killing anything, so `supervise_freenet`
/// can tell "the node exited because we just killed it, app is closing"
/// apart from "the node exited on its own" and skip respawning in the
/// former case.
struct ShuttingDown(AtomicBool);

/// Forwards a sidecar's stdout/stderr into this process's own console
/// (visible in `npm run tauri dev`'s terminal), prefixed so interleaved
/// output from multiple sidecars stays distinguishable. Used for the
/// delegate, which never needs to inspect its own exit code (see
/// `supervise_freenet` for why Freenet's sidecar needs its own loop instead
/// of this).
fn forward_logs(tag: &'static str, mut rx: tauri::async_runtime::Receiver<CommandEvent>) {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line) => {
                    print!("[{tag}] {}", String::from_utf8_lossy(&line));
                    let _ = std::io::stdout().flush();
                }
                CommandEvent::Stderr(line) => {
                    eprint!("[{tag}] {}", String::from_utf8_lossy(&line));
                    let _ = std::io::stderr().flush();
                }
                CommandEvent::Error(err) => {
                    eprintln!("[{tag}] process error: {err}");
                }
                CommandEvent::Terminated(payload) => {
                    eprintln!("[{tag}] exited: {payload:?}");
                }
                _ => {}
            }
        }
    });
}

/// Spawns the bundled Freenet node and keeps it running for the life of the
/// app - a minimal supervisor, because Freenet's own binary needs one.
///
/// Discovered directly while debugging "Aetheria isn't connecting to
/// Freenet" on an install where this was previously a bare one-shot
/// `.spawn()` (same shape as the delegate's, below): the node's own log
/// (`%LOCALAPPDATA%\freenet\logs\freenet.error.*.log`) showed it detecting a
/// newer released version, then exiting with code 42 a few seconds after
/// every launch - `freenet.exe --help`'s own `update` subcommand and its
/// `--disable-auto-update` flag's docs confirm this is intentional: a bare
/// `freenet network` process (this app's sidecar, chosen over `freenet
/// service` specifically to avoid that layer's onboarding/crash-loop
/// machinery, see the sidecar comment below) is documented to exit 42 and
/// rely on *something* supervising it to run `freenet update` and relaunch -
/// `freenet service` does that itself; a plain Tauri sidecar previously did
/// not, so the bundled node silently died on every single launch and never
/// came back, leaving the delegate stuck retrying a connection to a now-dead
/// port. `--disable-auto-update` is explicitly NOT the fix here (its own
/// help text: normal release nodes "MUST NOT set this, or it stops receiving
/// security/protocol updates") - the real fix is to actually be the
/// supervisor Freenet expects.
///
/// Also bounds plain crashes (distinct from the update-exit case): if the
/// node exits some other way and dies again within 5 seconds of respawning,
/// repeated `CRASH_LOOP_LIMIT` times, this gives up rather than spinning
/// forever - a node that ran for a while first (past that threshold) resets
/// the counter, so a one-off blip doesn't count against a later, unrelated
/// one.
const CRASH_LOOP_LIMIT: u32 = 3;
const MIN_STABLE_UPTIME_SECS: u64 = 5;

fn supervise_freenet(app: AppHandle, args: Vec<String>) {
    tauri::async_runtime::spawn(async move {
        let mut consecutive_crashes = 0u32;
        loop {
            if app.state::<ShuttingDown>().0.load(Ordering::SeqCst) {
                return;
            }

            let spawn_result = app
                .shell()
                .sidecar("freenet")
                .expect("failed to create freenet sidecar command")
                .args(&args)
                .spawn();
            let (mut rx, child) = match spawn_result {
                Ok(pair) => pair,
                Err(err) => {
                    eprintln!("[freenet] failed to spawn: {err}");
                    return;
                }
            };
            app.state::<FreenetChild>().0.lock().unwrap().replace(child);

            let started_at = Instant::now();
            let mut exit_code = None;
            while let Some(event) = rx.recv().await {
                match event {
                    CommandEvent::Stdout(line) => {
                        print!("[freenet] {}", String::from_utf8_lossy(&line));
                        let _ = std::io::stdout().flush();
                    }
                    CommandEvent::Stderr(line) => {
                        eprint!("[freenet] {}", String::from_utf8_lossy(&line));
                        let _ = std::io::stderr().flush();
                    }
                    CommandEvent::Error(err) => {
                        eprintln!("[freenet] process error: {err}");
                    }
                    CommandEvent::Terminated(payload) => {
                        eprintln!("[freenet] exited: {payload:?}");
                        exit_code = payload.code;
                        break;
                    }
                    _ => {}
                }
            }

            if app.state::<ShuttingDown>().0.load(Ordering::SeqCst) {
                return;
            }

            if exit_code == Some(42) {
                eprintln!(
                    "[freenet] update required (exit 42) - running `freenet update --quiet` before respawning"
                );
                match app
                    .shell()
                    .sidecar("freenet")
                    .expect("failed to create freenet sidecar command")
                    .args(["update", "--quiet"])
                    .spawn()
                {
                    Ok((mut update_rx, _update_child)) => {
                        while let Some(event) = update_rx.recv().await {
                            match event {
                                CommandEvent::Stdout(line) => {
                                    print!("[freenet update] {}", String::from_utf8_lossy(&line));
                                }
                                CommandEvent::Stderr(line) => {
                                    eprint!("[freenet update] {}", String::from_utf8_lossy(&line));
                                }
                                CommandEvent::Terminated(payload) => {
                                    eprintln!("[freenet update] finished: {payload:?}");
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("[freenet update] failed to run: {err}");
                    }
                }
                consecutive_crashes = 0;
                continue;
            }

            if started_at.elapsed().as_secs() >= MIN_STABLE_UPTIME_SECS {
                consecutive_crashes = 0;
            } else {
                consecutive_crashes += 1;
            }
            if consecutive_crashes >= CRASH_LOOP_LIMIT {
                eprintln!(
                    "[freenet] exited {consecutive_crashes} times in a row within {MIN_STABLE_UPTIME_SECS}s of starting - giving up, not respawning again"
                );
                return;
            }
        }
    });
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(DelegateChild(Mutex::new(None)))
        .manage(FreenetChild(Mutex::new(None)))
        .manage(ShuttingDown(AtomicBool::new(false)))
        .setup(|app| {
            let shell = app.shell();

            // Spawned first: the delegate's `FreenetBridge::connect_local()`
            // needs a real Freenet node listening on 127.0.0.1:7509 before it
            // can do anything, and this bundled node is that node - nothing
            // installs or starts one otherwise. Freenet takes a few seconds
            // to bind its WebSocket API after launch; rather than sequence
            // startup around that here (fixed sleeps are exactly the kind of
            // flaky timing hack this codebase avoids elsewhere, see
            // CLAUDE.md), `FreenetBridge::connect_local()` itself now retries
            // with backoff (see delegate/src/freenet_bridge.rs), so spawning
            // both sidecars back-to-back and letting the delegate's own
            // retry loop absorb the gap is simpler and also helps anyone
            // launching the delegate by hand before Freenet has finished
            // booting.
            //
            // Invocation matches the one already verified working on this
            // project (see CLAUDE.md's environment notes): plain `network`
            // mode, no `service`/wrapper subsystem (that layer's onboarding/
            // auto-restart machinery is meant for a persistent background
            // install, not a process Tauri already supervises) and no
            // `--data-dir`/`--config-dir` override (Freenet's own OS-default
            // data directory, `%LOCALAPPDATA%\The Freenet Project Inc\
            // Freenet\data\` on Windows, doesn't collide with Aetheria's own
            // `%APPDATA%\aetheria\aetheria-delegate\data\` - verified by
            // inspecting the running node's logs). See THIRD_PARTY_LICENSES.md
            // for why bundling this AGPL-3.0 binary here is fine (mere
            // aggregation - the delegate only ever talks to it over the
            // network API, never links against it).
            //
            // Actually spawning (and respawning across the app's lifetime)
            // happens in `supervise_freenet`, not here - see that function's
            // docs for why a one-shot spawn isn't enough for this specific
            // sidecar.
            let mut freenet_args = vec!["network".to_string()];
            // Dev/test escape hatch (same spirit as delegate/src/main.rs's
            // AETHERIA_DATA_DIR_OVERRIDE) - lets a "fresh machine" install
            // test point the bundled node at an empty scratch directory
            // instead of this machine's real Freenet data, without touching
            // real data. Unset for any normal run.
            if let Ok(dir) = std::env::var("AETHERIA_FREENET_DATA_DIR_OVERRIDE") {
                freenet_args.push("--data-dir".to_string());
                freenet_args.push(dir.clone());
                freenet_args.push("--config-dir".to_string());
                freenet_args.push(dir);
            }
            supervise_freenet(app.handle().clone(), freenet_args);

            // No passphrase env var here on purpose: the delegate starts
            // locked and stays that way until the in-app `UnlockScreen`
            // (app/src/components/UnlockScreen.tsx) sends a real `unlock`
            // IPC request - see delegate/src/ipc.rs's module docs. The old
            // AETHERIA_DEV_PASSPHRASE stopgap that used to live here is still
            // honored by delegate/src/keys.rs for CLI/dev use (set it in your
            // own shell before launching the delegate directly), just no
            // longer hardcoded into the shipped app's own launch path.
            let (delegate_rx, delegate_child) = shell
                .sidecar("aetheria-delegate")
                .expect("failed to create aetheria-delegate sidecar command")
                // The delegate logs via `tracing`; without RUST_LOG set its
                // subscriber prints nothing, and log forwarding above would
                // silently look broken. This only affects verbosity, not
                // delegate behavior.
                .env("RUST_LOG", "info")
                .spawn()
                .expect("failed to spawn aetheria-delegate sidecar");
            forward_logs("delegate", delegate_rx);
            app.state::<DelegateChild>().0.lock().unwrap().replace(delegate_child);

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Aetheria")
        .run(|app_handle, event| {
            // Make sure neither sidecar outlives the window - otherwise the
            // delegate would keep holding port 47021 and the SQLite lock,
            // and the bundled Freenet node would keep holding 7509, after
            // the user thinks they've quit the app. Delegate first (it
            // depends on Freenet), then Freenet. Setting `ShuttingDown`
            // first tells `supervise_freenet` that the `Terminated` event
            // it's about to see from this kill is expected, not a crash to
            // respawn from.
            if let RunEvent::ExitRequested { .. } | RunEvent::Exit = event {
                app_handle.state::<ShuttingDown>().0.store(true, Ordering::SeqCst);
                if let Some(child) = app_handle.state::<DelegateChild>().0.lock().unwrap().take() {
                    let _ = child.kill();
                }
                if let Some(child) = app_handle.state::<FreenetChild>().0.lock().unwrap().take() {
                    let _ = child.kill();
                }
            }
        });
}
