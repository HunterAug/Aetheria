import { useEffect, useRef, useState } from "react";
import { SearchIcon, LockIcon } from "./icons";
import { delegate, type FeedItem, type OpenedPost } from "../lib/delegate";
import { openFeedItem } from "../lib/feedItem";

interface PublisherResult {
  author_pubkey: string;
  display_name: string;
}

/// Real, functioning search - not a placeholder. There's no server-side
/// index (matches this app's "no discovery service beyond what's already
/// fetchable" philosophy elsewhere), so this searches over what's already
/// reachable: the Latest feed (every publisher's recent posts, network-wide,
/// up to its 1000-entry cap - see CLAUDE.md) plus locally-followed
/// publishers (so a publisher with zero posts yet is still findable by
/// name). Debounced 300ms after typing stops.
export default function RightRail({
  onOpenPost,
  onViewAuthor,
}: {
  onOpenPost: (post: OpenedPost) => void;
  onViewAuthor: (authorPubkey: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [posts, setPosts] = useState<FeedItem[]>([]);
  const [publishers, setPublishers] = useState<PublisherResult[]>([]);
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const [searching, setSearching] = useState(false);
  const [openingPostId, setOpeningPostId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const trimmed = query.trim();
    if (!trimmed) {
      setPosts([]);
      setPublishers([]);
      setDropdownOpen(false);
      return;
    }

    setSearching(true);
    setError(null);
    const handle = setTimeout(async () => {
      try {
        const [latest, followed] = await Promise.all([
          delegate.getLatestFeed(),
          delegate.listFollowedPublishers(),
        ]);
        const needle = trimmed.toLowerCase();

        const matchedPosts = latest.filter(
          (item) =>
            item.title.toLowerCase().includes(needle) ||
            item.summary.toLowerCase().includes(needle) ||
            item.author_display_name.toLowerCase().includes(needle),
        );

        const matchedPublishers: PublisherResult[] = followed
          .filter((f) => f.display_name.toLowerCase().includes(needle))
          .map((f) => ({ author_pubkey: f.author_pubkey, display_name: f.display_name }));
        const seenAuthors = new Set(matchedPublishers.map((p) => p.author_pubkey));
        for (const item of latest) {
          if (
            !item.is_own &&
            !seenAuthors.has(item.author_pubkey) &&
            item.author_display_name.toLowerCase().includes(needle)
          ) {
            matchedPublishers.push({
              author_pubkey: item.author_pubkey,
              display_name: item.author_display_name,
            });
            seenAuthors.add(item.author_pubkey);
          }
        }

        setPosts(matchedPosts.slice(0, 8));
        setPublishers(matchedPublishers.slice(0, 5));
        setDropdownOpen(true);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
        setDropdownOpen(true);
      } finally {
        setSearching(false);
      }
    }, 300);
    return () => clearTimeout(handle);
  }, [query]);

  useEffect(() => {
    function onClickOutside(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setDropdownOpen(false);
      }
    }
    document.addEventListener("mousedown", onClickOutside);
    return () => document.removeEventListener("mousedown", onClickOutside);
  }, []);

  async function selectPost(item: FeedItem) {
    if (item.locked) return;
    setOpeningPostId(item.post_id);
    setError(null);
    try {
      const opened = await openFeedItem(item);
      setQuery("");
      setDropdownOpen(false);
      onOpenPost(opened);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setOpeningPostId(null);
    }
  }

  function selectPublisher(authorPubkey: string) {
    setQuery("");
    setDropdownOpen(false);
    onViewAuthor(authorPubkey);
  }

  const hasResults = posts.length > 0 || publishers.length > 0;

  return (
    <aside className="w-72 shrink-0 hidden lg:block px-5 py-5 space-y-4">
      <div ref={containerRef} className="relative">
        <div className="flex items-center gap-2 rounded-full bg-ink-900 border border-ink-700 px-3.5 py-2 text-sm focus-within:ring-2 focus-within:ring-aeblue-500/50 focus-within:border-aeblue-500">
          <SearchIcon className="w-4 h-4 text-neutral-500 shrink-0" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onFocus={() => (posts.length > 0 || publishers.length > 0) && setDropdownOpen(true)}
            placeholder="Search Aetheria"
            className="bg-transparent outline-none text-neutral-200 placeholder:text-neutral-500 w-full min-w-0"
          />
        </div>

        {dropdownOpen && (
          <div className="absolute left-0 right-0 mt-2 rounded-xl border border-ink-700 bg-ink-900 shadow-2xl z-10 max-h-96 overflow-y-auto">
            {searching && <p className="text-sm text-neutral-500 px-4 py-3">Searching…</p>}
            {error && <p className="text-sm text-red-400 px-4 py-3">{error}</p>}
            {!searching && !error && !hasResults && (
              <p className="text-sm text-neutral-500 px-4 py-3">No matches.</p>
            )}
            {publishers.length > 0 && (
              <div className="py-1">
                <p className="px-4 pt-2 pb-1 text-xs font-semibold text-neutral-600 uppercase tracking-wide">
                  Publishers
                </p>
                {publishers.map((p) => (
                  <button
                    key={p.author_pubkey}
                    onClick={() => selectPublisher(p.author_pubkey)}
                    className="w-full text-left px-4 py-2 text-sm text-neutral-200 hover:bg-ink-800 truncate"
                  >
                    {p.display_name}
                  </button>
                ))}
              </div>
            )}
            {posts.length > 0 && (
              <div className="py-1 border-t border-ink-800">
                <p className="px-4 pt-2 pb-1 text-xs font-semibold text-neutral-600 uppercase tracking-wide">
                  Posts
                </p>
                {posts.map((item) => (
                  <button
                    key={`${item.author_pubkey}-${item.post_id}`}
                    onClick={() => selectPost(item)}
                    disabled={item.locked || openingPostId === item.post_id}
                    title={item.locked ? "Subscriber-only post - can't be opened yet" : undefined}
                    className="w-full text-left px-4 py-2 hover:bg-ink-800 disabled:cursor-not-allowed"
                  >
                    <p
                      className={`text-sm truncate flex items-center gap-1 ${
                        item.locked ? "text-neutral-500" : "text-neutral-200"
                      }`}
                    >
                      {item.locked && <LockIcon className="w-3 h-3 shrink-0" />}
                      {item.title}
                      {openingPostId === item.post_id && (
                        <span className="text-neutral-500">opening…</span>
                      )}
                    </p>
                    <p className="text-xs text-neutral-500 truncate">
                      {item.author_display_name}
                    </p>
                  </button>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      <div className="rounded-xl border border-ink-800 bg-ink-900 p-5">
        <img src="/logo.png" alt="" className="w-9 h-9 mb-3" />
        <h3 className="text-neutral-100 font-semibold mb-1">
          Sovereign by design
        </h3>
        <p className="text-sm text-neutral-400 leading-relaxed">
          Every post here is signed by your own keys and stored on Freenet:
          no publisher account, no platform that can pull it down.
        </p>
      </div>

      <div className="rounded-xl border border-ink-800 bg-ink-900 p-5 text-sm text-neutral-400 leading-relaxed">
        <h3 className="text-neutral-100 font-semibold mb-1">
          Local Delegate
        </h3>
        Reads and writes go through your local Delegate daemon on{" "}
        <code className="text-aecyan-400">127.0.0.1:47021</code>. Nothing
        leaves this machine except signed contract state.
      </div>
    </aside>
  );
}
