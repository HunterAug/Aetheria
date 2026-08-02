//! Content encryption pipeline and ECDH epoch-key exchange.
//!
//! Implements the mathematical flow from design doc section 4.2:
//!   Kepoch = CSPRNG(256)
//!   C = AES-GCM-Encrypt(Kepoch, Nonce, M)
//!   Si = HKDF(ECDH(SKpub, PKsub,i))
//!   Ekey,i = AES-GCM-Encrypt(Si, Noncek, Kepoch)

use aes_gcm::aead::{Aead, KeyInit, OsRng as AesOsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Result};
use hkdf::Hkdf;
use k256::ecdh::diffie_hellman;
use k256::{PublicKey as K256PublicKey, SecretKey as K256SecretKey};
use rand::RngCore;
use sha2::Sha256;

pub type EpochKey = [u8; 32];

/// Generates a fresh random 256-bit symmetric key for a billing epoch.
pub fn generate_epoch_key() -> EpochKey {
    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    key
}

pub struct Ciphertext {
    pub cipher_text: Vec<u8>,
    pub nonce: [u8; 12],
}

/// AES-256-GCM encrypt an article payload under the current epoch key.
pub fn encrypt_payload(key: &EpochKey, plaintext: &[u8]) -> Result<Ciphertext> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| anyhow!(e))?;
    let mut nonce_bytes = [0u8; 12];
    AesOsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let cipher_text = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow!("encryption failed: {e}"))?;
    Ok(Ciphertext {
        cipher_text,
        nonce: nonce_bytes,
    })
}

pub fn decrypt_payload(key: &EpochKey, nonce: &[u8; 12], cipher_text: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| anyhow!(e))?;
    cipher
        .decrypt(Nonce::from_slice(nonce), cipher_text)
        .map_err(|e| anyhow!("decryption failed: {e}"))
}

/// Publisher side: derive Si = HKDF(ECDH(SKpub, PKsub,i)) for a subscriber.
pub fn derive_shared_secret(
    publisher_secret: &K256SecretKey,
    subscriber_public: &K256PublicKey,
) -> [u8; 32] {
    let shared = diffie_hellman(
        publisher_secret.to_nonzero_scalar(),
        subscriber_public.as_affine(),
    );
    let hk = Hkdf::<Sha256>::new(None, shared.raw_secret_bytes());
    let mut okm = [0u8; 32];
    hk.expand(b"aetheria-epoch-key-bundle", &mut okm)
        .expect("32 bytes is a valid HKDF output length");
    okm
}

/// Subscriber side: derive the same Si using their secret and the
/// publisher's public key (ECDH is symmetric: Si is identical either way).
pub fn derive_shared_secret_as_subscriber(
    subscriber_secret: &K256SecretKey,
    publisher_public: &K256PublicKey,
) -> [u8; 32] {
    derive_shared_secret(subscriber_secret, publisher_public)
}

/// Encrypt Kepoch for a specific subscriber using their shared secret Si.
pub fn wrap_epoch_key(shared_secret: &[u8; 32], epoch_key: &EpochKey) -> Result<Ciphertext> {
    encrypt_payload(shared_secret, epoch_key)
}

/// Recover Kepoch from an encrypted key bundle using the shared secret Si.
pub fn unwrap_epoch_key(
    shared_secret: &[u8; 32],
    nonce: &[u8; 12],
    cipher_text: &[u8],
) -> Result<EpochKey> {
    let bytes = decrypt_payload(shared_secret, nonce, cipher_text)?;
    bytes
        .try_into()
        .map_err(|_| anyhow!("unwrapped key bundle was not 32 bytes"))
}
