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
  // Set only right after a brand-new identity is created, so the
  // save-your-passphrase prompt below can render in its place - this is the
  // one and only moment the passphrase is ever shown again (see the
  // no-recovery warning above), never stored past this component's state.
  const [justCreatedPassphrase, setJustCreatedPassphrase] = useState<string | null>(null);

  const mismatch = !hasExistingIdentity && confirm.length > 0 && passphrase !== confirm;
  const canSubmit =
    passphrase.length > 0 && (hasExistingIdentity || (confirm.length > 0 && passphrase === confirm));

  async function submit() {
    if (!canSubmit || busy) return;
    setBusy(true);
    setError(null);
    try {
      await delegate.unlock(passphrase);
      if (hasExistingIdentity) {
        onUnlocked();
      } else {
        setJustCreatedPassphrase(passphrase);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setBusy(false);
    }
  }

  function downloadPassphrase() {
    if (!justCreatedPassphrase) return;
    const contents =
      `Aetheria passphrase\n` +
      `Saved: ${new Date().toISOString()}\n\n` +
      `${justCreatedPassphrase}\n\n` +
      `Keep this file somewhere private and offline (not a cloud-synced folder) -\n` +
      `there is no way to recover your identity if this passphrase is lost.\n`;
    const blob = new Blob([contents], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "aetheria-passphrase.txt";
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  }

  if (justCreatedPassphrase) {
    return (
      <div className="min-h-screen bg-ink-950 text-neutral-200 flex items-center justify-center px-4">
        <div className="w-full max-w-sm rounded-xl border border-ink-700 bg-ink-900 p-6 shadow-2xl">
          <h2 className="text-lg font-semibold text-neutral-100 mb-1">
            Save your passphrase
          </h2>
          <p className="text-sm text-neutral-400 mb-4">
            This is the only time Aetheria shows it to you. If it's lost,
            your identity can't be recovered - no one, including us, can
            reset it.
          </p>
          <div className="w-full rounded-lg bg-ink-950 border border-ink-700 p-2.5 text-sm text-neutral-200 mb-4 font-mono break-all select-all">
            {justCreatedPassphrase}
          </div>
          <button
            onClick={downloadPassphrase}
            className="w-full rounded-lg border border-ink-700 text-neutral-200 text-sm font-semibold py-2.5 mb-2 hover:bg-ink-800 transition"
          >
            Download as .txt
          </button>
          <button
            onClick={onUnlocked}
            className="w-full rounded-lg bg-aetheria-gradient text-white text-sm font-semibold py-2.5 shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition"
          >
            Continue to Aetheria
          </button>
        </div>
      </div>
    );
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
