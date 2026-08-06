//! Key management: the publisher's Ed25519 master signing key, plus a
//! secp256k1 identity key that's no longer used for anything (it backed the
//! ECDH subscriber-key exchange from Aetheria's now-removed payments/
//! subscriptions feature). Kept rather than dropped: it's baked into the
//! on-disk encrypted identity file's fixed 64-byte key-material layout (see
//! below), and this machine's real, already-published identity was created
//! under that layout - removing the field would mean either a breaking
//! format migration or generating a fresh keypair, neither of which buys
//! anything now that nothing reads it.
//!
//! The identity file on disk is encrypted at rest: a passphrase is stretched
//! through Argon2id into an AES-256-GCM key that wraps the raw 64 bytes of
//! key material. Layout: `salt(16) || nonce(12) || ciphertext(80)` = 108
//! bytes total, versus the legacy plaintext format's exactly 64 bytes - that
//! length difference is how `load_or_generate` tells old files apart and
//! migrates them (re-encrypting the same key material under a freshly
//! chosen passphrase) instead of just failing to parse them as ciphertext.
//!
//! TODO(later): this prompts on stdin via `rpassword`, which works for the
//! current CLI-launched delegate but not once Tauri spawns this as a
//! background sidecar with no attached terminal - that needs a real
//! `unlock { passphrase }` IPC message the UI sends before any signing
//! operation is attempted, not a stdin prompt.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result};
use argon2::Argon2;
use ed25519_dalek::{SigningKey, VerifyingKey};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::SecretKey as K256SecretKey;
use rand::rngs::OsRng;
use rand::RngCore;
use std::path::Path;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_MATERIAL_LEN: usize = 64;
const LEGACY_PLAINTEXT_LEN: usize = KEY_MATERIAL_LEN;
const ENCRYPTED_LEN: usize = SALT_LEN + NONCE_LEN + KEY_MATERIAL_LEN + 16; // +16 AEAD tag

pub struct DelegateKeys {
    /// Ed25519 keypair used to sign contract state updates (publisher role).
    pub master_signing: SigningKey,
    /// Secp256k1 keypair used for ECDH epoch-key exchange (subscriber role).
    pub identity_secret: K256SecretKey,
}

impl DelegateKeys {
    pub fn generate() -> Self {
        Self {
            master_signing: SigningKey::generate(&mut OsRng),
            identity_secret: K256SecretKey::random(&mut OsRng),
        }
    }

    pub fn load_or_generate(path: &Path) -> Result<Self> {
        if !path.exists() {
            let passphrase = passphrase_for_new_identity()?;
            let keys = Self::generate();
            keys.save(path, &passphrase)?;
            return Ok(keys);
        }

        let raw = std::fs::read(path).with_context(|| format!("reading {path:?}"))?;
        match raw.len() {
            LEGACY_PLAINTEXT_LEN => {
                tracing::warn!(
                    "identity file is in the old unencrypted format - migrating to encrypted storage"
                );
                let keys = Self::from_key_material(&raw)?;
                let passphrase = passphrase_for_new_identity()?;
                keys.save(path, &passphrase)?;
                Ok(keys)
            }
            ENCRYPTED_LEN => {
                let passphrase = passphrase_for_unlock()?;
                Self::load_encrypted(&raw, &passphrase)
            }
            other => anyhow::bail!(
                "corrupt identity file: {other} bytes (expected {LEGACY_PLAINTEXT_LEN} or {ENCRYPTED_LEN})"
            ),
        }
    }

    fn from_key_material(raw: &[u8]) -> Result<Self> {
        anyhow::ensure!(raw.len() == KEY_MATERIAL_LEN, "corrupt key material length");
        let signing_bytes: [u8; 32] = raw[0..32].try_into().unwrap();
        let secp_bytes: [u8; 32] = raw[32..64].try_into().unwrap();
        Ok(Self {
            master_signing: SigningKey::from_bytes(&signing_bytes),
            identity_secret: K256SecretKey::from_bytes(&secp_bytes.into())
                .context("invalid secp256k1 key bytes")?,
        })
    }

    fn load_encrypted(raw: &[u8], passphrase: &str) -> Result<Self> {
        anyhow::ensure!(raw.len() == ENCRYPTED_LEN, "corrupt identity file length");
        let (salt, rest) = raw.split_at(SALT_LEN);
        let (nonce_bytes, ciphertext) = rest.split_at(NONCE_LEN);

        let wrapping_key = derive_wrapping_key(passphrase, salt)?;
        let cipher = Aes256Gcm::new_from_slice(&wrapping_key).expect("key is exactly 32 bytes");
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
            .map_err(|_| anyhow::anyhow!("wrong passphrase, or the identity file is corrupt"))?;

        Self::from_key_material(&plaintext)
    }

