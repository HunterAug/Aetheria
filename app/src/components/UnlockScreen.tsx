import { useState } from "react";
import { delegate } from "../lib/delegate";

/// Gates the entire app - rendered instead of everything else in App.tsx
/// while the delegate reports itself locked (see delegate/src/ipc.rs's
/// module docs on the locked/unlocked startup split). Unlike
/// FirstRunNamePrompt (an overlay on top of an already-rendered app), this
/// runs before any of that chrome exists, since nothing else can talk to the
/// delegate yet.
export default function UnlockScreen({
  hasExistingIdentity,
  onUnlocked,
}: {
  hasExistingIdentity: boolean;
  onUnlocked: () => void;
}) {
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const mismatch = !hasExistingIdentity && confirm.length > 0 && passphrase !== confirm;
  const canSubmit =
    passphrase.length > 0 && (hasExistingIdentity || (confirm.length > 0 && passphrase === confirm));

  async function submit() {
    if (!canSubmit || busy) return;
    setBusy(true);
    setError(null);
    try {
      await delegate.unlock(passphrase);
      onUnlocked();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setBusy(false);
    }
  }

  return (
    <div className="min-h-screen bg-ink-950 text-neutral-200 flex items-center justify-center px-4">
      <div className="w-full max-w-sm rounded-xl border border-ink-700 bg-ink-900 p-6 shadow-2xl">
        <h2 className="text-lg font-semibold text-neutral-100 mb-1">
          {hasExistingIdentity ? "Unlock Aetheria" : "Protect your Aetheria identity"}
        </h2>
        <p className="text-sm text-neutral-400 mb-4">
          {hasExistingIdentity
            ? "Enter your passphrase to unlock your keys."
            : "Choose a passphrase to encrypt your new identity. You'll need it every time Aetheria starts - there's no way to recover it if you forget it."}
        </p>
        <input
          autoFocus
          type="password"
          className="w-full rounded-lg bg-ink-950 border border-ink-700 p-2.5 text-sm text-neutral-200 placeholder:text-neutral-500 focus:outline-none focus:ring-2 focus:ring-aeblue-500/50 focus:border-aeblue-500 mb-3"
          placeholder="Passphrase"
          value={passphrase}
          onChange={(e) => setPassphrase(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && (hasExistingIdentity ? submit() : undefined)}
        />
        {!hasExistingIdentity && (
          <input
            type="password"
            className="w-full rounded-lg bg-ink-950 border border-ink-700 p-2.5 text-sm text-neutral-200 placeholder:text-neutral-500 focus:outline-none focus:ring-2 focus:ring-aeblue-500/50 focus:border-aeblue-500 mb-3"
            placeholder="Confirm passphrase"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submit()}
          />
        )}
        {mismatch && <p className="text-sm text-red-400 mb-3">Passphrases don't match.</p>}
        {error && <p className="text-sm text-red-400 mb-3">{error}</p>}
        <button
          onClick={submit}
          disabled={!canSubmit || busy}
          className="w-full rounded-lg bg-aetheria-gradient text-white text-sm font-semibold py-2.5 shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition disabled:opacity-40 disabled:cursor-not-allowed"
        >
          {busy ? "Unlocking…" : hasExistingIdentity ? "Unlock" : "Create identity"}
        </button>
      </div>
    </div>
  );
}
