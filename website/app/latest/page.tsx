import fs from "node:fs";
import path from "node:path";
import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Latest posts | Aetheria",
  description: "Recent posts published on the real Aetheria/Freenet network, read-only.",
};

// Refresh the page's static content periodically without a full redeploy -
// Vercel will re-render this route in the background at most this often.
// The underlying data file itself is only ever updated by a scheduled
// GitHub Action commit (see .github/workflows/refresh-latest-feed.yml) or a
// manual run of `cargo run --release --bin snapshot-latest-feed` from
// delegate/ - this page just displays whatever's currently on disk.
export const revalidate = 900;

interface FeedEntry {
  post_id: string;
  author_pubkey: string;
  author_display_name: string;
  title: string;
  summary: string;
  post_contract_id: string;
  access_level: "public" | "subscriber";
  locked: boolean;
  published_at: number;
}

interface Snapshot {
  generated_at: number;
  entries: FeedEntry[];
}

function readSnapshot(): Snapshot | null {
  try {
    const file = path.join(process.cwd(), "public", "data", "latest-feed.json");
    return JSON.parse(fs.readFileSync(file, "utf-8"));
  } catch {
    return null;
  }
}

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
  return `${Math.max(1, Math.floor(value))}${suffix} ago`;
}

function initial(name: string): string {
  return name.trim().charAt(0).toUpperCase() || "A";
}

export default function Latest() {
  const snapshot = readSnapshot();

  return (
    <div className="max-w-2xl mx-auto px-6 py-16">
      <h1 className="text-3xl font-bold text-neutral-50">Latest posts</h1>
      <p className="mt-3 text-neutral-400 leading-relaxed">
        Real posts from the real Aetheria network, browsable here without
        installing anything. This page is read-only. You can look, but
        publishing, following, and subscribing all require the app itself.
      </p>
      {snapshot && (
        <p className="mt-2 text-xs text-neutral-600">
          Snapshot updated{" "}
          {new Date(snapshot.generated_at * 1000).toLocaleString()}, refreshed
          periodically, not real-time.
        </p>
      )}

      <div className="mt-10">
        {!snapshot && (
          <p className="text-sm text-neutral-500">
            No snapshot available yet. Check back soon.
          </p>
        )}
        {snapshot && snapshot.entries.length === 0 && (
          <p className="text-sm text-neutral-500">
            Nothing published yet. Be the first to{" "}
            <a href="/download" className="text-aeblue-400 hover:underline">
              download Aetheria
            </a>
            .
          </p>
        )}
        <ul className="divide-y divide-ink-800 border-t border-b border-ink-800">
          {snapshot?.entries.map((item) => (
            <li key={`${item.author_pubkey}-${item.post_id}`} className="py-4 flex gap-3">
              <div className="w-9 h-9 rounded-full aetheria-gradient flex items-center justify-center text-sm font-semibold text-white shrink-0">
                {initial(item.author_display_name)}
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2 text-sm flex-wrap">
                  <span className="font-semibold text-neutral-100">
                    {item.author_display_name}
                  </span>
                  <span className="text-neutral-500">
                    {relativeTime(item.published_at)}
                  </span>
                  {item.locked && (
                    <span className="text-xs bg-aepurple-500/15 text-aepurple-400 px-1.5 py-0.5 rounded">
                      Subscriber-only
                    </span>
                  )}
                </div>
                <p
                  className={`text-sm font-medium mt-0.5 ${
                    item.locked ? "text-neutral-500" : "text-neutral-200"
                  }`}
                >
                  {item.title}
                </p>
                <p className="text-sm text-neutral-400 mt-0.5">{item.summary}</p>
                {item.locked && (
                  <p className="text-xs text-neutral-600 mt-1">
                    Full content is encrypted for subscribers. This preview
                    is all anyone else ever sees.
                  </p>
                )}
              </div>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
