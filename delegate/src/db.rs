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
            "#,
        )?;
        // `profile` is a brand-new table (no existing on-disk DB predates
        // it), so plain `CREATE TABLE IF NOT EXISTS` above is enough - unlike
        // `post_contract_id` below, there's no pre-existing schema shape to
        // guard against.

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
}
