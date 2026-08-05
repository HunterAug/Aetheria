// Resolves a `FeedItem` (from any feed - Home, Latest, a publisher profile,
// or a search result) into an `OpenedPost` ready to render. Own posts read
// straight from the local cache; anyone else's fetches over the network.
// Shared so ReaderFeed.tsx, PublisherProfileView.tsx, and RightRail.tsx's
// search don't each reimplement the same is_own branch.

import { delegate, type FeedItem, type OpenedPost } from "./delegate";

export async function openFeedItem(item: FeedItem): Promise<OpenedPost> {
  if (item.is_own) {
    const detail = await delegate.getPost(item.post_id);
    return {
      post_id: detail.post_id,
      title: detail.title,
      markdown: detail.markdown,
      author_pubkey: item.author_pubkey,
      author_display_name: item.author_display_name,
      author_avatar_freenet_key: item.author_avatar_freenet_key,
      is_own: true,
    };
  }

  if (!item.post_contract_id) {
    throw new Error("this post hasn't synced to the network yet - try again later");
  }
  const detail = await delegate.getRemotePost(item.post_contract_id);
  return {
    post_id: item.post_id,
    title: item.title,
    markdown: detail.markdown,
    author_pubkey: item.author_pubkey,
    author_display_name: item.author_display_name,
    author_avatar_freenet_key: item.author_avatar_freenet_key,
    is_own: false,
  };
}