    fn save(&self, path: &Path, passphrase: &str) -> Result<()> {
        let mut key_material = Vec::with_capacity(KEY_MATERIAL_LEN);
        key_material.extend_from_slice(&self.master_signing.to_bytes());
        key_material.extend_from_slice(&self.identity_secret.to_bytes());

        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let wrapping_key = derive_wrapping_key(passphrase, &salt)?;
        let cipher = Aes256Gcm::new_from_slice(&wrapping_key).expect("key is exactly 32 bytes");

        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), key_material.as_slice())
            .map_err(|e| anyhow::anyhow!("encrypting identity file: {e}"))?;

        let mut out = Vec::with_capacity(ENCRYPTED_LEN);
        out.extend_from_slice(&salt);
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        anyhow::ensure!(out.len() == ENCRYPTED_LEN, "encrypted identity file has unexpected length");

        std::fs::write(path, out).with_context(|| format!("writing {path:?}"))
    }

    /// Non-interactive counterpart to `load_or_generate`'s new-identity
    /// branch, for the IPC `unlock` flow (see `ipc.rs`'s module docs) - the
    /// delegate already knows (via `identity_key_path.exists()`) that this is
    /// a first run before calling this, so there's no stdin prompt here, just
    /// the passphrase the UI already collected (with its own confirm-field
    /// double-entry, matching what `prompt_new_passphrase` enforces on the
    /// CLI path).
    pub fn create_new(path: &Path, passphrase: &str) -> Result<Self> {
        anyhow::ensure!(!path.exists(), "identity file already exists at {path:?}");
        anyhow::ensure!(!passphrase.is_empty(), "passphrase cannot be empty");
        let keys = Self::generate();
        keys.save(path, passphrase)?;
        Ok(keys)
    }

    /// Non-interactive counterpart to `load_or_generate`'s existing-file
    /// branch. A wrong passphrase surfaces as a plain `Err` from
    /// `load_encrypted` - callers (the `unlock` IPC handler) should treat
    /// that as a retryable user-facing error, not a crash.
    pub fn unlock_existing(path: &Path, passphrase: &str) -> Result<Self> {
        let raw = std::fs::read(path).with_context(|| format!("reading {path:?}"))?;
        match raw.len() {
            LEGACY_PLAINTEXT_LEN => {
                tracing::warn!(
                    "identity file is in the old unencrypted format - migrating to encrypted storage"
                );
                let keys = Self::from_key_material(&raw)?;
                keys.save(path, passphrase)?;
                Ok(keys)
            }
            ENCRYPTED_LEN => Self::load_encrypted(&raw, passphrase),
            other => anyhow::bail!(
                "corrupt identity file: {other} bytes (expected {LEGACY_PLAINTEXT_LEN} or {ENCRYPTED_LEN})"
            ),
        }
    }

    pub fn master_signing_verifying_bytes(&self) -> [u8; 32] {
        VerifyingKey::from(&self.master_signing).to_bytes()
    }

    /// Compressed SEC1 encoding of this delegate's secp256k1 identity public
    /// key (1 parity byte + 32-byte x-coordinate). Currently unused - see
    /// this module's top-level docs on why the underlying keypair is kept
    /// anyway.
    #[allow(dead_code)]
    pub fn identity_public_compressed(&self) -> [u8; 33] {
        let point = self.identity_secret.public_key().to_encoded_point(true);
        point
            .as_bytes()
            .try_into()
            .expect("SEC1 compressed point is 33 bytes")
    }
}

fn derive_wrapping_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow::anyhow!("deriving key from passphrase: {e}"))?;
    Ok(key)
}

/// Set to skip the interactive passphrase prompt entirely during local
/// dev/testing - **insecure, dev-only**, since it lets anything that can
/// read this process's environment (or a shell history file) recover the
/// passphrase. Never set this outside a local dev loop.
const DEV_PASSPHRASE_ENV_VAR: &str = "AETHERIA_DEV_PASSPHRASE";

fn warn_dev_passphrase_in_use() {
    tracing::warn!(
        "{DEV_PASSPHRASE_ENV_VAR} is set - using it instead of an interactive prompt. \
         This is insecure and for local dev/testing only; never set this in a real deployment."
    );
}

fn passphrase_for_new_identity() -> Result<String> {
    if let Ok(p) = std::env::var(DEV_PASSPHRASE_ENV_VAR) {
        warn_dev_passphrase_in_use();
        anyhow::ensure!(!p.is_empty(), "{DEV_PASSPHRASE_ENV_VAR} is set but empty");
        return Ok(p);
    }
    prompt_new_passphrase()
}

fn passphrase_for_unlock() -> Result<String> {
    if let Ok(p) = std::env::var(DEV_PASSPHRASE_ENV_VAR) {
        warn_dev_passphrase_in_use();
        return Ok(p);
    }
    rpassword::prompt_password("Enter passphrase to unlock your Aetheria identity: ")
        .context("reading passphrase")
}

