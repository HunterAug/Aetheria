// Typed client for the local Delegate daemon's WebSocket IPC (see
// delegate/src/ipc.rs). The UI never touches key material, ciphertext, or
// Freenet contract calls directly — everything goes through this loopback
// socket, request/response pairs correlated by a generated `id`.

const DELEGATE_IPC_URL = "ws://127.0.0.1:47021";

export type AccessLevel = "public" | "subscriber";

export interface PostSummary {
  post_id: string;
  title: string;
  summary: string;
  access_level: AccessLevel;
  epoch_id: number;
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
}

export interface UpdateProfileResult extends Profile {
  network_synced: boolean;
  network_error: string | null;
}

export interface Tier {
  tier_id: number;
  name: string;
  price_sats_per_month: number;
  features: string[];
}

export interface SubscriptionInfo {
  /// This publication's Ed25519 pubkey (hex) - what a reader's Delegate
  /// would use to locate the `SubscriberRegistryContract` on Freenet.
  publisher_pubkey: string;
  /// This delegate's own secp256k1 identity pubkey (hex, compressed) - the
  /// `EncryptedKeyBundle.subscriber_pubkey` a bundle addressed to "you"
  /// would be keyed on.
  subscriber_pubkey: string;
  tiers: Tier[];
  wallet_connected: boolean;
}

export interface SubscribeResult {
  tier_id: number;
  epoch_id: number;
  preimage: string;
  network_synced: boolean;
  network_error: string | null;
}

export interface SubscriberEntry {
  subscriber_pubkey: string;
  epoch_id: number;
  issued_at: number;
}

interface PendingEntry {
  resolve: (value: unknown) => void;
  reject: (reason: unknown) => void;
}

class DelegateClient {
  private ws: WebSocket | null = null;
  private connecting: Promise<WebSocket> | null = null;
  private pending = new Map<string, PendingEntry>();

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
        reject(new Error("could not reach the Aetheria delegate — is it running?"));
      });
      socket.addEventListener("close", () => {
        this.ws = null;
      });
    });
    return this.connecting;
  }

  private handleMessage(event: MessageEvent<string>) {
    const msg = JSON.parse(event.data) as {
      id: string;
      result?: unknown;
      error?: string;
    };
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
    access: AccessLevel;
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

  /// Connect a Lightning wallet via Nostr Wallet Connect (NIP-47) - a
  /// `nostr+walletconnect://...` URI exported from a wallet such as Alby,
  /// Mutiny, Phoenix, or Umbrel.
  connectWallet(uri: string): Promise<{ connected: boolean }> {
    return this.call("connect_wallet", { uri });
  }

  getSubscriptionInfo(): Promise<SubscriptionInfo> {
    return this.call("get_subscription_info");
  }

  /// Pays for `tier_id` via the connected wallet, then (once settlement is
  /// verified) delivers an ECDH-encrypted epoch key bundle. Requires a
  /// wallet to already be connected via `connectWallet`.
  subscribe(tierId: number): Promise<SubscribeResult> {
    return this.call("subscribe", { tier_id: tierId });
  }

  listSubscribers(): Promise<SubscriberEntry[]> {
    return this.call("list_subscribers");
  }
}

export const delegate = new DelegateClient();
