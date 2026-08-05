// Shared "reading a single post" view - used by both ReaderFeed.tsx (own +
// followed posts from the Home feed) and Following.tsx (followed-only feed),
// factored out so the two feeds don't duplicate the markdown-rendering
// chrome around a post.

import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { markdownComponents } from "./ReaderFeed";
import type { OpenedPost } from "../lib/delegate";
import Avatar from "./Avatar";

export default function OpenedPostView({
  post,
  onBack,
  onOpenProfile,
  onViewAuthor,
}: {
  post: OpenedPost;
  onBack: () => void;
  /// Only meaningful for `post.is_own` - navigates to this delegate's own
  /// Profile tab.
  onOpenProfile?: () => void;
  /// Only meaningful for `!post.is_own` - navigates to the author's
  /// publisher profile page.
  onViewAuthor?: (authorPubkey: string) => void;
}) {
  const authorInner = (
    <>
      <Avatar
        name={post.author_display_name}
        avatarFreenetKey={post.author_avatar_freenet_key}
        size="xs"
        shrink={false}
      />
      <span className="text-sm font-semibold text-neutral-300 group-hover:underline">
        {post.author_display_name}
      </span>
    </>
  );

  const authorClickHandler = post.is_own
    ? onOpenProfile
    : onViewAuthor
      ? () => onViewAuthor(post.author_pubkey)
      : undefined;

  return (
    <div className="px-6 py-5 max-w-2xl mx-auto">
      <button
        onClick={onBack}
        className="text-sm text-neutral-500 hover:text-neutral-200 mb-5"
      >
        ← Back to feed
      </button>
      {authorClickHandler ? (
        <button onClick={authorClickHandler} className="flex items-center gap-2 mb-3 group">
          {authorInner}
        </button>
      ) : (
        <div className="flex items-center gap-2 mb-3">{authorInner}</div>
      )}
      <h2 className="text-2xl font-bold text-neutral-100 mb-4">{post.title}</h2>
      <div className="max-w-none">
        <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
          {post.markdown}
        </ReactMarkdown>
      </div>
    </div>
  );
}
