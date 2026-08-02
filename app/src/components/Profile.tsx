import { useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  delegate,
  type PostDetail,
  type PostSummary,
  type Profile as ProfileData,
} from "../lib/delegate";
import { markdownComponents } from "./ReaderFeed";

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

export default function Profile({
  onEditProfile,
}: {
  onEditProfile: () => void;
}) {
  const [profile, setProfile] = useState<ProfileData | null>(null);
  const [posts, setPosts] = useState<PostSummary[] | null>(null);
  const [selected, setSelected] = useState<PostDetail | null>(null);
  const [opening, setOpening] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    delegate.getProfile().then(setProfile).catch(() => {});
    delegate
      .listPosts()
      .then(setPosts)
      .catch((err) =>
        setError(err instanceof Error ? err.message : String(err)),
      );
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

  const displayName = profile?.display_name?.trim() || "Untitled Publication";
  const initial = (displayName.trim().charAt(0) || "A").toUpperCase();

  if (selected) {
    return (
      <div className="px-6 py-5 max-w-2xl mx-auto">
        <button
          onClick={() => setSelected(null)}
          className="text-sm text-neutral-500 hover:text-neutral-200 mb-5"
        >
          ← Back to profile
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
      <div className="px-6 py-6 border-b border-ink-800">
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-center gap-4">
            {profile?.avatar_data_url ? (
              <img
                src={profile.avatar_data_url}
                alt=""
                className="w-16 h-16 rounded-full object-cover shrink-0"
              />
            ) : (
              <div className="w-16 h-16 rounded-full bg-aetheria-gradient flex items-center justify-center text-xl font-semibold text-white shrink-0">
                {initial}
              </div>
            )}
            <div>
              <h2 className="text-xl font-bold text-neutral-100">
                {displayName}
              </h2>
              {profile?.bio && (
                <p className="text-sm text-neutral-400 mt-1 max-w-md">
                  {profile.bio}
                </p>
              )}
            </div>
          </div>
          <button
            onClick={onEditProfile}
            className="shrink-0 text-sm text-neutral-400 hover:text-neutral-200 border border-ink-700 rounded-lg px-3 py-1.5 hover:bg-ink-900 transition"
          >
            Edit profile
          </button>
        </div>
      </div>

      {error && <p className="text-sm text-red-400 px-6 pt-4">{error}</p>}

      {posts === null && !error && (
        <p className="text-sm text-neutral-500 px-6 py-8">Loading…</p>
      )}
      {posts?.length === 0 && (
        <p className="text-sm text-neutral-500 px-6 py-8">
          No posts published yet.
        </p>
      )}

      <ul className="divide-y divide-ink-800">
        {posts?.map((post) => (
          <li key={post.post_id}>
            <button
              onClick={() => open(post.post_id)}
              disabled={opening === post.post_id}
              className="w-full text-left px-6 py-4 hover:bg-ink-900/60 transition-colors"
            >
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
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
