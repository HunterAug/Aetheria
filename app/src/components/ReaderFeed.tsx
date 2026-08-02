import { useEffect, useState } from "react";
import { delegate, type PostDetail, type PostSummary } from "../lib/delegate";

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
      <div className="p-6 max-w-2xl">
        <button
          onClick={() => setSelected(null)}
          className="text-sm text-neutral-500 hover:underline mb-4"
        >
          ← Back to feed
        </button>
        <h2 className="text-xl font-semibold mb-4">{selected.title}</h2>
        <pre className="whitespace-pre-wrap font-sans text-sm leading-relaxed">
          {selected.markdown}
        </pre>
      </div>
    );
  }

  return (
    <div className="p-6 max-w-2xl">
      <div className="flex items-center justify-between mb-1">
        <h2 className="text-xl font-semibold">Feed</h2>
        <button onClick={refresh} className="text-sm text-neutral-500 hover:underline">
          Refresh
        </button>
      </div>
      <p className="text-sm text-neutral-500 mb-4">
        Posts fetched from the local Delegate's cache. Subscriber-only posts
        decrypt automatically here since this is your own publisher identity —
        a real subscriber's feed would only unlock what they've paid for.
      </p>

      {error && <p className="text-sm text-red-700 mb-3">{error}</p>}

      {posts === null && !error && (
        <p className="text-sm text-neutral-400">Loading…</p>
      )}
      {posts?.length === 0 && (
        <p className="text-sm text-neutral-400">
          No posts yet — write one in the Draft tab.
        </p>
      )}

      <ul className="divide-y divide-neutral-200">
        {posts?.map((post) => (
          <li key={post.post_id} className="py-3">
            <button
              onClick={() => open(post.post_id)}
              disabled={opening === post.post_id}
              className="w-full text-left"
            >
              <div className="flex items-center gap-2">
                <span className="font-medium">{post.title}</span>
                {post.access_level === "subscriber" && (
                  <span className="text-xs bg-amber-100 text-amber-800 px-1.5 py-0.5 rounded">
                    Subscriber
                  </span>
                )}
                {opening === post.post_id && (
                  <span className="text-xs text-neutral-400">opening…</span>
                )}
              </div>
              <p className="text-sm text-neutral-500">{post.summary}</p>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
