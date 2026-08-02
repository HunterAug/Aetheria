// The desktop shell only renders the UI; all crypto, key storage, and
// Freenet/NWC networking lives in the separate `aetheria-delegate` daemon
// (see delegate/), which the UI talks to over a loopback WebSocket.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Aetheria");
}