fn prompt_new_passphrase() -> Result<String> {
    loop {
        let first = rpassword::prompt_password("Create a passphrase to protect your Aetheria identity: ")
            .context("reading passphrase")?;
        anyhow::ensure!(!first.is_empty(), "passphrase cannot be empty");
        let confirm =
            rpassword::prompt_password("Confirm passphrase: ").context("reading passphrase")?;
        if first == confirm {
            return Ok(first);
        }
        eprintln!("Passphrases didn't match - try again.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These exercise the encryption/migration logic directly against
    // in-memory byte buffers and literal passphrases, deliberately
    // bypassing `prompt_new_passphrase`/`load_or_generate`'s stdin prompts -
    // `rpassword` reads from the real console device on Windows (see its
    // `tests/no-terminal.rs`), not redirected stdin, so it can't be driven
    // by a piped-input test anyway. This still covers every security-
    // relevant code path: only the interactive prompt itself is untested.

    fn sample_key_material() -> ([u8; 64], DelegateKeys) {
        let keys = DelegateKeys::generate();
        let mut raw = [0u8; 64];
        raw[0..32].copy_from_slice(&keys.master_signing.to_bytes());
        raw[32..64].copy_from_slice(&keys.identity_secret.to_bytes());
        (raw, keys)
    }

    #[test]
    fn round_trips_with_correct_passphrase() {
        let dir = std::env::temp_dir().join(format!("aetheria-keytest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity.key");

        let keys = DelegateKeys::generate();
        let original_pubkey = keys.master_signing_verifying_bytes();
        keys.save(&path, "correct horse battery staple").unwrap();

        let raw = std::fs::read(&path).unwrap();
        assert_eq!(raw.len(), ENCRYPTED_LEN);

        let loaded =
            DelegateKeys::load_encrypted(&raw, "correct horse battery staple").unwrap();
        assert_eq!(loaded.master_signing_verifying_bytes(), original_pubkey);
        assert_eq!(
            loaded.identity_secret.to_bytes(),
            keys.identity_secret.to_bytes()
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wrong_passphrase_is_rejected() {
        let dir = std::env::temp_dir().join(format!("aetheria-keytest-wrong-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity.key");

        let keys = DelegateKeys::generate();
        keys.save(&path, "correct horse battery staple").unwrap();
        let raw = std::fs::read(&path).unwrap();

        let result = DelegateKeys::load_encrypted(&raw, "wrong passphrase entirely");
        assert!(result.is_err(), "decryption should fail with the wrong passphrase");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn migrates_legacy_plaintext_key_material() {
        let (raw, original) = sample_key_material();
        let migrated = DelegateKeys::from_key_material(&raw).unwrap();
        assert_eq!(
            migrated.master_signing_verifying_bytes(),
            original.master_signing_verifying_bytes()
        );
        assert_eq!(
            migrated.identity_secret.to_bytes(),
            original.identity_secret.to_bytes()
        );
    }

    #[test]
    fn rejects_corrupt_lengths() {
        assert!(DelegateKeys::from_key_material(&[0u8; 63]).is_err());
        assert!(DelegateKeys::from_key_material(&[0u8; 65]).is_err());
        assert!(DelegateKeys::load_encrypted(&[0u8; ENCRYPTED_LEN - 1], "x").is_err());
        assert!(DelegateKeys::load_encrypted(&[0u8; ENCRYPTED_LEN + 1], "x").is_err());
    }

    #[test]
    fn load_or_generate_migrates_a_real_legacy_file_in_place() {
        // This one *does* exercise `load_or_generate`'s file-format branch,
        // but calls `from_key_material` + `save` directly with the same
        // legacy bytes rather than going through the stdin-prompting public
        // entry point, for the reason noted at the top of this module.
        let dir = std::env::temp_dir().join(format!("aetheria-keytest-migrate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity.key");

        let (raw, original) = sample_key_material();
        std::fs::write(&path, raw).unwrap();
        assert_eq!(std::fs::read(&path).unwrap().len(), LEGACY_PLAINTEXT_LEN);

        let legacy_bytes = std::fs::read(&path).unwrap();
        let recovered = DelegateKeys::from_key_material(&legacy_bytes).unwrap();
        recovered.save(&path, "a new passphrase").unwrap();

        let migrated_raw = std::fs::read(&path).unwrap();
        assert_eq!(migrated_raw.len(), ENCRYPTED_LEN);
        let reloaded = DelegateKeys::load_encrypted(&migrated_raw, "a new passphrase").unwrap();
        assert_eq!(
            reloaded.master_signing_verifying_bytes(),
            original.master_signing_verifying_bytes()
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
