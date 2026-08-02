//! Key management: the publisher's Ed25519 master signing key and the
//! reader's Secp256k1 identity key used for ECDH key exchange.
//!
//! See design doc section 4.1. Storage here is a placeholder — production
//! should encrypt the key file at rest (OS keychain / passphrase-derived
//! wrapping key) rather than writing raw key bytes to disk.

use anyhow::{Context, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};
use k256::SecretKey as K256SecretKey;
use rand::rngs::OsRng;
use std::path::Path;

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
        if path.exists() {
            Self::load(path)
        } else {
            let keys = Self::generate();
            keys.save(path)?;
            Ok(keys)
        }
    }

    fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read(path).with_context(|| format!("reading {path:?}"))?;
        anyhow::ensure!(raw.len() == 64, "corrupt identity key file");
        let signing_bytes: [u8; 32] = raw[0..32].try_into().unwrap();
        let secp_bytes: [u8; 32] = raw[32..64].try_into().unwrap();
        Ok(Self {
            master_signing: SigningKey::from_bytes(&signing_bytes),
            identity_secret: K256SecretKey::from_bytes(&secp_bytes.into())
                .context("invalid secp256k1 key bytes")?,
        })
    }

    fn save(&self, path: &Path) -> Result<()> {
        let mut raw = Vec::with_capacity(64);
        raw.extend_from_slice(&self.master_signing.to_bytes());
        raw.extend_from_slice(&self.identity_secret.to_bytes());
        std::fs::write(path, raw).with_context(|| format!("writing {path:?}"))
    }

    pub fn master_signing_verifying_bytes(&self) -> [u8; 32] {
        VerifyingKey::from(&self.master_signing).to_bytes()
    }
}
