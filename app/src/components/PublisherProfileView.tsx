import { useEffect, useState } from "react";
import {
  delegate,
  type FeedItem,
  type OpenedPost,
  type PublisherProfileData,
} from "../lib/delegate";
import { openFeedItem } from "../lib/feedItem";
import { FeedItemsList } from "./FeedItemsList";
import OpenedPostView from "./OpenedPostView";
import Avatar from "./Avatar";

/// Viewing another publisher's profile page - reached by clicking an
/// author's name in any feed. The Follow/Unfollow button here is the
/// primary way to follow someone in practice; `Following.tsx`'s paste-a-
/// pubkey box remains the only way to follow a publisher you haven't seen a
/// post from yet.
export default function PublisherProfileView({
  authorPubkey,
  onBack,
}: {
  authorPubkey: string;
  onBack: () => void;
}) {
  const [data, setData] = useState<PublisherProfileData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [followBusy, setFollowBusy] = useState(false);
  const [selected, setSelected] = useState<OpenedPost | null>(null);
  const [opening, setOpening] = useState<string | null>(null);

  useEffect(() => {
    setData(null);
    setError(null);
    delegate
      .getPublisherProfile(authorPubkey)
      .then(setData)
      .catch((err) => setError(err instanceof Error ? err.message : String(err)));
  }, [authorPubkey]);

  async function toggleFollow() {
    if (!data) return;
    setFollowBusy(true);
    setError(null);
    try {
      if (data.is_following) {
        await delegate.unfollowPublisher(authorPubkey);
      } else {
        await delegate.followPublisher(authorPubkey);
      }
      setData({ ...data, is_following: !data.is_following });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setFollowBusy(false);
    }
  }

  async function open(item: FeedItem) {
    if (item.locked) return;
    setOpening(item.post_id);
    setError(null);
    try {
      setSelected(await openFeedItem(item));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setOpening(null);
    }
  }

  if (selected) {
    return <OpenedPostView post={selected} onBack={() => setSelected(null)} />;
  }

  if (error && !data) {
    return (
      <div className="px-6 py-5 max-w-2xl mx-auto">
        <button onClick={onBack} className="text-sm text-neutral-500 hover:text-neutral-200 mb-5">
          ← Back
        </button>
        <p className="text-sm text-red-400">{error}</p>
      </div>
    );
  }

  return (
    <div className="max-w-2xl mx-auto">
      <div className="px-6 py-6 border-b border-ink-800">
        <button onClick={onBack} className="text-sm text-neutral-500 hover:text-neutral-200 mb-4">
          ← Back
        </button>
        {!data ? (
          <p className="text-sm text-neutral-500">Loading…</p>
        ) : (
          <div className="flex items-start justify-between gap-4">
            <div className="flex items-center gap-4">
              <Avatar
                name={data.display_name}
                avatarFreenetKey={data.avatar_freenet_key}
                size="md"
              />
              <div>
                <h2 className="text-xl font-bold text-neutral-100">{data.display_name}</h2>
                {data.bio && (
                  <p className="text-sm text-neutral-400 mt-1 max-w-md">{data.bio}</p>
                )}
              </div>
            </div>
            {!data.is_own && (
              <button
                onClick={toggleFollow}
                disabled={followBusy}
                className={`shrink-0 text-sm font-semibold rounded-lg px-4 py-1.5 transition disabled:opacity-40 disabled:cursor-not-allowed whitespace-nowrap ${
                  data.is_following
                    ? "text-neutral-400 border border-ink-700 hover:bg-ink-900"
                    : "bg-aetheria-gradient text-white shadow-lg shadow-aeblue-600/20 hover:brightness-110"
                }`}
              >
                {followBusy ? "…" : data.is_following ? "Following" : "Follow"}
              </button>
            )}
          </div>
        )}
      </div>

      {error && <p className="text-sm text-red-400 px-6 pt-4">{error}</p>}

      {data && data.posts.length === 0 && (
        <p className="text-sm text-neutral-500 px-6 py-8">No posts published yet.</p>
      )}
      {data && data.posts.length > 0 && (
        <FeedItemsList items={data.posts} opening={opening} onOpen={open} />
      )}
    </div>
  );
}
