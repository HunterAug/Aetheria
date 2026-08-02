// Thin client for the local Delegate daemon's WebSocket IPC (see
// delegate/src/ipc.rs). The UI never touches key material or Freenet
// contract calls directly — everything goes through this loopback socket.

const DELEGATE_IPC_URL = "ws://127.0.0.1:47021";

export function connectDelegate(): WebSocket {
  return new WebSocket(DELEGATE_IPC_URL);
}
