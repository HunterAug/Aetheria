import { useState } from "react";
import { delegate } from "../lib/delegate";

type Status =
  | { kind: "idle" }
  | { kind: "publishing" }
  | { kind: "success"; postId: string }
  | { kind: "error"; message: string };

const inputClass =
  "w-full rounded-lg bg-ink-900 border border-ink-700 p-2.5 text-sm text-neutral-200 placeholder:text-neutral-500 focus:outline-none focus:ring-2 focus:ring-aeblue-500/50 focus:border-aeblue-500";

export default function Editor() {
  const [title, setTitle] = useState("");
  const [summary, setSummary] = useState("");
  const [markdown, setMarkdown] = useState("");
  const [status, setStatus] = useState<Status>({ kind: "idle" });

  async function publish() {
    setStatus({ kind: "publishing" });
    try {
      const { post_id } = await delegate.publishPost({
        title,
        summary,
        markdown,
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
    <div className="px-6 py-5 max-w-2xl mx-auto">
      <h2 className="text-xl font-semibold text-neutral-100 mb-1">Draft</h2>
      <p className="text-sm text-neutral-500 mb-5">
        Publishes to your local Delegate, then syncs to the real Freenet
        network. Every post is public.
      </p>

      <div className="space-y-3">
        <input
          className={inputClass}
          placeholder="Title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
        />
        <input
          className={inputClass}
          placeholder="One-line summary (shown unencrypted in the feed)"
          value={summary}
          onChange={(e) => setSummary(e.target.value)}
        />
        <textarea
          className={`${inputClass} h-64 font-mono`}
          placeholder="Write your article in Markdown..."
          value={markdown}
          onChange={(e) => setMarkdown(e.target.value)}
        />

        <div className="flex items-center justify-end pt-1">
          <button
            onClick={publish}
            disabled={!canPublish || status.kind === "publishing"}
            className="px-5 py-2 rounded-lg bg-aetheria-gradient text-white text-sm font-semibold shadow-lg shadow-aeblue-600/20 hover:brightness-110 transition disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {status.kind === "publishing" ? "Publishing…" : "Publish"}
          </button>
        </div>

        {status.kind === "success" && (
          <p className="text-sm text-aecyan-400">
            Published (post {status.postId.slice(0, 8)}…). Check the Home tab.
          </p>
        )}
        {status.kind === "error" && (
          <p className="text-sm text-red-400">{status.message}</p>
        )}
      </div>
    </div>
  );
}
