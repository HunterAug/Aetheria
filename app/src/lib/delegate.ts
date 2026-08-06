// Typed client for the local Delegate daemon's WebSocket IPC (see
// delegate/src/ipc.rs). The UI never touches key material, ciphertext, or
// Freenet contract calls directly: everything goes through this loopback
// socket, request/response pairs correlated by a generated `id`.

const DELEGATE_IPC_URL = "ws://127.0.0.1:47021";

export interface PostSummary {
  post_id: string;
  title: string;
  summary: string;
  published_at: number;
}

export interface PostDetail {
  post_id: string;
  title: string;
  markdown: string;
}

export interface Profile {
  display_name: string;
  bio: string;
  /// A `data:<mime>;base64,...` URL, or `null` if no avatar has been set.
  avatar_data_url: string | null;
  /// Encoded contract id of the avatar's `PostDataContract` instance on
  /// Freenet, or `null` if it hasn't reached the network yet.
  avatar_freenet_key: string | null;
  /// This delegate's own hex-encoded Ed25519 pubkey - what someone else
  /// pastes into their Following tab to follow you.
  author_pubkey: string;
}

export interface UpdateProfileResult extends Profile {
  network_synced: boolean;
  network_error: string | null;
}

/// One entry in the Home (following) feed, the Latest (network-wide) feed,
/// or a publisher profile's post list - fetched live over the real network
/// (see `delegate/src/ipc.rs`'s `handle_get_following_feed`/
/// `handle_get_latest_feed`/`handle_get_publisher_profile`, which all share
/// this same shape via `feed_item_json`).
export interface FeedItem {
  post_id: string;
  title: string;
  summary: string;
  published_at: number;
  /// Hex-encoded Ed25519 pubkey of whoever published this post.
  author_pubkey: string;
  author_display_name: string;
  /// The author's avatar, as a `PostDataContract` id fetchable via
  /// `getRemoteAvatar` - `null` if they've never set one (or, for someone
  /// else's post, if this delegate has never viewed their profile yet - see
  /// `handle_get_latest_feed`'s module docs on why that's a real gap, not a
  /// bug).
  author_avatar_freenet_key: string | null;
  is_own: boolean;
  /// The post's `PostDataContract` id, or `null` if it hasn't reached the
  /// network yet (own posts only - a remote post only ever appears here
  /// once it's already in the fetched index, so this is always set for
  /// `is_own: false` entries).
  post_contract_id: string | null;
}

export interface FollowedPublisher {
  author_pubkey: string;
  display_name: string;
  avatar_freenet_key: string | null;
  followed_at: number;
}

export interface FollowResult extends FollowedPublisher {
  bio: string;
}

export interface RemotePostDetail {
  post_contract_id: string;
  markdown: string;
}

/// Another publisher's profile page - their real, network-verified
/// `PublisherProfileContract` plus their recent posts, for viewing (and
/// optionally following) someone reached by clicking an author's name in
/// any feed.
export interface PublisherProfileData {
  author_pubkey: string;
  display_name: string;
  bio: string;
  avatar_freenet_key: string | null;
  is_own: boolean;
  is_following: boolean;
  posts: FeedItem[];
}

export interface LockStatus {
  locked: boolean;
  /// Whether an identity.key already exists on disk - lets the UI show a
  /// plain "unlock" form vs. a "create a passphrase" form (with confirm
  /// field) without guessing.
  has_existing_identity: boolean;
}

export interface UnlockResult {
  created_new_identity: boolean;
  already_unlocked: boolean;
}

/// Live health of the delegate's connection to the Freenet network - see
/// `delegate/src/ipc.rs`'s `handle_get_network_status`.
///
/// The distinction that matters: a local Freenet node can be running and
/// answering its API perfectly while being connected to **zero** peers, in
/// which case every feed looks empty and nothing publishes. That is
/// `"isolated"`, and it is indistinguishable from `"connected"` by any other
/// signal the UI has.
export interface NetworkStatus {
  /// - `connected` - the node holds at least one peer connection.
  /// - `isolated`  - the node is up and answering but has **no** peers.
  ///                 Commonly a VPN or a restrictive firewall breaking NAT
  ///                 hole-punching.
  /// - `unknown`   - the node didn't answer the status query (`query_error`).
  /// - `locked`    - the delegate hasn't been unlocked yet, so no Freenet
  ///                 connection exists yet. Not an error.
  state: "connected" | "isolated" | "unknown" | "locked";
  freenet_connected: boolean;
  /// Peers the node reports ring connections to. `null` only when the query
  /// itself failed - `0` is a real answer, not a missing one.
  peer_count: number | null;
  node_peer_id: string | null;
  /// Seconds since one of this delegate's own contract operations last
  /// succeeded - a second, independent signal from `peer_count` (which is
  /// the node's self-report). `null` if none has yet on this connection.
  last_successful_operation_secs_ago: number | null;
  /// The most recent contract-operation failure, if the last one failed.
  last_error: string | null;
  /// Why `peer_count` is `null`, if it is.
  query_error: string | null;
}

