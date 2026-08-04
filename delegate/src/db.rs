//! Local SQLite cache: decrypted posts, subscriber records, and epoch keys
//! the Delegate has recovered so the UI can render feeds offline.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

pub struct LocalStore {
    // `rusqlite::Connection` uses internal `RefCell`s (statement cache etc.)
    // and so isn't `Sync` on its own, which would make `LocalStore` - and
    // anything holding it behind a shared reference - not `Sync` either.
    // Since `ipc.rs`'s `handle_publish_post` now awaits real network I/O
    // while holding `&Delegate`, the compiler requires `Delegate: Sync` for
    // the connection-handling task to be `Send`. A `std::sync::Mutex` here
    // is never held across an `.await` (every method below is a plain
    // synchronous rusqlite call), so this is a type-level fix, not a new
    // source of contention beyond what the outer `tokio::sync::Mutex<Delegate>`
    // already serializes.
    conn: Mutex<Connection>,
}

/// A row in the `posts` table, as needed to render a feed entry.
pub struct PostSummary {
    pub post_id: [u8; 16],
    pub title: String,
    pub summary: String,
    pub access_level: String,
    pub epoch_id: u32,
    pub published_at: u64,
    /// `None` until `publish_post_to_network` succeeds for this post (see
    /// `ipc.rs::handle_publish_post`) - the local SQLite write happens first
    /// and unconditionally, so a post can legitimately sit here with this
    /// still unset if the network side failed or hasn't been retried yet.
    /// Not an error state by itself; callers surface it as "not yet synced"
    /// rather than treating a `None` row as corrupt or incomplete.
    pub post_contract_id: Option<String>,
}

/// This delegate's own publication profile as cached locally for fast
/// rendering. Mirrors the pieces of the network's `PublisherProfile`
/// (`contracts.rs`) that the UI reads directly - the signed/pubkey/tier
/// fields stay in `contracts.rs`/`keys.rs`, not duplicated here.
pub struct ProfileRow {
    pub display_name: String,
    pub bio: String,
    /// Raw image bytes, stored locally for fast rendering regardless of
    /// whether the best-effort network publish (`contracts::publish_avatar_to_network`)
    /// has succeeded yet.
    pub avatar_bytes: Option<Vec<u8>>,
    pub avatar_mime: Option<String>,
    #[allow(dead_code)]
    pub updated_at: u64,
}

/// Full stored representation of a post, before decryption.
pub struct PostRow {
    pub title: String,
    pub access_level: String,
    pub epoch_id: u32,
    /// Present only for `access_level = "public"` posts.
    pub markdown_plain: Option<String>,
    /// Present only for `access_level = "subscriber"` posts.
    pub cipher_text: Option<Vec<u8>>,
    pub nonce: Option<Vec<u8>>,
    /// See `PostSummary::post_contract_id` - same "not yet synced" meaning.
    pub post_contract_id: Option<String>,
}

/// A locally-recorded grant of subscriber access - see `LocalStore::record_subscriber`.
pub struct SubscriberRow {
    /// Compressed SEC1 secp256k1 pubkey (33 bytes).
    pub subscriber_pubkey: Vec<u8>,
    pub epoch_id: u32,
    pub issued_at: u64,
}

/// One publisher's profile as last successfully fetched from the network -
/// see the "cached_remote_*" tables' module docs below for why this exists
/// alongside the live `contracts::fetch_remote_profile` call.
pub struct CachedRemoteProfile {
    pub display_name: String,
    pub bio: String,
    pub avatar_freenet_key: Option<String>,
}

/// One post as last successfully fetched from a publisher's `ContentIndexContract`
/// (`contracts::fetch_remote_posts`) or the shared `GlobalDirectoryContract`
/// (`contracts::fetch_global_directory`) - the same shape serves both since
/// `ipc.rs::feed_item_json` renders them identically either way.
pub struct CachedRemotePost {
    pub post_id: [u8; 16],
    pub author_pubkey: [u8; 32],
    pub author_display_name: String,
    pub title: String,
    pub summary: String,
    pub post_contract_id: String,
    /// `"public"` or `"subscriber"` - mirrors `feed_item_json`'s flattening
    /// of `AccessTier` rather than storing the enum's own serde shape, so
    /// reconstructing a feed item back out doesn't need this module to know
    /// `aetheria_types::AccessTier` at all.
    pub access_level: String,
    pub epoch_id: u32,
    pub published_at: u64,
}

