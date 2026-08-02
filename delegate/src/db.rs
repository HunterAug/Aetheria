//! Local SQLite cache: decrypted posts, subscriber records, and epoch keys
//! the Delegate has recovered so the UI can render feeds offline.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct LocalStore {
    conn: Connection,
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
            "#,
        )?;
        Ok(Self { conn })
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
        self.conn.execute(
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
        let mut stmt = self.conn.prepare(
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
        self.conn.execute(
            "INSERT INTO epoch_keys (epoch_id, key_bytes, recovered_at) VALUES (?1, ?2, ?3)",
            params![epoch_id, key.as_slice(), now as i64],
        )?;
        Ok(key)
    }

    pub fn get_epoch_key(&self, epoch_id: u32) -> Result<Option<[u8; 32]>> {
        self.conn
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