/// A post that's been opened and is ready to render - the shape
/// `ReaderFeed.tsx`/`Following.tsx` build once they have both the feed
/// item's metadata (title, author) and the fetched markdown body.
export interface OpenedPost {
  post_id: string;
  title: string;
  markdown: string;
  author_pubkey: string;
  author_display_name: string;
  author_avatar_freenet_key: string | null;
  is_own: boolean;
}

/// A post from a publisher you follow, pushed by the delegate the moment it
/// learns about it - either from a real Freenet subscription push or from the
/// watcher's polling backstop (see `delegate/src/watcher.rs`). Unlike
/// everything else in this file this arrives *unprompted*: it carries an
/// `event` field and no `id`, which is exactly how the socket handler below
/// tells it apart from a reply to a request.
export interface NewPostEvent {
  event: "new_post";
  post_id: string;
  post_contract_id: string;
  title: string;
  summary: string;
  author_pubkey: string;
  author_display_name: string;
  published_at: number;
}

export type DelegateEvent = NewPostEvent;
export type DelegateEventName = DelegateEvent["event"];

interface PendingEntry {
  resolve: (value: unknown) => void;
  reject: (reason: unknown) => void;
}

/// How long to wait before rebuilding a dropped socket. Only applies once
/// something has registered an event listener (see `on`) - without one there
/// is nothing to keep a connection open *for*, and the next request opens one
/// anyway.
const RECONNECT_DELAY_MS = 3000;

class DelegateClient {
  private ws: WebSocket | null = null;
  private connecting: Promise<WebSocket> | null = null;
  private pending = new Map<string, PendingEntry>();
  private listeners = new Map<string, Set<(event: DelegateEvent) => void>>();
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  private connect(): Promise<WebSocket> {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      return Promise.resolve(this.ws);
    }
    if (this.connecting) return this.connecting;

