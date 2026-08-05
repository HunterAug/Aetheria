// Shared author-avatar rendering: an actual `<img>` once the image has been
// fetched from the network by its `author_avatar_freenet_key`, falling back
// to the existing gradient-initial placeholder until then (or forever, for
// an author who's never set one). Used by every place a post's author
// currently renders an avatar - FeedItemsList.tsx, PublisherProfileView.tsx,
// OpenedPostView.tsx - so the fetch-once-render-everywhere behavior below
// stays in one place instead of being reimplemented per call site.
//
// Own posts and someone else's are fetched the exact same way: an avatar is
// published `Public` (see `contracts::publish_avatar_to_network`), so
// there's no access distinction to special-case here - only whether a key is
// present at all (`author_avatar_freenet_key` is `null` for an author who's
// never set one, or - for someone else's post - whose profile this delegate
// has never viewed yet, see `handle_get_latest_feed`'s module docs).

import { useEffect, useState } from "react";
import { delegate } from "../lib/delegate";
import { initial } from "../lib/format";

/// Module-level so every `Avatar` instance for the same key (e.g. the same
/// author appearing in several feed cards) shares one fetch and one cached
/// result instead of each mounting its own network round trip.
const dataUrlCache = new Map<string, string>();
const inFlight = new Map<string, Promise<string>>();

function fetchAvatar(key: string): Promise<string> {
  const cached = dataUrlCache.get(key);
  if (cached) return Promise.resolve(cached);
  let promise = inFlight.get(key);
  if (!promise) {
    promise = delegate
      .getRemoteAvatar(key)
      .then((result) => {
        dataUrlCache.set(key, result.avatar_data_url);
        inFlight.delete(key);
        return result.avatar_data_url;
      })
      .catch((err) => {
        inFlight.delete(key);
        throw err;
      });
    inFlight.set(key, promise);
  }
  return promise;
}

const SIZES = {
  sm: { box: "w-9 h-9", text: "text-sm" },
  md: { box: "w-16 h-16", text: "text-xl" },
  xs: { box: "w-7 h-7", text: "text-xs" },
} as const;

export default function Avatar({
  name,
  avatarFreenetKey,
  size,
  shrink = true,
}: {
  name: string;
  avatarFreenetKey: string | null | undefined;
  size: keyof typeof SIZES;
  shrink?: boolean;
}) {
  const [dataUrl, setDataUrl] = useState<string | null>(
    avatarFreenetKey ? dataUrlCache.get(avatarFreenetKey) ?? null : null,
  );

  useEffect(() => {
    if (!avatarFreenetKey) {
      setDataUrl(null);
      return;
    }
    const cached = dataUrlCache.get(avatarFreenetKey);
    if (cached) {
      setDataUrl(cached);
      return;
    }
    let cancelled = false;
    fetchAvatar(avatarFreenetKey)
      .then((url) => {
        if (!cancelled) setDataUrl(url);
      })
      .catch(() => {
        // Not reachable (network hiccup, or never actually published) -
        // falls back to the initial below, same as an author with no
        // avatar at all. Not surfaced as an error: a missing/failed avatar
        // is cosmetic, not something worth interrupting the feed for.
      });
    return () => {
      cancelled = true;
    };
  }, [avatarFreenetKey]);

  const { box, text } = SIZES[size];
  const shrinkClass = shrink ? "shrink-0" : "";

  if (dataUrl) {
    return (
      <img
        src={dataUrl}
        alt=""
        className={`${box} rounded-full object-cover ${shrinkClass}`}
      />
    );
  }
  return (
    <div
      className={`${box} rounded-full bg-aetheria-gradient flex items-center justify-center ${text} font-semibold text-white ${shrinkClass}`}
    >
      {initial(name)}
    </div>
  );
}
