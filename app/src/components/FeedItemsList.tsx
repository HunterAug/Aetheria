// Shared rendering for a list of `FeedItem`s - used by both ReaderFeed.tsx's
// merged Home view and Following.tsx's followed-only view, so the per-card
// treatment stays in one place instead of drifting between the two feeds.

import type { FeedItem } from "../lib/delegate";
import { relativeTime } from "../lib/format";
import Avatar from "./Avatar";

export function FeedItemsList({
  items,
  opening,
  onOpen,
  onViewAuthor,
}: {
  items: FeedItem[];
  opening: string | null;
  onOpen: (item: FeedItem) => void;
  /// Omitted entirely for a context where there's nowhere sensible to send
  /// the click (there isn't one today - always passed - but kept optional so
  /// a future caller isn't forced to invent a no-op). Never called for
  /// `item.is_own` - the author name renders as plain text there instead of
  /// a button that would just navigate to yourself.
  onViewAuthor?: (authorPubkey: string) => void;
}) {
  return (
    <ul className="divide-y divide-ink-800">
      {items.map((item) => (
        <li
          key={`${item.author_pubkey}-${item.post_id}`}
          className="hover:bg-ink-900/60 transition-colors"
        >
          <div className="px-6 py-4 flex gap-3">
            <Avatar
              name={item.author_display_name}
              avatarFreenetKey={item.author_avatar_freenet_key}
              size="sm"
            />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 text-sm flex-wrap">
                {!item.is_own && onViewAuthor ? (
                  <button
                    onClick={() => onViewAuthor(item.author_pubkey)}
                    className="font-semibold text-neutral-100 hover:underline"
                  >
                    {item.author_display_name}
                  </button>
                ) : (
                  <span className="font-semibold text-neutral-100">
                    {item.author_display_name}
                  </span>
                )}
                {item.is_own && (
                  <span className="text-xs bg-aeblue-500/15 text-aeblue-400 px-1.5 py-0.5 rounded">
                    You
                  </span>
                )}
                <span className="text-neutral-500">{relativeTime(item.published_at)}</span>
                {opening === item.post_id && (
                  <span className="text-xs text-neutral-500">opening…</span>
                )}
              </div>
              <button
                onClick={() => onOpen(item)}
                disabled={opening === item.post_id}
                className="block w-full text-left disabled:cursor-not-allowed"
              >
                <p className="text-sm font-medium mt-0.5 text-neutral-200">{item.title}</p>
                <p className="text-sm text-neutral-400 mt-0.5 truncate">{item.summary}</p>
              </button>
            </div>
          </div>
        </li>
      ))}
    </ul>
  );
}
