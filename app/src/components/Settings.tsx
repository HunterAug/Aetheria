import { useEffect, useRef, useState } from "react";
import { delegate } from "../lib/delegate";

const inputClass =
  "w-full rounded-lg bg-ink-900 border border-ink-700 p-2.5 text-sm text-neutral-200 placeholder:text-neutral-500 focus:outline-none focus:ring-2 focus:ring-aeblue-500/50 focus:border-aeblue-500";

type Status =
  | { kind: "idle" }
  | { kind: "saving" }
  | { kind: "success" }
  | { kind: "partial"; message: string }
  | { kind: "error"; message: string };

export default function Settings() {
  const [displayName, setDisplayName] = useState("");
  const [bio, setBio] = useState("");
  const [avatarDataUrl, setAvatarDataUrl] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    (async () => {
      try {
        const profile = await delegate.getProfile();
        setDisplayName(profile.display_name);
        setBio(profile.bio);
        setAvatarDataUrl(profile.avatar_data_url);
      } catch (err) {
        setStatus({
          kind: "error",
          message: err instanceof Error ? err.message : String(err),
        });
      } finally {
        setLoaded(true);
      }
    })();
  }, []);

  function onPickAvatar(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => setAvatarDataUrl(reader.result as string);
    reader.readAsDataURL(file);
    // Allow re-picking the same file later (input value otherwise stays put).
    e.target.value = "";
  }

  async function save() {
    setStatus({ kind: "saving" });
    try {
      const result = await delegate.updateProfile({
        display_name: displayName,
        bio,
        avatar_data_url: avatarDataUrl,
      });
      setDisplayName(result.display_name);
      setBio(result.bio);
      setAvatarDataUrl(result.avatar_data_url);
      if (result.network_synced) {
        setStatus({ kind: "success" });
      } else {
        setStatus({
          kind: "partial",
          message: `Saved locally, not yet synced to the network (${
            result.network_error ?? "unknown error"
          }). It'll keep this device's copy either way.`,
        });
      }
    } catch (err) {
      setStatus({
        kind: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  const canSave = displayName.trim() !== "" && status.kind !== "saving";

  return (
    <div className="px-6 py-5 max-w-2xl mx-auto">
      <h2 className="text-xl font-semibold text-neutral-100 mb-1">Settings</h2>
      <p className="text-sm text-neutral-500 mb-5">
        Your display name and avatar are shown on every post you publish, and
        on your public profile. Readers can click through from a post to see
        it.
      </p>

      {!loaded ? (
        <p className="text-sm text-neutral-500">Loading…</p>
      ) : (
        <div className="space-y-4">
          <div className="flex items-center gap-4">
            <button
              onClick={() => fileInputRef.current?.click()}
              className="w-16 h-16 rounded-full bg-aetheria-gradient flex items-center justify-center text-xl font-semibold text-white overflow-hidden shrink-0"
              title="Change avatar"
            >
              {avatarDataUrl ? (
                <img
                  src={avatarDataUrl}
                  alt=""
                  className="w-full h-full object-cover"
                />
              ) : (
                (displayName.trim().charAt(0) || "A").toUpperCase()
              )}
            </button>
            <div>
              <button
                onClick={() => fileInputRef.current?.click()}
                className="text-sm text-aeblue-400 hover:text-aeblue-500"
              >
                {avatarDataUrl ? "Change avatar" : "Upload avatar"}
              </button>
              <input
                ref={fileInputRef}
                type="file"
                accept="image/*"
                onChange={onPickAvatar}
                className="hidden"
              />
            </div>
          </div>

          <input
            className={inputClass}
            placeholder="Display name"
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
          />
          <textarea
            className={`${inputClass} h-28`}
            placeholder="Short bio, shown on your profile page"
            value={bio}
            onChange={(e) => setBio(e.target.value)}
          />

          <div className="flex items-center justify-between pt-1">
            <div className="text-sm">
              {status.kind === "success" && (
                <span className="text-aecyan-400">Saved.</span>
              )}
              {status.kind === "partial" && (
                <span className="text-amber-400">{status.message}</span>
              )}
              {status.kind === "error" && (
                <span className="text-red-400">{status.message}</span>
              )}
            </div>
            <button
              onClick={save}
              disabled={!canSave}
              className="px-5 py-2 rounded-lg bg-aetheria-gradient text-white text-sm font-semibold shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition disabled:opacity-40 disabled:cursor-not-allowed"
            >
              {status.kind === "saving" ? "Saving…" : "Save"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
