import { useEffect, useState } from "react";
import type { Components } from "react-markdown";
import { delegate, type FeedItem, type OpenedPost } from "../lib/delegate";
import { FeedItemsList } from "./FeedItemsList";
import OpenedPostView from "./OpenedPostView";

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

/// Generic "list of posts, click to read one" screen - the same shape backs
/// both the Home tab (following feed) and the Latest tab (network-wide
/// feed), so this takes which fetch to run and what to call it rather than
/// being two near-duplicate components.
export default function ReaderFeed({
  title,
  fetchItems,
  emptyMessage,
  onOpenProfile,
  onViewAuthor,
}: {
  title: string;
  fetchItems: () => Promise<FeedItem[]>;
  emptyMessage: string;
  onOpenProfile: () => void;
  onViewAuthor: (authorPubkey: string) => void;
}) {
  const [items, setItems] = useState<FeedItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<OpenedPost | null>(null);
  const [opening, setOpening] = useState<string | null>(null);

  async function refresh() {
    setError(null);
    try {
      setItems(await fetchItems());
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  // `fetchItems` intentionally not in the dependency array - App.tsx remounts
  // this component (a fresh instance) whenever the tab switches, so a plain
  // once-on-mount effect is enough and avoids re-fetching on every parent
  // render just because an inline arrow function prop got a new identity.
  useEffect(() => {
    refresh();
  }, []);

  async function open(item: FeedItem) {
    if (item.locked) return;
    setOpening(item.post_id);
    setError(null);
    try {
      if (item.is_own) {
        const detail = await delegate.getPost(item.post_id);
        setSelected({
          post_id: detail.post_id,
          title: detail.title,
          markdown: detail.markdown,
          author_pubkey: item.author_pubkey,
          author_display_name: item.author_display_name,
          is_own: true,
        });
      } else {
        if (!item.post_contract_id) {
          throw new Error("this post hasn't synced to the network yet - try again later");
        }
        const detail = await delegate.getRemotePost(item.post_contract_id);
        setSelected({
          post_id: item.post_id,
          title: item.title,
          markdown: detail.markdown,
          author_pubkey: item.author_pubkey,
          author_display_name: item.author_display_name,
          is_own: false,
        });
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setOpening(null);
    }
  }

  if (selected) {
    return (
      <OpenedPostView
        post={selected}
        onBack={() => setSelected(null)}
        onOpenProfile={onOpenProfile}
        onViewAuthor={onViewAuthor}
      />
    );
  }

  return (
    <div className="max-w-2xl mx-auto">
      <div className="flex items-center justify-between px-6 py-4 border-b border-ink-800">
        <h2 className="text-base font-semibold text-neutral-100">{title}</h2>
        <button
          onClick={refresh}
          className="text-sm text-neutral-500 hover:text-neutral-200"
        >
          Refresh
        </button>
      </div>

      {error && <p className="text-sm text-red-400 px-6 pt-4">{error}</p>}

      {items === null && !error && (
        <p className="text-sm text-neutral-500 px-6 py-8">Loading…</p>
      )}
      {items?.length === 0 && (
        <p className="text-sm text-neutral-500 px-6 py-8">{emptyMessage}</p>
      )}

      {items && items.length > 0 && (
        <FeedItemsList
          items={items}
          opening={opening}
          onOpen={open}
          onViewAuthor={onViewAuthor}
        />
      )}
    </div>
  );
}
