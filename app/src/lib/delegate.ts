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
}

export const delegate = new DelegateClient();
