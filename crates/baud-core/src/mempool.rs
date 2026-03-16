use std::collections::BTreeMap;

use parking_lot::RwLock;
use tracing::debug;

use crate::crypto::{verify_signature, Address, Hash};
use crate::error::{BaudError, BaudResult};
use crate::types::Transaction;

/// Maximum number of pending transactions in the mempool.
const DEFAULT_CAPACITY: usize = 100_000;

/// Maximum nonce gap allowed above the highest pending nonce per sender.
const MAX_NONCE_GAP: u64 = 100;

/// Maximum pending transactions per sender.
const MAX_PER_SENDER: usize = 1000;

/// Thread-safe transaction mempool ordered by arrival.
pub struct Mempool {
    /// Transactions indexed by hash for dedup.
    by_hash: RwLock<std::collections::HashMap<Hash, Transaction>>,
    /// Ordering: (timestamp, tx_hash) → tx_hash for deterministic ordering.
    ordered: RwLock<BTreeMap<(u64, Hash), Hash>>,
    /// Per-sender pending nonces for gap detection.
    sender_nonces: RwLock<std::collections::HashMap<Address, Vec<u64>>>,
    capacity: usize,
}

impl Mempool {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            by_hash: RwLock::new(std::collections::HashMap::new()),
            ordered: RwLock::new(BTreeMap::new()),
            sender_nonces: RwLock::new(std::collections::HashMap::new()),
            capacity,
        }
    }

    /// Add a transaction to the mempool. Rejects duplicates, capacity overflow,
    /// and structurally invalid or unsigned transactions.
    pub fn add(&self, tx: Transaction) -> BaudResult<Hash> {
        // Structural validation (prevents malformed txs from consuming pool space).
        tx.validate_structure()?;

        // Signature verification (prevents unsigned/forged txs from entering pool).
        let signable = tx.signable_hash();
        verify_signature(&tx.sender, signable.as_bytes(), &tx.signature)?;

        let tx_hash = tx.hash();

        let mut by_hash = self.by_hash.write();
        if by_hash.len() >= self.capacity {
            return Err(BaudError::MempoolFull(self.capacity));
        }
        if by_hash.contains_key(&tx_hash) {
            return Err(BaudError::DuplicateTransaction(tx_hash.to_hex()));
        }

        let mut ordered = self.ordered.write();
        let mut sender_nonces = self.sender_nonces.write();

        // Per-sender limits: prevent spam and nonce-gap attacks
        let sender_entry = sender_nonces.entry(tx.sender).or_default();
        if sender_entry.len() >= MAX_PER_SENDER {
            return Err(BaudError::MempoolFull(MAX_PER_SENDER));
        }
        if let Some(&max_nonce) = sender_entry.iter().max() {
            if tx.nonce > max_nonce + MAX_NONCE_GAP {
                return Err(BaudError::NonceGapTooLarge {
                    current: max_nonce,
                    got: tx.nonce,
                    max_gap: MAX_NONCE_GAP,
                });
            }
        }

        ordered.insert((tx.timestamp, tx_hash), tx_hash);
        sender_entry.push(tx.nonce);
        by_hash.insert(tx_hash, tx);

        debug!(tx = %tx_hash, pool_size = by_hash.len(), "tx added to mempool");
        Ok(tx_hash)
    }

    /// Remove a transaction (e.g., after it is included in a block).
    pub fn remove(&self, tx_hash: &Hash) {
        let mut by_hash = self.by_hash.write();
        if let Some(tx) = by_hash.remove(tx_hash) {
            let mut ordered = self.ordered.write();
            ordered.remove(&(tx.timestamp, *tx_hash));

            let mut sender_nonces = self.sender_nonces.write();
            if let Some(nonces) = sender_nonces.get_mut(&tx.sender) {
                nonces.retain(|n| *n != tx.nonce);
                if nonces.is_empty() {
                    sender_nonces.remove(&tx.sender);
                }
            }
        }
    }

    /// Remove multiple transactions (batch removal after block application).
    pub fn remove_batch(&self, tx_hashes: &[Hash]) {
        for h in tx_hashes {
            self.remove(h);
        }
    }

    /// Get up to `limit` transactions ordered by timestamp for block proposal.
    pub fn get_pending(&self, limit: usize) -> Vec<Transaction> {
        let by_hash = self.by_hash.read();
        let ordered = self.ordered.read();

        ordered
            .values()
            .filter_map(|h| by_hash.get(h).cloned())
            .take(limit)
            .collect()
    }

    /// Check if a transaction is already in the pool.
    pub fn contains(&self, tx_hash: &Hash) -> bool {
        self.by_hash.read().contains_key(tx_hash)
    }

    /// Current number of pending transactions.
    pub fn len(&self) -> usize {
        self.by_hash.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get a specific transaction by hash.
    pub fn get(&self, tx_hash: &Hash) -> Option<Transaction> {
        self.by_hash.read().get(tx_hash).cloned()
    }
}

impl Default for Mempool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{KeyPair, Signature as BaudSignature};
    use crate::types::TxPayload;

    fn dummy_tx(kp: &KeyPair, nonce: u64, ts: u64) -> Transaction {
        let payload = TxPayload::Transfer {
            to: KeyPair::generate().address(),
            amount: 1,
            memo: None,
        };
        let mut tx = Transaction {
            sender: kp.address(),
            nonce,
            payload,
            timestamp: ts,
            chain_id: "test".into(),
            signature: BaudSignature::zero(),
        };
        let h = tx.signable_hash();
        tx.signature = kp.sign(h.as_bytes());
        tx
    }

    #[test]
    fn add_and_retrieve() {
        let pool = Mempool::new();
        let kp = KeyPair::generate();
        let tx = dummy_tx(&kp, 0, 100);
        let hash = pool.add(tx).unwrap();
        assert!(pool.contains(&hash));
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn reject_duplicate() {
        let pool = Mempool::new();
        let kp = KeyPair::generate();
        let tx = dummy_tx(&kp, 0, 100);
        let tx_clone = tx.clone();
        pool.add(tx).unwrap();
        assert!(pool.add(tx_clone).is_err());
    }

    #[test]
    fn remove_works() {
        let pool = Mempool::new();
        let kp = KeyPair::generate();
        let tx = dummy_tx(&kp, 0, 100);
        let hash = pool.add(tx).unwrap();
        pool.remove(&hash);
        assert!(!pool.contains(&hash));
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn ordering_by_timestamp() {
        let pool = Mempool::new();
        let kp = KeyPair::generate();

        let tx1 = dummy_tx(&kp, 0, 300);
        let tx2 = dummy_tx(&kp, 1, 100);
        let tx3 = dummy_tx(&kp, 2, 200);

        pool.add(tx1).unwrap();
        pool.add(tx2).unwrap();
        pool.add(tx3).unwrap();

        let pending = pool.get_pending(10);
        assert_eq!(pending.len(), 3);
        assert!(pending[0].timestamp <= pending[1].timestamp);
        assert!(pending[1].timestamp <= pending[2].timestamp);
    }

    #[test]
    fn capacity_limit() {
        let pool = Mempool::with_capacity(2);
        let kp = KeyPair::generate();

        pool.add(dummy_tx(&kp, 0, 100)).unwrap();
        pool.add(dummy_tx(&kp, 1, 200)).unwrap();
        assert!(pool.add(dummy_tx(&kp, 2, 300)).is_err());
    }
}
