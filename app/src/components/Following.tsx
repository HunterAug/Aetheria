import { useEffect, useState } from "react";
import { delegate, type FollowedPublisher } from "../lib/delegate";
import { shortHex } from "../lib/format";

const inputClass =
  "w-full rounded-lg bg-ink-900 border border-ink-700 p-2.5 text-sm text-neutral-200 placeholder:text-neutral-500 focus:outline-none focus:ring-2 focus:ring-aeblue-500/50 focus:border-aeblue-500";

type FollowStatus =
  | { kind: "idle" }
  | { kind: "following" }
  | { kind: "error"; message: string };

/// Follow-management only - the followed feed itself now lives on the Home
/// tab (`get_following_feed`, see App.tsx), so this doesn't duplicate it.
/// The paste-a-pubkey box here is the only way to follow a publisher you
/// haven't already seen a post from; `PublisherProfileView.tsx`'s Follow
/// button (reached by clicking an author's name in any feed) is the other,
/// more common path.
export default function Following({
  onViewAuthor,
}: {
  onViewAuthor: (authorPubkey: string) => void;
}) {
  const [followed, setFollowed] = useState<FollowedPublisher[] | null>(null);
  const [pubkeyInput, setPubkeyInput] = useState("");
  const [followStatus, setFollowStatus] = useState<FollowStatus>({ kind: "idle" });
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    setError(null);
    try {
      setFollowed(await delegate.listFollowedPublishers());
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function follow() {
    const pubkey = pubkeyInput.trim();
    if (!pubkey) return;
    setFollowStatus({ kind: "following" });
    try {
      await delegate.followPublisher(pubkey);
      setPubkeyInput("");
      setFollowStatus({ kind: "idle" });
      await refresh();
    } catch (err) {
      setFollowStatus({
        kind: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  async function unfollow(authorPubkey: string) {
    try {
      await delegate.unfollowPublisher(authorPubkey);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <div className="max-w-2xl mx-auto">
      <div className="px-6 py-4 border-b border-ink-800">
        <h2 className="text-base font-semibold text-neutral-100 mb-3">Following</h2>
        <div className="flex gap-2">
          <input
            className={inputClass}
            placeholder="Publisher's pubkey (hex)"
            value={pubkeyInput}
            onChange={(e) => setPubkeyInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && follow()}
          />
          <button
            onClick={follow}
            disabled={!pubkeyInput.trim() || followStatus.kind === "following"}
            className="px-4 py-2 rounded-lg bg-aetheria-gradient text-white text-sm font-semibold shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition disabled:opacity-40 disabled:cursor-not-allowed whitespace-nowrap"
          >
            {followStatus.kind === "following" ? "Following…" : "Follow"}
          </button>
        </div>
        {followStatus.kind === "error" && (
          <p className="text-sm text-red-400 mt-2">{followStatus.message}</p>
        )}
        <p className="text-xs text-neutral-600 mt-2">
          Click a publisher's name from the Home, Latest, or another post's author to follow
          them from their profile - or paste their Ed25519 pubkey (the "Your publication key"
          hex string shown on their Subscribers tab) here if you don't have a post of theirs
          to click yet.
        </p>
      </div>

      {error && <p className="text-sm text-red-400 px-6 pt-4">{error}</p>}

      {followed === null && !error && (
        <p className="text-sm text-neutral-500 px-6 py-8">Loading…</p>
      )}
      {followed?.length === 0 && (
        <p className="text-sm text-neutral-500 px-6 py-8">
          You're not following anyone yet — paste a publisher's pubkey above to get started.
        </p>
      )}

      {followed && followed.length > 0 && (
        <div className="px-6 py-4 space-y-2">
          {followed.map((f) => (
            <div
              key={f.author_pubkey}
              className="flex items-center justify-between rounded-lg border border-ink-700 bg-ink-900 p-3"
            >
              <button
                onClick={() => onViewAuthor(f.author_pubkey)}
                className="min-w-0 text-left"
              >
                <p className="text-sm font-medium text-neutral-200 truncate hover:underline">
                  {f.display_name}
                </p>
                <p className="text-xs text-neutral-500 font-mono">{shortHex(f.author_pubkey)}</p>
              </button>
              <button
                onClick={() => unfollow(f.author_pubkey)}
                className="text-xs text-neutral-500 hover:text-red-400 whitespace-nowrap"
              >
                Unfollow
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
