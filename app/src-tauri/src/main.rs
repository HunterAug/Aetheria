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

/// Holds the handle to the running delegate child process so it can be
/// killed when the app exits. `None` before setup runs or after the child
/// has already been reaped.
struct DelegateProcess(Mutex<Option<CommandChild>>);

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(DelegateProcess(Mutex::new(None)))
        .setup(|app| {
            let shell = app.shell();

            // TODO(next): this env var is a temporary stopgap, not a real
            // solution. It unlocks the delegate's encrypted identity.key
            // non-interactively so a sidecar with no attached terminal
            // doesn't hang forever on the `rpassword` prompt in
            // delegate/src/keys.rs. The real fix is an in-app "enter your
            // passphrase" unlock screen that sends a new `unlock
            // { passphrase }` IPC request to the delegate before any signing
            // operation, instead of the delegate reading a passphrase from
            // its own environment/stdin. That needs new request/response
            // variants in delegate/src/ipc.rs and changes to
            // delegate/src/keys.rs's unlock path - deliberately deferred
            // because a concurrent agent is actively editing both of those
            // files for the NWC/Lightning payment feature and touching them
            // here would conflict. See CLAUDE.md's "Desktop shell (Tauri)"
            // section for the full story.
            let (mut rx, child) = shell
                .sidecar("aetheria-delegate")
                .expect("failed to create aetheria-delegate sidecar command")
                .env("AETHERIA_DEV_PASSPHRASE", "aetheria-dev-local-only")
                // The delegate logs via `tracing`; without RUST_LOG set its
                // subscriber prints nothing, and log forwarding below would
                // silently look broken. This only affects verbosity, not
                // delegate behavior.
                .env("RUST_LOG", "info")
                .spawn()
                .expect("failed to spawn aetheria-delegate sidecar");

            *app.state::<DelegateProcess>().0.lock().unwrap() = Some(child);

            // Forward the delegate's stdout/stderr into this process's own
            // console (visible in `npm run tauri dev`'s terminal) so
            // delegate logs stay visible without a separate window.
            tauri::async_runtime::spawn(async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        CommandEvent::Stdout(line) => {
                            print!("[delegate] {}", String::from_utf8_lossy(&line));
                            let _ = std::io::stdout().flush();
                        }
                        CommandEvent::Stderr(line) => {
                            eprint!("[delegate] {}", String::from_utf8_lossy(&line));
                            let _ = std::io::stderr().flush();
                        }
                        CommandEvent::Error(err) => {
                            eprintln!("[delegate] process error: {err}");
                        }
                        CommandEvent::Terminated(payload) => {
                            eprintln!("[delegate] exited: {payload:?}");
                        }
                        _ => {}
                    }
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Aetheria")
        .run(|app_handle, event| {
            // Make sure the delegate never outlives the window - otherwise
            // it would keep holding port 47021 and the SQLite lock after the
            // user thinks they've quit the app.
            if let RunEvent::ExitRequested { .. } | RunEvent::Exit = event {
                if let Some(child) = app_handle
                    .state::<DelegateProcess>()
                    .0
                    .lock()
                    .unwrap()
                    .take()
                {
                    let _ = child.kill();
                }
            }
        });
}