/// A publisher this delegate has chosen to follow (see `contracts::fetch_remote_profile`
/// for how `display_name`/`avatar_freenet_key` were validated at follow time).
/// Cached locally so the Following tab and the merged Home feed render fast
/// without a network round trip just to show a name - the actual post list
/// still comes from a live fetch (see `contracts::fetch_remote_posts`), only
/// the publisher's identity/display metadata is cached here.
pub struct FollowedPublisherRow {
    /// Ed25519 master signing pubkey - same identity `ensure_publisher_identity`
    /// keys `content_index`/`publisher_profile` on for this delegate's own
    /// identity.
    pub author_pubkey: [u8; 32],
    pub display_name: String,
    pub avatar_freenet_key: Option<String>,
    pub followed_at: u64,
}

impl LocalStore {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS posts (
                post_id       BLOB PRIMARY KEY,
                title         TEXT NOT NULL,
                summary       TEXT NOT NULL,
                markdown      TEXT,
                cipher_text   BLOB,
                nonce         BLOB,
                access_level  TEXT NOT NULL,
                epoch_id      INTEGER NOT NULL,
                published_at  INTEGER NOT NULL,
                post_contract_id TEXT
            );

            CREATE TABLE IF NOT EXISTS epoch_keys (
                epoch_id      INTEGER PRIMARY KEY,
                key_bytes     BLOB NOT NULL,
                recovered_at  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS subscribers (
                subscriber_pubkey BLOB NOT NULL,
                epoch_id          INTEGER NOT NULL,
                issued_at         INTEGER NOT NULL,
                PRIMARY KEY (subscriber_pubkey, epoch_id)
            );

            CREATE TABLE IF NOT EXISTS contract_registry (
                role         TEXT PRIMARY KEY,
                instance_id  BLOB NOT NULL,
                code_hash    BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS profile (
                id            INTEGER PRIMARY KEY CHECK (id = 0),
                display_name  TEXT NOT NULL,
                bio           TEXT NOT NULL,
                avatar_bytes  BLOB,
                avatar_mime   TEXT,
                updated_at    INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS followed_publishers (
                author_pubkey      BLOB PRIMARY KEY,
                display_name       TEXT NOT NULL,
                avatar_freenet_key TEXT,
                followed_at        INTEGER NOT NULL
            );

            -- Durable local archive of everything this delegate has ever
            -- successfully fetched from the network: other publishers'
            -- profiles, their post headers (whether discovered via a
            -- followed publisher's own index or via the network-wide
            -- GlobalDirectoryContract), and the actual content of any post
            -- once opened. The real Freenet network only keeps a contract's
            -- state reachable for as long as some peer bothers to host it -
            -- these tables are what make "you saw it once" mean "you have it
            -- forever," independent of whether the network can still
            -- produce it on a later live fetch. Every write here is a
            -- same-key upsert (never deleted automatically) and every read
            -- is additive to whatever the live network returns this time -
            -- see `ipc.rs`'s feed handlers for how live results and this
            -- cache are merged.
            CREATE TABLE IF NOT EXISTS cached_remote_profiles (
                author_pubkey      BLOB PRIMARY KEY,
                display_name       TEXT NOT NULL,
                bio                TEXT NOT NULL,
                avatar_freenet_key TEXT,
                cached_at          INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS cached_remote_posts (
                post_contract_id     TEXT PRIMARY KEY,
                post_id              BLOB NOT NULL,
                author_pubkey        BLOB NOT NULL,
                author_display_name  TEXT NOT NULL,
                title                TEXT NOT NULL,
                summary              TEXT NOT NULL,
                access_level         TEXT NOT NULL,
                epoch_id             INTEGER NOT NULL,
                published_at         INTEGER NOT NULL,
                cached_at            INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS cached_post_payloads (
                post_contract_id TEXT PRIMARY KEY,
                markdown         TEXT NOT NULL,
                cached_at        INTEGER NOT NULL
            );

            -- Every post this delegate has already accounted for as far as
            -- desktop notifications are concerned (see `watcher.rs`). A row
            -- here means "do not toast about this post again" - either
            -- because it was toasted once already, or because it was
            -- deliberately absorbed silently the first time a publisher's
            -- index was read (otherwise following someone with 40 old posts,
            -- or simply restarting the app, would fire 40 toasts at once).
            -- Deliberately its own table rather than reusing
            -- `cached_remote_posts`: that one is written by every feed
            -- refresh as a rendering cache, so keying notifications off it
            -- would mean a feed the user never looked at silently suppressed
            -- the notification for a post they hadn't seen.
            CREATE TABLE IF NOT EXISTS notified_posts (
                post_id       BLOB PRIMARY KEY,
                author_pubkey BLOB NOT NULL,
                notified_at   INTEGER NOT NULL
            );
            "#,
        )?;
        // `profile`/`followed_publishers` are brand-new tables (no existing
        // on-disk DB predates them), so plain `CREATE TABLE IF NOT EXISTS`
        // above is enough - unlike `post_contract_id` below, there's no
        // pre-existing schema shape to guard against.

        // `CREATE TABLE IF NOT EXISTS` above is a no-op against a `posts`
        // table that already existed on disk from before this column was
        // added (e.g. this machine's real dev DB under
        // `%APPDATA%\aetheria\aetheria-delegate\data\`) - add it defensively
        // rather than assuming every on-disk DB is fresh.
        let has_post_contract_id = conn
            .prepare("SELECT post_contract_id FROM posts LIMIT 1")
            .is_ok();
        if !has_post_contract_id {
            conn.execute("ALTER TABLE posts ADD COLUMN post_contract_id TEXT", [])?;
        }

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Looks up a previously-published contract's network identity by role
    /// (e.g. `"content_index"`, `"publisher_profile"`) so a restart reuses
    /// the same contract instance instead of publishing a new one every time.
    /// Returns `(instance_id, code_hash)`, both raw 32-byte
    /// `freenet_stdlib` key components.
    pub fn get_contract_registration(&self, role: &str) -> Result<Option<([u8; 32], [u8; 32])>> {
        self.conn
            .lock()
            .expect("db mutex poisoned")
            .query_row(
                "SELECT instance_id, code_hash FROM contract_registry WHERE role = ?1",
                params![role],
                |row| {
                    let instance_id: Vec<u8> = row.get(0)?;
                    let code_hash: Vec<u8> = row.get(1)?;
                    Ok((instance_id, code_hash))
                },
            )
            .optional()?
            .map(|(instance_id, code_hash)| {
                let instance_id: [u8; 32] = instance_id
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("corrupt instance_id length"))?;
                let code_hash: [u8; 32] = code_hash
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("corrupt code_hash length"))?;
                Ok((instance_id, code_hash))
            })
            .transpose()
    }

    pub fn set_contract_registration(
        &self,
        role: &str,
        instance_id: &[u8],
        code_hash: &[u8],
    ) -> Result<()> {
        self.conn.lock().expect("db mutex poisoned").execute(
            "INSERT INTO contract_registry (role, instance_id, code_hash) VALUES (?1, ?2, ?3)
             ON CONFLICT(role) DO UPDATE SET instance_id = excluded.instance_id, code_hash = excluded.code_hash",
            params![role, instance_id, code_hash],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_post(
        &self,
        post_id: &[u8; 16],
        title: &str,
        summary: &str,
        access_level: &str,
        epoch_id: u32,
        published_at: u64,
        markdown_plain: Option<&str>,
        cipher_text: Option<&[u8]>,
        nonce: Option<&[u8; 12]>,
    ) -> Result<()> {
        self.conn.lock().expect("db mutex poisoned").execute(
            "INSERT INTO posts
                (post_id, title, summary, markdown, cipher_text, nonce, access_level, epoch_id, published_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                post_id.as_slice(),
                title,
                summary,
                markdown_plain,
                cipher_text,
                nonce.map(|n| n.as_slice()),
                access_level,
                epoch_id,
                published_at as i64,
            ],
        )?;
        Ok(())
    }

    pub fn list_posts(&self) -> Result<Vec<PostSummary>> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT post_id, title, summary, access_level, epoch_id, published_at, post_contract_id
             FROM posts ORDER BY published_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let post_id: Vec<u8> = row.get(0)?;
            let epoch_id: i64 = row.get(4)?;
            let published_at: i64 = row.get(5)?;
            Ok(PostSummary {
                post_id: post_id.try_into().unwrap_or([0u8; 16]),
                title: row.get(1)?,
                summary: row.get(2)?,
                access_level: row.get(3)?,
                epoch_id: epoch_id as u32,
                published_at: published_at as u64,
                post_contract_id: row.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn get_post(&self, post_id: &[u8; 16]) -> Result<Option<PostRow>> {
        self.conn
            .lock()
            .expect("db mutex poisoned")
            .query_row(
                "SELECT title, access_level, epoch_id, markdown, cipher_text, nonce, post_contract_id
                 FROM posts WHERE post_id = ?1",
                params![post_id.as_slice()],
                |row| {
                    let epoch_id: i64 = row.get(2)?;
                    Ok(PostRow {
                        title: row.get(0)?,
                        access_level: row.get(1)?,
                        epoch_id: epoch_id as u32,
                        markdown_plain: row.get(3)?,
                        cipher_text: row.get(4)?,
                        nonce: row.get(5)?,
                        post_contract_id: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    /// Records that `post_id`'s network publish (`PostDataContract` mint +
    /// `ContentIndexContract` fold, see `contracts::publish_post_to_network`)
    /// succeeded, so `list_posts`/`get_post` can report it as synced. Never
    /// called if the network publish failed - the row is simply left with
    /// `post_contract_id = NULL`, which is the "saved locally, not yet
    /// synced" state, not an error to clean up.
    pub fn set_post_contract_id(&self, post_id: &[u8; 16], post_contract_id: &str) -> Result<()> {
        self.conn.lock().expect("db mutex poisoned").execute(
            "UPDATE posts SET post_contract_id = ?1 WHERE post_id = ?2",
            params![post_contract_id, post_id.as_slice()],
        )?;
        Ok(())
    }

    /// Returns the key for `epoch_id`, generating and storing one if it
    /// doesn't exist yet (first post published in a new epoch).
    pub fn get_or_create_epoch_key(
        &self,
        epoch_id: u32,
        generate: impl FnOnce() -> [u8; 32],
        now: u64,
    ) -> Result<[u8; 32]> {
        if let Some(existing) = self.get_epoch_key(epoch_id)? {
            return Ok(existing);
        }
        let key = generate();
        self.conn.lock().expect("db mutex poisoned").execute(
            "INSERT INTO epoch_keys (epoch_id, key_bytes, recovered_at) VALUES (?1, ?2, ?3)",
            params![epoch_id, key.as_slice(), now as i64],
        )?;
        Ok(key)
    }

    /// Reads this delegate's locally-cached profile fields, or `None` if
    /// `set_profile` has never been called (fresh install, before the user
    /// has visited the Profile tab and saved anything) - callers fall back
    /// to the same placeholder `ensure_publisher_identity` publishes on
    /// first run (`"Untitled Publication"`, empty bio, no avatar).
    pub fn get_profile(&self) -> Result<Option<ProfileRow>> {
        self.conn
            .lock()
            .expect("db mutex poisoned")
            .query_row(
                "SELECT display_name, bio, avatar_bytes, avatar_mime, updated_at FROM profile WHERE id = 0",
                [],
                |row| {
                    let updated_at: i64 = row.get(4)?;
                    Ok(ProfileRow {
                        display_name: row.get(0)?,
                        bio: row.get(1)?,
                        avatar_bytes: row.get(2)?,
                        avatar_mime: row.get(3)?,
                        updated_at: updated_at as u64,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    /// Upserts the single-row local profile cache (singleton row `id = 0`).
    pub fn set_profile(
        &self,
        display_name: &str,
        bio: &str,
        avatar_bytes: Option<&[u8]>,
        avatar_mime: Option<&str>,
        updated_at: u64,
    ) -> Result<()> {
        self.conn.lock().expect("db mutex poisoned").execute(
            "INSERT INTO profile (id, display_name, bio, avatar_bytes, avatar_mime, updated_at)
             VALUES (0, ?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                display_name = excluded.display_name,
                bio = excluded.bio,
                avatar_bytes = excluded.avatar_bytes,
                avatar_mime = excluded.avatar_mime,
                updated_at = excluded.updated_at",
            params![display_name, bio, avatar_bytes, avatar_mime, updated_at as i64],
        )?;
        Ok(())
    }

    /// Records that this publisher just issued (or re-issued) an
    /// `EncryptedKeyBundle` to `subscriber_pubkey` for `epoch_id` - a purely
    /// local bookkeeping row for `list_subscribers` to render quickly,
    /// written unconditionally the moment the delegate decides to grant
    /// access (same local-first philosophy as `insert_post`/`set_profile`:
    /// the decision to subscribe them is real regardless of whether the
    /// network publish of the bundle itself, `contracts::publish_key_bundle_to_network`,
    /// succeeds or is still retrying).
    pub fn record_subscriber(
        &self,
        subscriber_pubkey: &[u8; 33],
        epoch_id: u32,
        issued_at: u64,
    ) -> Result<()> {
        self.conn.lock().expect("db mutex poisoned").execute(
            "INSERT INTO subscribers (subscriber_pubkey, epoch_id, issued_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(subscriber_pubkey, epoch_id) DO UPDATE SET issued_at = excluded.issued_at",
            params![subscriber_pubkey.as_slice(), epoch_id, issued_at as i64],
        )?;
        Ok(())
    }

    /// Locally-recorded (subscriber pubkey, epoch) grants, most recent
    /// first - see `record_subscriber`.
    pub fn list_subscribers(&self) -> Result<Vec<SubscriberRow>> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT subscriber_pubkey, epoch_id, issued_at FROM subscribers ORDER BY issued_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let subscriber_pubkey: Vec<u8> = row.get(0)?;
            let epoch_id: i64 = row.get(1)?;
            let issued_at: i64 = row.get(2)?;
            Ok(SubscriberRow {
                subscriber_pubkey,
                epoch_id: epoch_id as u32,
                issued_at: issued_at as u64,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// Records (or re-confirms) that this delegate follows `author_pubkey` -
    /// called only after `contracts::fetch_remote_profile` has verified a
    /// real, signed `PublisherProfileContract` exists at that pubkey (see
    /// `ipc.rs::handle_follow_publisher`), never for an unvalidated pubkey a
    /// user merely typed in. Upserts on re-follow so refreshing an existing
    /// follow's cached name/avatar doesn't require a separate code path.
    pub fn follow_publisher(
        &self,
        author_pubkey: &[u8; 32],
        display_name: &str,
        avatar_freenet_key: Option<&str>,
        followed_at: u64,
    ) -> Result<()> {
        self.conn.lock().expect("db mutex poisoned").execute(
            "INSERT INTO followed_publishers (author_pubkey, display_name, avatar_freenet_key, followed_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(author_pubkey) DO UPDATE SET
                display_name = excluded.display_name,
                avatar_freenet_key = excluded.avatar_freenet_key,
                followed_at = excluded.followed_at",
            params![author_pubkey.as_slice(), display_name, avatar_freenet_key, followed_at as i64],
        )?;
        Ok(())
    }

    pub fn unfollow_publisher(&self, author_pubkey: &[u8; 32]) -> Result<()> {
        self.conn.lock().expect("db mutex poisoned").execute(
            "DELETE FROM followed_publishers WHERE author_pubkey = ?1",
            params![author_pubkey.as_slice()],
        )?;
        Ok(())
    }

    /// Followed publishers, most recently followed first - the Following
    /// tab's list and the set of publishers `ipc.rs`'s feed handlers fan out
    /// remote fetches to.
    pub fn list_followed_publishers(&self) -> Result<Vec<FollowedPublisherRow>> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT author_pubkey, display_name, avatar_freenet_key, followed_at
             FROM followed_publishers ORDER BY followed_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let author_pubkey: Vec<u8> = row.get(0)?;
            let followed_at: i64 = row.get(3)?;
            Ok(FollowedPublisherRow {
                author_pubkey: author_pubkey.try_into().unwrap_or([0u8; 32]),
                display_name: row.get(1)?,
                avatar_freenet_key: row.get(2)?,
                followed_at: followed_at as u64,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// Upserts one publisher's profile into the durable local cache - called
    /// after any successful `contracts::fetch_remote_profile`, regardless of
    /// whether the caller is following them (a one-off profile visit is
    /// worth remembering too, not just followed publishers).
    pub fn cache_remote_profile(
        &self,
        author_pubkey: &[u8; 32],
        display_name: &str,
        bio: &str,
        avatar_freenet_key: Option<&str>,
        cached_at: u64,
    ) -> Result<()> {
        self.conn.lock().expect("db mutex poisoned").execute(
            "INSERT INTO cached_remote_profiles (author_pubkey, display_name, bio, avatar_freenet_key, cached_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(author_pubkey) DO UPDATE SET
                display_name = excluded.display_name,
                bio = excluded.bio,
                avatar_freenet_key = excluded.avatar_freenet_key,
                cached_at = excluded.cached_at",
            params![
                author_pubkey.as_slice(),
                display_name,
                bio,
                avatar_freenet_key,
                cached_at as i64
            ],
        )?;
        Ok(())
    }

    /// The last successfully-cached copy of `author_pubkey`'s profile, or
    /// `None` if it's never been fetched successfully - the fallback
    /// `handle_get_publisher_profile` reaches for when a live fetch fails.
    pub fn get_cached_remote_profile(
        &self,
        author_pubkey: &[u8; 32],
    ) -> Result<Option<CachedRemoteProfile>> {
        self.conn
            .lock()
            .expect("db mutex poisoned")
            .query_row(
                "SELECT display_name, bio, avatar_freenet_key FROM cached_remote_profiles WHERE author_pubkey = ?1",
                params![author_pubkey.as_slice()],
                |row| {
                    Ok(CachedRemoteProfile {
                        display_name: row.get(0)?,
                        bio: row.get(1)?,
                        avatar_freenet_key: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    /// Upserts one post header into the durable local cache, keyed by its
    /// globally-unique `post_contract_id` - the same table backs both a
    /// followed publisher's own index (`contracts::fetch_remote_posts`) and
    /// the network-wide directory (`contracts::fetch_global_directory`),
    /// since both are just "a post header, discovered a different way."
    #[allow(clippy::too_many_arguments)]
    pub fn cache_remote_post(
        &self,
        post_id: &[u8; 16],
        author_pubkey: &[u8; 32],
        author_display_name: &str,
        title: &str,
        summary: &str,
        post_contract_id: &str,
        access_level: &str,
        epoch_id: u32,
        published_at: u64,
        cached_at: u64,
    ) -> Result<()> {
        self.conn.lock().expect("db mutex poisoned").execute(
            "INSERT INTO cached_remote_posts
                (post_contract_id, post_id, author_pubkey, author_display_name, title, summary, access_level, epoch_id, published_at, cached_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(post_contract_id) DO UPDATE SET
                post_id = excluded.post_id,
                author_pubkey = excluded.author_pubkey,
                author_display_name = excluded.author_display_name,
                title = excluded.title,
                summary = excluded.summary,
                access_level = excluded.access_level,
                epoch_id = excluded.epoch_id,
                published_at = excluded.published_at,
                cached_at = excluded.cached_at",
            params![
                post_contract_id,
                post_id.as_slice(),
                author_pubkey.as_slice(),
                author_display_name,
                title,
                summary,
                access_level,
                epoch_id,
                published_at as i64,
                cached_at as i64,
            ],
        )?;
        Ok(())
    }

    /// Every cached post header from every publisher, most recent first -
    /// backs the Latest tab's durable half of the merge in
    /// `ipc.rs::handle_get_latest_feed`.
    pub fn list_cached_remote_posts(&self) -> Result<Vec<CachedRemotePost>> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT post_id, author_pubkey, author_display_name, title, summary, post_contract_id, access_level, epoch_id, published_at
             FROM cached_remote_posts ORDER BY published_at DESC",
        )?;
        let rows = stmt.query_map([], Self::row_to_cached_remote_post)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// Cached post headers from just one publisher, most recent first -
    /// backs the Home tab's durable half of the merge in
    /// `ipc.rs::followed_feed_items`.
    pub fn list_cached_remote_posts_by_author(
        &self,
        author_pubkey: &[u8; 32],
    ) -> Result<Vec<CachedRemotePost>> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT post_id, author_pubkey, author_display_name, title, summary, post_contract_id, access_level, epoch_id, published_at
             FROM cached_remote_posts WHERE author_pubkey = ?1 ORDER BY published_at DESC",
        )?;
        let rows = stmt.query_map(params![author_pubkey.as_slice()], Self::row_to_cached_remote_post)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    fn row_to_cached_remote_post(row: &rusqlite::Row) -> rusqlite::Result<CachedRemotePost> {
        let post_id: Vec<u8> = row.get(0)?;
        let author_pubkey: Vec<u8> = row.get(1)?;
        let epoch_id: i64 = row.get(7)?;
        let published_at: i64 = row.get(8)?;
        Ok(CachedRemotePost {
            post_id: post_id.try_into().unwrap_or([0u8; 16]),
            author_pubkey: author_pubkey.try_into().unwrap_or([0u8; 32]),
            author_display_name: row.get(2)?,
            title: row.get(3)?,
            summary: row.get(4)?,
            post_contract_id: row.get(5)?,
            access_level: row.get(6)?,
            epoch_id: epoch_id as u32,
            published_at: published_at as u64,
        })
    }

    /// Atomically claims the right to notify about `post_id` exactly once.
    ///
    /// Returns `true` only for the first caller to ever see this post,
    /// `false` for every caller afterwards - the whole check-and-record is a
    /// single `INSERT OR IGNORE`, so two code paths racing over the same new
    /// post (a live subscription push and the polling backstop, see
    /// `watcher.rs`) can never both decide to toast about it. The same call
    /// with the result ignored is how the watcher *silences* a post it has
    /// decided not to announce (a publisher's backlog at follow time, or
    /// everything already published when the app starts).
    pub fn claim_post_notification(
        &self,
        post_id: &[u8; 16],
        author_pubkey: &[u8; 32],
        now: u64,
    ) -> Result<bool> {
        let inserted = self.conn.lock().expect("db mutex poisoned").execute(
            "INSERT OR IGNORE INTO notified_posts (post_id, author_pubkey, notified_at)
             VALUES (?1, ?2, ?3)",
            params![post_id.as_slice(), author_pubkey.as_slice(), now as i64],
        )?;
        Ok(inserted > 0)
    }

    /// Upserts the actual markdown content of a post once successfully
    /// fetched (`contracts::fetch_remote_post_payload` plus the public-post
    /// plaintext convention `handle_get_remote_post` already applies) - once
    /// a reader has actually opened a post, its content stays theirs even if
    /// the network can no longer produce it on a later visit.
    pub fn cache_post_payload(
        &self,
        post_contract_id: &str,
        markdown: &str,
        cached_at: u64,
    ) -> Result<()> {
        self.conn.lock().expect("db mutex poisoned").execute(
            "INSERT INTO cached_post_payloads (post_contract_id, markdown, cached_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(post_contract_id) DO UPDATE SET markdown = excluded.markdown, cached_at = excluded.cached_at",
            params![post_contract_id, markdown, cached_at as i64],
        )?;
        Ok(())
    }

    /// The last successfully-cached copy of a remote post's markdown, or
    /// `None` if it's never been opened successfully before -
    /// `handle_get_remote_post`'s fallback when a live fetch fails.
    pub fn get_cached_post_payload(&self, post_contract_id: &str) -> Result<Option<String>> {
        self.conn
            .lock()
            .expect("db mutex poisoned")
            .query_row(
                "SELECT markdown FROM cached_post_payloads WHERE post_contract_id = ?1",
                params![post_contract_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn get_epoch_key(&self, epoch_id: u32) -> Result<Option<[u8; 32]>> {
        self.conn
            .lock()
            .expect("db mutex poisoned")
            .query_row(
                "SELECT key_bytes FROM epoch_keys WHERE epoch_id = ?1",
                params![epoch_id],
                |row| {
                    let bytes: Vec<u8> = row.get(0)?;
                    Ok(bytes)
                },
            )
            .optional()?
            .map(|bytes| {
                bytes
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("corrupt epoch key length"))
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_temp() -> (LocalStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("aetheria-dbtest-{}", uuid_like()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.sqlite");
        (LocalStore::open(&path).unwrap(), dir)
    }

    // Not a real UUID - just enough entropy that parallel test threads don't
    // collide on the same temp dir (matches the pattern `keys.rs`'s tests use
    // with `std::process::id()`, extended with an atomic counter since
    // several tests in this module open a temp DB within the same process).
    fn uuid_like() -> String {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        format!(
            "{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    }

    #[test]
    fn get_profile_is_none_before_any_save() {
        let (db, dir) = open_temp();
        assert!(db.get_profile().unwrap().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn set_profile_then_get_profile_round_trips() {
        let (db, dir) = open_temp();
        db.set_profile(
            "Hunter Lawson",
            "Independent science writer",
            Some(b"fake-png-bytes"),
            Some("image/png"),
            1_700_000_000,
        )
        .unwrap();

        let row = db.get_profile().unwrap().expect("profile should exist");
        assert_eq!(row.display_name, "Hunter Lawson");
        assert_eq!(row.bio, "Independent science writer");
        assert_eq!(row.avatar_bytes.as_deref(), Some(b"fake-png-bytes".as_slice()));
        assert_eq!(row.avatar_mime.as_deref(), Some("image/png"));
        assert_eq!(row.updated_at, 1_700_000_000);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn set_profile_upserts_the_singleton_row_not_a_new_one() {
        let (db, dir) = open_temp();
        db.set_profile("First Name", "first bio", None, None, 1).unwrap();
        db.set_profile("Second Name", "second bio", None, None, 2).unwrap();

        let row = db.get_profile().unwrap().expect("profile should exist");
        assert_eq!(row.display_name, "Second Name");
        assert_eq!(row.bio, "second bio");
        assert_eq!(row.updated_at, 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn follow_then_list_then_unfollow_round_trips() {
        let (db, dir) = open_temp();
        let pubkey = [7u8; 32];
        db.follow_publisher(&pubkey, "Some Writer", Some("abc123"), 1_700_000_000)
            .unwrap();

        let rows = db.list_followed_publishers().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].author_pubkey, pubkey);
        assert_eq!(rows[0].display_name, "Some Writer");
        assert_eq!(rows[0].avatar_freenet_key.as_deref(), Some("abc123"));

        db.unfollow_publisher(&pubkey).unwrap();
        assert!(db.list_followed_publishers().unwrap().is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn follow_publisher_upserts_rather_than_duplicating() {
        let (db, dir) = open_temp();
        let pubkey = [9u8; 32];
        db.follow_publisher(&pubkey, "First Name", None, 1).unwrap();
        db.follow_publisher(&pubkey, "Updated Name", Some("key1"), 2)
            .unwrap();

        let rows = db.list_followed_publishers().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].display_name, "Updated Name");
        assert_eq!(rows[0].avatar_freenet_key.as_deref(), Some("key1"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cached_remote_profile_is_none_before_any_cache_then_round_trips() {
        let (db, dir) = open_temp();
        let pubkey = [3u8; 32];
        assert!(db.get_cached_remote_profile(&pubkey).unwrap().is_none());

        db.cache_remote_profile(&pubkey, "Some Writer", "Bio text", Some("avatar-key"), 100)
            .unwrap();
        let cached = db.get_cached_remote_profile(&pubkey).unwrap().unwrap();
        assert_eq!(cached.display_name, "Some Writer");
        assert_eq!(cached.bio, "Bio text");
        assert_eq!(cached.avatar_freenet_key.as_deref(), Some("avatar-key"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cache_remote_profile_upserts_rather_than_duplicating() {
        let (db, dir) = open_temp();
        let pubkey = [4u8; 32];
        db.cache_remote_profile(&pubkey, "First", "first bio", None, 1)
            .unwrap();
        db.cache_remote_profile(&pubkey, "Second", "second bio", Some("k"), 2)
            .unwrap();

        let cached = db.get_cached_remote_profile(&pubkey).unwrap().unwrap();
        assert_eq!(cached.display_name, "Second");
        assert_eq!(cached.bio, "second bio");
        assert_eq!(cached.avatar_freenet_key.as_deref(), Some("k"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cached_remote_posts_round_trip_and_filter_by_author() {
        let (db, dir) = open_temp();
        let alice = [5u8; 32];
        let bob = [6u8; 32];
        db.cache_remote_post(
            &[1u8; 16],
            &alice,
            "Alice",
            "Alice's post",
            "summary",
            "contract-1",
            "public",
            0,
            200,
            1000,
        )
        .unwrap();
        db.cache_remote_post(
            &[2u8; 16],
            &bob,
            "Bob",
            "Bob's post",
            "summary",
            "contract-2",
            "subscriber",
            0,
            100,
            1000,
        )
        .unwrap();

        let all = db.list_cached_remote_posts().unwrap();
        assert_eq!(all.len(), 2);
        // Most recent (published_at) first.
        assert_eq!(all[0].post_contract_id, "contract-1");
        assert_eq!(all[1].post_contract_id, "contract-2");

        let alice_only = db.list_cached_remote_posts_by_author(&alice).unwrap();
        assert_eq!(alice_only.len(), 1);
        assert_eq!(alice_only[0].post_contract_id, "contract-1");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cache_remote_post_upserts_by_post_contract_id() {
        let (db, dir) = open_temp();
        let author = [7u8; 32];
        db.cache_remote_post(
            &[1u8; 16], &author, "Author", "Old title", "old summary", "contract-x", "public", 0,
            100, 1000,
        )
        .unwrap();
        db.cache_remote_post(
            &[1u8; 16], &author, "Author", "New title", "new summary", "contract-x", "public", 0,
            100, 2000,
        )
        .unwrap();

        let all = db.list_cached_remote_posts().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].title, "New title");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cached_post_payload_is_none_before_any_cache_then_round_trips() {
        let (db, dir) = open_temp();
        assert!(db.get_cached_post_payload("contract-y").unwrap().is_none());

        db.cache_post_payload("contract-y", "# Hello world", 1000).unwrap();
        assert_eq!(
            db.get_cached_post_payload("contract-y").unwrap().as_deref(),
            Some("# Hello world")
        );

        // Re-caching (e.g. re-opening the same post later) overwrites rather
        // than erroring or duplicating.
        db.cache_post_payload("contract-y", "# Updated content", 2000)
            .unwrap();
        assert_eq!(
            db.get_cached_post_payload("contract-y").unwrap().as_deref(),
            Some("# Updated content")
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
