import { useEffect, useState } from "react";
import { delegate, type PostDetail, type PostSummary } from "../lib/delegate";

function relativeTime(unixSeconds: number): string {
  const diffSec = Math.max(0, Date.now() / 1000 - unixSeconds);
  const units: [number, string][] = [
    [60, "s"],
    [60, "m"],
    [24, "h"],
    [365, "d"],
  ];
  let value = diffSec;
  let suffix = "s";
  for (const [size, label] of units) {
    if (value < size) {
      suffix = label;
      break;
    }
    value /= size;
    suffix = label;
  }
  return `${Math.max(1, Math.floor(value))}${suffix}`;
}

function initial(title: string): string {
  return title.trim().charAt(0).toUpperCase() || "A";
}

export default function ReaderFeed() {
  const [posts, setPosts] = useState<PostSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<PostDetail | null>(null);
  const [opening, setOpening] = useState<string | null>(null);

  async function refresh() {
    setError(null);
    try {
      setPosts(await delegate.listPosts());
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function open(postId: string) {
    setOpening(postId);
    setError(null);
    try {
      setSelected(await delegate.getPost(postId));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setOpening(null);
    }
  }

  if (selected) {
    return (
      <div className="px-6 py-5 max-w-2xl">
        <button
          onClick={() => setSelected(null)}
          className="text-sm text-neutral-500 hover:text-neutral-200 mb-5"
        >
          ← Back to feed
        </button>
        <h2 className="text-2xl font-bold text-neutral-100 mb-4">
          {selected.title}
        </h2>
        <pre className="whitespace-pre-wrap font-sans text-[15px] leading-relaxed text-neutral-300">
          {selected.markdown}
        </pre>
      </div>
    );
  }

  return (
    <div>
      <div className="flex items-center justify-between px-6 py-4 border-b border-ink-800">
        <h2 className="text-base font-semibold text-neutral-100">
          For you
        </h2>
        <button
          onClick={refresh}
          className="text-sm text-neutral-500 hover:text-neutral-200"
        >
          Refresh
        </button>
      </div>

      {error && <p className="text-sm text-red-400 px-6 pt-4">{error}</p>}

      {posts === null && !error && (
        <p className="text-sm text-neutral-500 px-6 py-8">Loading…</p>
      )}
      {posts?.length === 0 && (
        <p className="text-sm text-neutral-500 px-6 py-8">
          No posts yet — write one from the Draft tab.
        </p>
      )}

      <ul className="divide-y divide-ink-800">
        {posts?.map((post) => (
          <li key={post.post_id}>
            <button
              onClick={() => open(post.post_id)}
              disabled={opening === post.post_id}
              className="w-full text-left px-6 py-4 hover:bg-ink-900/60 transition-colors flex gap-3"
            >
              <div className="w-9 h-9 rounded-full bg-aetheria-gradient flex items-center justify-center text-sm font-semibold text-white shrink-0">
                {initial(post.title)}
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2 text-sm">
                  <span className="font-semibold text-neutral-100">
                    {post.title}
                  </span>
                  <span className="text-neutral-500">
                    {relativeTime(post.published_at)}
                  </span>
                  {post.access_level === "subscriber" && (
                    <span className="text-xs bg-aepurple-500/15 text-aepurple-400 px-1.5 py-0.5 rounded">
                      Subscriber
                    </span>
                  )}
                  {opening === post.post_id && (
                    <span className="text-xs text-neutral-500">opening…</span>
                  )}
                </div>
                <p className="text-sm text-neutral-400 mt-0.5 truncate">
                  {post.summary}
                </p>
              </div>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
