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
use std::sync::Mutex;

use tauri::{Manager, RunEvent};
use tauri_plugin_shell::{process::CommandChild, process::CommandEvent, ShellExt};

/// Holds the handles to the running sidecar child processes (Freenet node,
/// then delegate) so they can be killed when the app exits. Populated in
/// spawn order by `.setup()`; killed in reverse (LIFO) order on exit, so the
/// delegate - which depends on the node - is signalled before the node it
/// was talking to disappears.
struct Sidecars(Mutex<Vec<CommandChild>>);

/// Forwards a sidecar's stdout/stderr into this process's own console
/// (visible in `npm run tauri dev`'s terminal), prefixed so interleaved
/// output from multiple sidecars stays distinguishable.
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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(Sidecars(Mutex::new(Vec::new())))
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
            let mut freenet_cmd = shell
                .sidecar("freenet")
                .expect("failed to create freenet sidecar command")
                .args(["network"]);
            // Dev/test escape hatch (same spirit as delegate/src/main.rs's
            // AETHERIA_DATA_DIR_OVERRIDE) - lets a "fresh machine" install
            // test point the bundled node at an empty scratch directory
            // instead of this machine's real Freenet data, without touching
            // real data. Unset for any normal run.
            if let Ok(dir) = std::env::var("AETHERIA_FREENET_DATA_DIR_OVERRIDE") {
                freenet_cmd = freenet_cmd.args(["--data-dir", &dir, "--config-dir", &dir]);
            }
            let (freenet_rx, freenet_child) = freenet_cmd
                .spawn()
                .expect("failed to spawn freenet sidecar");
            forward_logs("freenet", freenet_rx);
            app.state::<Sidecars>().0.lock().unwrap().push(freenet_child);

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
            app.state::<Sidecars>().0.lock().unwrap().push(delegate_child);

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Aetheria")
        .run(|app_handle, event| {
            // Make sure neither sidecar outlives the window - otherwise the
            // delegate would keep holding port 47021 and the SQLite lock,
            // and the bundled Freenet node would keep holding 7509, after
            // the user thinks they've quit the app. Killed in reverse spawn
            // order (delegate, then freenet).
            if let RunEvent::ExitRequested { .. } | RunEvent::Exit = event {
                let sidecars = app_handle.state::<Sidecars>();
                let mut children = sidecars.0.lock().unwrap();
                while let Some(child) = children.pop() {
                    let _ = child.kill();
                }
            }
        });
}