    this.connecting = new Promise((resolve, reject) => {
      const socket = new WebSocket(DELEGATE_IPC_URL);
      socket.addEventListener("open", () => {
        this.ws = socket;
        this.connecting = null;
        resolve(socket);
      });
      socket.addEventListener("message", (event) => this.handleMessage(event));
      socket.addEventListener("error", () => {
        this.connecting = null;
        this.scheduleReconnect();
        reject(new Error("could not reach the Aetheria delegate. Is it running?"));
      });
      socket.addEventListener("close", () => {
        this.ws = null;
        this.scheduleReconnect();
      });
    });
    return this.connecting;
  }

  /// Keeps the push channel alive across a delegate restart (or any dropped
  /// socket) for as long as anything is listening for events. Requests don't
  /// need this - each one connects on demand - but a notification nobody is
  /// connected to receive is simply never delivered, so the listening case
  /// has to reconnect on its own.
  private scheduleReconnect() {
    if (this.listeners.size === 0 || this.reconnectTimer !== null) return;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect().catch(() => this.scheduleReconnect());
    }, RECONNECT_DELAY_MS);
  }

  /// Subscribes to server-push events. Returns an unsubscribe function, so a
  /// React effect can just `return delegate.on(...)`.
  on(event: DelegateEventName, handler: (event: DelegateEvent) => void): () => void {
    let handlers = this.listeners.get(event);
    if (!handlers) {
      handlers = new Set();
      this.listeners.set(event, handlers);
    }
    handlers.add(handler);
    // Make sure a socket actually exists - otherwise nothing would be
    // listening until the UI happened to make a request.
    void this.connect().catch(() => {
      /* reported through the reconnect loop, not here */
    });

    return () => {
      const set = this.listeners.get(event);
      if (!set) return;
      set.delete(handler);
      if (set.size === 0) this.listeners.delete(event);
    };
  }

  private handleMessage(event: MessageEvent<string>) {
    const msg = JSON.parse(event.data) as {
      id?: string;
      event?: DelegateEventName;
      result?: unknown;
      error?: string;
    };

    if (msg.event) {
      const handlers = this.listeners.get(msg.event);
      if (!handlers) return;
      for (const handler of handlers) {
        try {
          handler(msg as unknown as DelegateEvent);
        } catch (err) {
          // One bad listener must not stop the others, and must not take
          // down the socket's message handler.
          console.error("delegate event handler failed", err);
        }
      }
      return;
    }

    if (!msg.id) return;
    const entry = this.pending.get(msg.id);
    if (!entry) return;
    this.pending.delete(msg.id);
    if (msg.error) entry.reject(new Error(msg.error));
    else entry.resolve(msg.result);
  }

  private async call<T>(op: string, extra: Record<string, unknown> = {}): Promise<T> {
    const socket = await this.connect();
    const id = crypto.randomUUID();
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      socket.send(JSON.stringify({ id, op, ...extra }));
    });
  }

  /// Answerable even while the delegate is locked (see delegate/src/ipc.rs's
  /// module docs) - call this before anything else so the UI knows whether
  /// to show the unlock screen at all, and if so, which form (plain unlock
  /// vs. create-a-new-passphrase).
  lockStatus(): Promise<LockStatus> {
    return this.call("lock_status");
  }

  /// Creates a new identity under `passphrase` (if none exists yet) or
  /// unlocks the existing one. A wrong passphrase against an existing
  /// identity rejects with a retryable error, not a crash.
  unlock(passphrase: string): Promise<UnlockResult> {
    return this.call("unlock", { passphrase });
  }

  /// Live Freenet connectivity, asked of the local node itself (real peer
  /// count, not an inference). Answerable even while the delegate is locked,
  /// where it honestly reports `state: "locked"` rather than a fake
  /// connected/disconnected verdict. Safe to poll - the delegate answers it
  /// from a bounded, purely-local query against the node.
  getNetworkStatus(): Promise<NetworkStatus> {
    return this.call("get_network_status");
  }

  listPosts(): Promise<PostSummary[]> {
    return this.call("list_posts");
  }

  getPost(postId: string): Promise<PostDetail> {
    return this.call("get_post", { post_id: postId });
  }

  publishPost(input: {
    title: string;
    summary: string;
    markdown: string;
  }): Promise<{ post_id: string }> {
    return this.call("publish_post", input);
  }

  getProfile(): Promise<Profile> {
    return this.call("get_profile");
  }

  updateProfile(input: {
    display_name: string;
    bio: string;
    /// Omit (or pass `null`) to leave the currently-stored avatar unchanged.
    avatar_data_url?: string | null;
  }): Promise<UpdateProfileResult> {
    return this.call("update_profile", input);
  }

  /// Fetches and verifies `authorPubkey`'s real `PublisherProfileContract`
  /// over the network before saving anything locally - rejects with a clear
  /// error if no such publisher exists rather than blindly saving an
  /// unverified pubkey.
  followPublisher(authorPubkey: string): Promise<FollowResult> {
    return this.call("follow_publisher", { author_pubkey: authorPubkey });
  }

  unfollowPublisher(authorPubkey: string): Promise<{ author_pubkey: string }> {
    return this.call("unfollow_publisher", { author_pubkey: authorPubkey });
  }

  listFollowedPublishers(): Promise<FollowedPublisher[]> {
    return this.call("list_followed_publishers");
  }

  /// Every followed publisher's posts, sorted by recency - backs the Home
  /// tab.
  getFollowingFeed(): Promise<FeedItem[]> {
    return this.call("get_following_feed");
  }

  /// The most recent posts from *every* publisher on the network (own posts
  /// included), via the shared network-wide directory - backs the Latest
  /// tab. See CLAUDE.md's "Latest feed" section for what backs this (no
  /// discovery service existed before it).
  getLatestFeed(): Promise<FeedItem[]> {
    return this.call("get_latest_feed");
  }

  /// Fetches (and verifies) another publisher's profile plus their recent
  /// posts - the "view a publisher's profile page" screen reached by
  /// clicking an author's name in any feed.
  getPublisherProfile(authorPubkey: string): Promise<PublisherProfileData> {
    return this.call("get_publisher_profile", { author_pubkey: authorPubkey });
  }

  /// Opens a post from another publisher.
  getRemotePost(postContractId: string): Promise<RemotePostDetail> {
    return this.call("get_remote_post", { post_contract_id: postContractId });
  }

  /// Fetches an avatar image (own or any other publisher's, whichever
  /// `author_avatar_freenet_key` a `FeedItem`/`OpenedPost`/`PublisherProfileData`
  /// happens to carry) as a ready-to-render `data:` URL.
  getRemoteAvatar(avatarFreenetKey: string): Promise<{ avatar_data_url: string }> {
    return this.call("get_remote_avatar", { avatar_freenet_key: avatarFreenetKey });
  }
}

export const delegate = new DelegateClient();
