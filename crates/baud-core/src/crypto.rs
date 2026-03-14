use ed25519_dalek::{Signature as DalekSignature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::error::{BaudError, BaudResult};

// ─── Address ────────────────────────────────────────────────────────────────

/// A 32-byte agent address derived from an Ed25519 public key.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Address(#[serde(with = "hex::serde")] pub [u8; 32]);

impl Address {
    pub fn from_public_key(pk: &VerifyingKey) -> Self {
        Self(pk.to_bytes())
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> BaudResult<Self> {
        let bytes = hex::decode(s).map_err(|_| BaudError::InvalidPublicKey)?;
        if bytes.len() != 32 {
            return Err(BaudError::InvalidPublicKey);
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The zero address (used for genesis minting).
    pub fn zero() -> Self {
        Self([0u8; 32])
    }

    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }
}

impl std::fmt::Debug for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Address({})", &self.to_hex()[..16])
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

// ─── Hash ───────────────────────────────────────────────────────────────────

/// A 32-byte BLAKE3 hash.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Hash(#[serde(with = "hex::serde")] pub [u8; 32]);

impl Hash {
    pub fn digest(data: &[u8]) -> Self {
        Self(*blake3::hash(data).as_bytes())
    }

    /// Compute hash of multiple byte slices concatenated.
    pub fn digest_many(slices: &[&[u8]]) -> Self {
        let mut hasher = blake3::Hasher::new();
        for s in slices {
            hasher.update(s);
        }
        Self(*hasher.finalize().as_bytes())
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> BaudResult<Self> {
        let bytes = hex::decode(s).map_err(|e| BaudError::Serialization(e.to_string()))?;
        if bytes.len() != 32 {
            return Err(BaudError::Serialization("invalid hash length".into()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    pub fn zero() -> Self {
        Self([0u8; 32])
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Hash({})", &self.to_hex()[..16])
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

// ─── Merkle Root ────────────────────────────────────────────────────────────

/// Compute the Merkle root of a list of hashes (binary tree).
pub fn merkle_root(hashes: &[Hash]) -> Hash {
    if hashes.is_empty() {
        return Hash::zero();
    }
    if hashes.len() == 1 {
        return hashes[0];
    }

    let mut level: Vec<Hash> = hashes.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for chunk in level.chunks(2) {
            if chunk.len() == 2 {
                next.push(Hash::digest_many(&[&chunk[0].0, &chunk[1].0]));
            } else {
                // Odd element: hash with itself.
                next.push(Hash::digest_many(&[&chunk[0].0, &chunk[0].0]));
            }
        }
        level = next;
    }
    level[0]
}

// ─── Signature ──────────────────────────────────────────────────────────────

/// A 64-byte Ed25519 signature.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature(#[serde(with = "hex::serde")] pub [u8; 64]);

impl Signature {
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> BaudResult<Self> {
        let bytes = hex::decode(s).map_err(|e| BaudError::Serialization(e.to_string()))?;
        if bytes.len() != 64 {
            return Err(BaudError::Serialization("invalid signature length".into()));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    pub fn zero() -> Self {
        Self([0u8; 64])
    }
}

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sig({}..)", &self.to_hex()[..16])
    }
}

// ─── KeyPair wrapper ────────────────────────────────────────────────────────

/// Wrapper around Ed25519 signing key for ergonomic use.
pub struct KeyPair {
    signing: SigningKey,
}

impl KeyPair {
    /// Generate a new random keypair using OS entropy.
    pub fn generate() -> Self {
        Self {
            signing: SigningKey::generate(&mut OsRng),
        }
    }

    /// Restore from a 32-byte secret seed.
    pub fn from_secret_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            signing: SigningKey::from_bytes(bytes),
        }
    }

    /// Restore from hex-encoded secret seed.
    pub fn from_secret_hex(s: &str) -> BaudResult<Self> {
        let bytes = hex::decode(s).map_err(|_| BaudError::InvalidSecretKey)?;
        if bytes.len() != 32 {
            return Err(BaudError::InvalidSecretKey);
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self::from_secret_bytes(&arr))
    }

    pub fn address(&self) -> Address {
        Address::from_public_key(&self.signing.verifying_key())
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }

    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }

    pub fn secret_hex(&self) -> String {
        hex::encode(self.signing.to_bytes())
    }

    /// Sign arbitrary bytes, returning our Signature wrapper.
    pub fn sign(&self, message: &[u8]) -> Signature {
        let sig = self.signing.sign(message);
        Signature(sig.to_bytes())
    }
}

// ─── Standalone verification ────────────────────────────────────────────────

/// Verify a signature against a message and address (public key bytes).
pub fn verify_signature(
    address: &Address,
    message: &[u8],
    signature: &Signature,
) -> BaudResult<()> {
    let pk = VerifyingKey::from_bytes(&address.0)
        .map_err(|e| BaudError::VerificationFailed(format!("bad public key: {e}")))?;
    let sig = DalekSignature::from_bytes(&signature.0);
    pk.verify(message, &sig)
        .map_err(|e| BaudError::InvalidSignature(format!("{e}")))
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify() {
        let kp = KeyPair::generate();
        let msg = b"hello baud";
        let sig = kp.sign(msg);
        assert!(verify_signature(&kp.address(), msg, &sig).is_ok());
    }

    #[test]
    fn wrong_message_fails() {
        let kp = KeyPair::generate();
        let sig = kp.sign(b"correct");
        assert!(verify_signature(&kp.address(), b"wrong", &sig).is_err());
    }

    #[test]
    fn wrong_key_fails() {
        let kp1 = KeyPair::generate();
        let kp2 = KeyPair::generate();
        let sig = kp1.sign(b"data");
        assert!(verify_signature(&kp2.address(), b"data", &sig).is_err());
    }

    #[test]
    fn address_hex_roundtrip() {
        let kp = KeyPair::generate();
        let addr = kp.address();
        let hex_str = addr.to_hex();
        let recovered = Address::from_hex(&hex_str).unwrap();
        assert_eq!(addr, recovered);
    }

    #[test]
    fn hash_deterministic() {
        let h1 = Hash::digest(b"test");
        let h2 = Hash::digest(b"test");
        assert_eq!(h1, h2);
    }

    #[test]
    fn merkle_root_deterministic() {
        let hashes = vec![Hash::digest(b"a"), Hash::digest(b"b"), Hash::digest(b"c")];
        let r1 = merkle_root(&hashes);
        let r2 = merkle_root(&hashes);
        assert_eq!(r1, r2);
    }

    #[test]
    fn merkle_root_empty() {
        assert_eq!(merkle_root(&[]), Hash::zero());
    }

    #[test]
    fn keypair_restore_from_hex() {
        let kp = KeyPair::generate();
        let secret = kp.secret_hex();
        let restored = KeyPair::from_secret_hex(&secret).unwrap();
        assert_eq!(kp.address(), restored.address());
    }
}
