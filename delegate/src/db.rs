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
                published_at  INTEGER NOT NULL
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
            "#,
        )?;
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
            "SELECT post_id, title, summary, access_level, epoch_id, published_at
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
                "SELECT title, access_level, epoch_id, markdown, cipher_text, nonce
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
                    })
                },
            )
            .optional()
            .map_err(Into::into)
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
