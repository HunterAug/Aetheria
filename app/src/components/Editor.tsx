import { useState } from "react";
import { delegate, type AccessLevel } from "../lib/delegate";

type Status =
  | { kind: "idle" }
  | { kind: "publishing" }
  | { kind: "success"; postId: string }
  | { kind: "error"; message: string };

export default function Editor() {
  const [title, setTitle] = useState("");
  const [summary, setSummary] = useState("");
  const [markdown, setMarkdown] = useState("");
  const [access, setAccess] = useState<AccessLevel>("public");
  const [status, setStatus] = useState<Status>({ kind: "idle" });

  async function publish() {
    setStatus({ kind: "publishing" });
    try {
      const { post_id } = await delegate.publishPost({
        title,
        summary,
        markdown,
        access,
      });
      setStatus({ kind: "success", postId: post_id });
      setTitle("");
      setSummary("");
      setMarkdown("");
    } catch (err) {
      setStatus({
        kind: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  const canPublish = title.trim() !== "" && markdown.trim() !== "";

  return (
    <div className="p-6 max-w-2xl">
      <h2 className="text-xl font-semibold mb-1">Draft</h2>
      <p className="text-sm text-neutral-500 mb-4">
        Publishes straight to the local Delegate's SQLite cache — subscriber
        posts are AES-256-GCM encrypted with this epoch's key. Freenet
        broadcast isn't wired up yet, so this only affects your local feed.
      </p>

      <div className="space-y-3">
        <input
          className="w-full rounded border border-neutral-300 p-2 text-sm"
          placeholder="Title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
        />
        <input
          className="w-full rounded border border-neutral-300 p-2 text-sm"
          placeholder="One-line summary (shown unencrypted in the feed)"
          value={summary}
          onChange={(e) => setSummary(e.target.value)}
        />
        <textarea
          className="w-full h-64 rounded border border-neutral-300 p-3 font-mono text-sm"
          placeholder="Write your article in Markdown..."
          value={markdown}
          onChange={(e) => setMarkdown(e.target.value)}
        />

        <div className="flex items-center justify-between">
          <label className="flex items-center gap-2 text-sm text-neutral-700">
            <input
              type="checkbox"
              checked={access === "subscriber"}
              onChange={(e) =>
                setAccess(e.target.checked ? "subscriber" : "public")
              }
            />
            Subscriber-only
          </label>

          <button
            onClick={publish}
            disabled={!canPublish || status.kind === "publishing"}
            className="px-4 py-1.5 rounded bg-neutral-900 text-white text-sm font-medium disabled:opacity-40"
          >
            {status.kind === "publishing" ? "Publishing…" : "Publish"}
          </button>
        </div>

        {status.kind === "success" && (
          <p className="text-sm text-green-700">
            Published (post {status.postId.slice(0, 8)}…) — check the Feed tab.
          </p>
        )}
        {status.kind === "error" && (
          <p className="text-sm text-red-700">{status.message}</p>
        )}
      </div>
    </div>
  );
}
