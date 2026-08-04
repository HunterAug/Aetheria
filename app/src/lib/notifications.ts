// Turns the delegate's "someone you follow just published" pushes into real
// OS notifications (Windows toasts).
//
// The chain is: delegate/src/watcher.rs notices a new post on the real
// Freenet network (a subscription push, or its polling backstop) → pushes a
// `new_post` event down the IPC WebSocket → this module → the tiny
// `show_notification` Tauri command in app/src-tauri/src/main.rs → the OS.
// Nothing in that chain is a server: the only two processes involved are the
// user's own delegate and their own Freenet node.
//
// The webview is the middle link because it's the only part of the app that
// can reach both the delegate (which owns the network connection but has no
// window) and Tauri's APIs (which own the toast but not the network). Hiding
// the window to the tray keeps this running - that's exactly why the tray
// work landed alongside it.

import { delegate, type DelegateEvent, type NewPostEvent } from "./delegate";

/// Whether we're running inside the real Tauri shell rather than a plain
/// browser pointed at the Vite dev server. Tauri v2 injects
/// `__TAURI_INTERNALS__` before any app code runs.
function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function showOsNotification(title: string, body: string): Promise<void> {
  if (inTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("show_notification", { title, body });
    return;
  }

  // Dev fallback: a normal browser against the Vite dev server, where Tauri's
  // command doesn't exist. Genuinely useful (it's how this path gets driven
  // without a full desktop build) and harmless in production, where the
  // branch above always wins. WebView2 doesn't implement the web Notification
  // API, so this is *only* ever a dev path, never a silent substitute for the
  // real toast.
  if (typeof Notification === "undefined") return;
  if (Notification.permission === "default") {
    await Notification.requestPermission();
  }
  if (Notification.permission === "granted") {
    new Notification(title, { body });
  }
}

function notificationText(event: NewPostEvent): { title: string; body: string } {
  return {
    title: `New from ${event.author_display_name}`,
    // The post title is the useful part; a subscriber-only post is still
    // announced (the teaser is the point - see CLAUDE.md's Latest-feed
    // section) but says so, since it can't be opened yet.
    body: event.locked ? `${event.title} (subscribers only)` : event.title,
  };
}

/// Starts listening. Returns an unsubscribe function so a React effect can
/// return it directly.
export function startNewPostNotifications(): () => void {
  return delegate.on("new_post", (event: DelegateEvent) => {
    if (event.event !== "new_post") return;
    const { title, body } = notificationText(event);
    // Deliberately fire-and-forget with a logged failure: a toast that the
    // OS refused (notifications muted, focus assist, an unpackaged dev build
    // Windows won't toast for) is cosmetic. The post itself is already in
    // the delegate's durable cache and will show up in Home regardless.
    void showOsNotification(title, body).catch((err) => {
      console.error("could not show a desktop notification", err);
    });
  });
}
