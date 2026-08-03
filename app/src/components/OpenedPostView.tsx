// Shared "reading a single post" view - used by both ReaderFeed.tsx (own +
// followed posts from the Home feed) and Following.tsx (followed-only feed),
// factored out so the two feeds don't duplicate the markdown-rendering
// chrome around a post.

import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { markdownComponents } from "./ReaderFeed";
import type { OpenedPost } from "../lib/delegate";
import { initial } from "../lib/format";

export default function OpenedPostView({
  post,
  onBack,
  onOpenProfile,
}: {
  post: OpenedPost;
  onBack: () => void;
  /// Only meaningful for `post.is_own` - there's no profile view yet for a
  /// followed publisher (see CLAUDE.md's discovery-UI gap), so this is
  /// omitted entirely by `Following.tsx`.
  onOpenProfile?: () => void;
}) {
  const authorInner = (
    <>
      <div className="w-7 h-7 rounded-full bg-aetheria-gradient flex items-center justify-center text-xs font-semibold text-white">
        {initial(post.author_display_name)}
      </div>
      <span className="text-sm font-semibold text-neutral-300 group-hover:underline">
        {post.author_display_name}
      </span>
    </>
  );

  return (
    <div className="px-6 py-5 max-w-2xl mx-auto">
      <button
        onClick={onBack}
        className="text-sm text-neutral-500 hover:text-neutral-200 mb-5"
      >
        ← Back to feed
      </button>
      {post.is_own && onOpenProfile ? (
        <button onClick={onOpenProfile} className="flex items-center gap-2 mb-3 group">
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
