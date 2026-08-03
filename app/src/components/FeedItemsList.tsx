// Shared rendering for a list of `FeedItem`s - used by both ReaderFeed.tsx's
// merged Home view and Following.tsx's followed-only view, so the "who
// published this, is it locked" per-card treatment stays in one place
// instead of drifting between the two feeds.

import type { FeedItem } from "../lib/delegate";
import { initial, relativeTime } from "../lib/format";
import { LockIcon } from "./icons";

export function FeedItemsList({
  items,
  opening,
  onOpen,
}: {
  items: FeedItem[];
  opening: string | null;
  onOpen: (item: FeedItem) => void;
}) {
  return (
    <ul className="divide-y divide-ink-800">
      {items.map((item) => (
        <li
          key={`${item.author_pubkey}-${item.post_id}`}
          className="hover:bg-ink-900/60 transition-colors"
        >
          <div className="px-6 py-4 flex gap-3">
            {/* TODO(later): fetch and render the followed publisher's real
                avatar image (`avatar_freenet_key`) instead of an initial -
                deferred to keep this pass scoped to text/metadata, which is
                all `PostMetadataHeader`/`FollowedPublisherRow` carry today. */}
            <div className="w-9 h-9 rounded-full bg-aetheria-gradient flex items-center justify-center text-sm font-semibold text-white shrink-0">
              {initial(item.author_display_name)}
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 text-sm flex-wrap">
                <span className="font-semibold text-neutral-100">
                  {item.author_display_name}
                </span>
                {item.is_own && (
                  <span className="text-xs bg-aeblue-500/15 text-aeblue-400 px-1.5 py-0.5 rounded">
                    You
                  </span>
                )}
                <span className="text-neutral-500">{relativeTime(item.published_at)}</span>
                {item.access_level === "subscriber" && (
                  <span className="text-xs bg-aepurple-500/15 text-aepurple-400 px-1.5 py-0.5 rounded flex items-center gap-1">
                    {item.locked && <LockIcon className="w-3 h-3" />}
                    Subscriber
                  </span>
                )}
                {opening === item.post_id && (
                  <span className="text-xs text-neutral-500">opening…</span>
                )}
              </div>
              <button
                onClick={() => onOpen(item)}
                disabled={item.locked || opening === item.post_id}
                title={item.locked ? "Subscriber-only post - can't be opened yet" : undefined}
                className="block w-full text-left disabled:cursor-not-allowed"
              >
                <p
                  className={`text-sm font-medium mt-0.5 ${
                    item.locked ? "text-neutral-500" : "text-neutral-200"
                  }`}
                >
                  {item.title}
                </p>
                <p className="text-sm text-neutral-400 mt-0.5 truncate">{item.summary}</p>
                {item.locked && (
                  <p className="text-xs text-neutral-600 mt-1 flex items-center gap-1">
                    <LockIcon className="w-3 h-3" />
                    Subscriber-only content from another publisher isn't unlockable yet
                  </p>
                )}
              </button>
            </div>
          </div>
        </li>
      ))}
    </ul>
  );
}
