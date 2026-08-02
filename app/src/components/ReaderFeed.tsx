import { useEffect, useState } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  delegate,
  type PostDetail,
  type PostSummary,
  type Profile,
} from "../lib/delegate";

export const markdownComponents: Components = {
  h1: ({ children }) => (
    <h1 className="text-2xl font-bold text-neutral-100 mt-8 mb-3 first:mt-0">
      {children}
    </h1>
  ),
  h2: ({ children }) => (
    <h2 className="text-xl font-bold text-neutral-100 mt-7 mb-3 first:mt-0">
      {children}
    </h2>
  ),
  h3: ({ children }) => (
    <h3 className="text-lg font-semibold text-neutral-100 mt-6 mb-2 first:mt-0">
      {children}
    </h3>
  ),
  p: ({ children }) => (
    <p className="text-[15px] leading-relaxed text-neutral-300 mb-4">
      {children}
    </p>
  ),
  a: ({ children, href }) => (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      className="text-aeblue-400 hover:text-aeblue-500 underline underline-offset-2 decoration-aeblue-400/40 hover:decoration-aeblue-500"
    >
      {children}
    </a>
  ),
  strong: ({ children }) => (
    <strong className="font-semibold text-neutral-100">{children}</strong>
  ),
  em: ({ children }) => <em className="italic text-neutral-200">{children}</em>,
  del: ({ children }) => (
    <del className="text-neutral-500 line-through">{children}</del>
  ),
  ul: ({ children }) => (
    <ul className="list-disc list-outside pl-5 mb-4 space-y-1 text-[15px] leading-relaxed text-neutral-300">
      {children}
    </ul>
  ),
  ol: ({ children }) => (
    <ol className="list-decimal list-outside pl-5 mb-4 space-y-1 text-[15px] leading-relaxed text-neutral-300">
      {children}
    </ol>
  ),
  li: ({ children }) => <li className="pl-1">{children}</li>,
  blockquote: ({ children }) => (
    <blockquote className="border-l-2 border-aepurple-500/50 pl-4 my-4 text-neutral-400 italic">
      {children}
    </blockquote>
  ),
  hr: () => <hr className="border-ink-700 my-6" />,
  code: ({ className, children }) => {
    const isBlock = /language-/.test(className || "");
    if (isBlock) {
      return <code className={className}>{children}</code>;
    }
    return (
      <code className="px-1.5 py-0.5 rounded bg-ink-800 text-aecyan-400 text-[0.85em] font-mono">
        {children}
      </code>
    );
  },
  pre: ({ children }) => (
    <pre className="bg-ink-900 border border-ink-800 rounded-lg p-3.5 mb-4 overflow-x-auto text-[13px] leading-relaxed font-mono text-neutral-300">
      {children}
    </pre>
  ),
  table: ({ children }) => (
    <div className="overflow-x-auto mb-4">
      <table className="w-full text-[14px] text-left border-collapse">
        {children}
      </table>
    </div>
  ),
  thead: ({ children }) => (
    <thead className="border-b border-ink-700">{children}</thead>
  ),
  th: ({ children }) => (
    <th className="px-3 py-2 font-semibold text-neutral-100">{children}</th>
  ),
  td: ({ children }) => (
    <td className="px-3 py-2 border-t border-ink-800 text-neutral-300">
      {children}
    </td>
  ),
  img: ({ src, alt }) => (
    <img src={src} alt={alt} className="rounded-lg max-w-full my-4" />
  ),
};

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

function initial(name: string): string {
  return name.trim().charAt(0).toUpperCase() || "A";
}

export default function ReaderFeed({
  onOpenProfile,
}: {
  onOpenProfile: () => void;
}) {
  const [posts, setPosts] = useState<PostSummary[] | null>(null);
  const [profile, setProfile] = useState<Profile | null>(null);
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
    // The profile applies to every post (one publisher per delegate for
    // now) - fetch it once rather than per-card. A failure here just means
    // the feed falls back to the pre-profile initial-circle rendering.
    delegate
      .getProfile()
      .then(setProfile)
      .catch(() => {});
  }, []);

  const authorName = profile?.display_name?.trim() || "Untitled Publication";

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
      <div className="px-6 py-5 max-w-2xl mx-auto">
        <button
          onClick={() => setSelected(null)}
          className="text-sm text-neutral-500 hover:text-neutral-200 mb-5"
        >
          ← Back to feed
        </button>
        <button
          onClick={onOpenProfile}
          className="flex items-center gap-2 mb-3 group"
        >
          {profile?.avatar_data_url ? (
            <img
              src={profile.avatar_data_url}
              alt=""
              className="w-7 h-7 rounded-full object-cover"
            />
          ) : (
            <div className="w-7 h-7 rounded-full bg-aetheria-gradient flex items-center justify-center text-xs font-semibold text-white">
              {initial(authorName)}
            </div>
          )}
          <span className="text-sm font-semibold text-neutral-300 group-hover:underline">
            {authorName}
          </span>
        </button>
        <h2 className="text-2xl font-bold text-neutral-100 mb-4">
          {selected.title}
        </h2>
        <div className="max-w-none">
          <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
            {selected.markdown}
          </ReactMarkdown>
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-2xl mx-auto">
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
          <li
            key={post.post_id}
            className="hover:bg-ink-900/60 transition-colors"
          >
            <div className="px-6 py-4 flex gap-3">
              <button
                onClick={onOpenProfile}
                className="shrink-0"
                title={`View ${authorName}'s profile`}
              >
                {profile?.avatar_data_url ? (
                  <img
                    src={profile.avatar_data_url}
                    alt=""
                    className="w-9 h-9 rounded-full object-cover"
                  />
                ) : (
                  <div className="w-9 h-9 rounded-full bg-aetheria-gradient flex items-center justify-center text-sm font-semibold text-white">
                    {initial(authorName)}
                  </div>
                )}
              </button>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2 text-sm">
                  <button
                    onClick={onOpenProfile}
                    className="font-semibold text-neutral-100 hover:underline"
                  >
                    {authorName}
                  </button>
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
                <button
                  onClick={() => open(post.post_id)}
                  disabled={opening === post.post_id}
                  className="block w-full text-left"
                >
                  <p className="text-sm font-medium text-neutral-200 mt-0.5">
                    {post.title}
                  </p>
                  <p className="text-sm text-neutral-400 mt-0.5 truncate">
                    {post.summary}
                  </p>
                </button>
              </div>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
