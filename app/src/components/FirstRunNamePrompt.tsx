import { useState } from "react";
import { delegate } from "../lib/delegate";

export default function FirstRunNamePrompt({
  onDone,
}: {
  onDone: () => void;
}) {
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function save() {
    const trimmed = name.trim();
    if (!trimmed) return;
    setSaving(true);
    setError(null);
    try {
      await delegate.updateProfile({ display_name: trimmed, bio: "" });
      onDone();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setSaving(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm px-4">
      <div className="w-full max-w-sm rounded-xl border border-ink-700 bg-ink-900 p-6 shadow-2xl">
        <h2 className="text-lg font-semibold text-neutral-100 mb-1">
          Welcome to Aetheria
        </h2>
        <p className="text-sm text-neutral-400 mb-4">
          Choose a display name for your publication. You can change this
          anytime in Settings.
        </p>
        <input
          autoFocus
          className="w-full rounded-lg bg-ink-950 border border-ink-700 p-2.5 text-sm text-neutral-200 placeholder:text-neutral-500 focus:outline-none focus:ring-2 focus:ring-aeblue-500/50 focus:border-aeblue-500 mb-3"
          placeholder="Display name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && save()}
        />
        {error && <p className="text-sm text-red-400 mb-3">{error}</p>}
        <button
          onClick={save}
          disabled={!name.trim() || saving}
          className="w-full rounded-lg bg-aetheria-gradient text-white text-sm font-semibold py-2.5 shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition disabled:opacity-40 disabled:cursor-not-allowed"
        >
          {saving ? "Saving…" : "Get started"}
        </button>
      </div>
    </div>
  );
}
