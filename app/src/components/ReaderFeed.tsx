import { useEffect, useState } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { delegate, type PostDetail, type PostSummary } from "../lib/delegate";

const markdownComponents: Components = {
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
        <div className="max-w-none">
          <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
            {selected.markdown}
          </ReactMarkdown>
        </div>
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
