use serde::{Deserialize, Serialize};

use crate::crypto::{Address, Hash, Signature};

// ─── Amount ─────────────────────────────────────────────────────────────────

/// Amount in quanta (the smallest indivisible unit).
/// 1 BAUD = 10^18 quanta, enabling extreme micro-transactions.
pub type Amount = u128;

/// 1 BAUD expressed in quanta.
pub const QUANTA_PER_BAUD: Amount = 1_000_000_000_000_000_000;

// ─── Transaction types ──────────────────────────────────────────────────────

/// Every action on the ledger is a signed transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// The agent submitting this transaction.
    pub sender: Address,
    /// Monotonically increasing per-sender nonce (replay protection).
    pub nonce: u64,
    /// What this transaction does.
    pub payload: TxPayload,
    /// Unix-millisecond timestamp. Nodes reject txs too far in the future.
    pub timestamp: u64,
    /// Chain identifier — prevents cross-chain replay attacks.
    pub chain_id: String,
    /// Ed25519 signature over the canonical hash of the above fields.
    pub signature: Signature,
}

/// The semantic actions an agent can perform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TxPayload {
    /// Simple value transfer between two agents.
    Transfer {
        to: Address,
        amount: Amount,
        /// Optional opaque memo (max 256 bytes) for inter-agent metadata.
        memo: Option<Vec<u8>>,
    },
    /// Lock funds into a hash-locked escrow contract.
    EscrowCreate {
        recipient: Address,
        amount: Amount,
        /// BLAKE3 hash of the secret that unlocks the escrow.
        hash_lock: Hash,
        /// Unix-millisecond deadline after which the sender can reclaim.
        deadline: u64,
    },
    /// Release escrowed funds by revealing the pre-image.
    EscrowRelease {
        escrow_id: Hash,
        /// The pre-image whose BLAKE3 hash matches the hash_lock.
        preimage: Vec<u8>,
    },
    /// Refund escrowed funds after the deadline has passed.
    EscrowRefund {
        escrow_id: Hash,
    },
    /// Register / update agent metadata on-chain.
    AgentRegister {
        /// Human-readable agent name (max 64 bytes, UTF-8).
        name: Vec<u8>,
        /// Service endpoint URL or multiaddr (max 256 bytes).
        endpoint: Vec<u8>,
        /// Capability tags for discovery (e.g., ["llm", "vision"]).
        capabilities: Vec<Vec<u8>>,
    },
}

/// Maximum serialized transaction size (64 KiB).
pub const MAX_TX_SIZE: usize = 65_536;
/// Maximum memo length in bytes.
pub const MAX_MEMO_LEN: usize = 256;
/// Maximum agent name length.
pub const MAX_AGENT_NAME_LEN: usize = 64;
/// Maximum endpoint length.
pub const MAX_ENDPOINT_LEN: usize = 256;
/// Maximum number of capability tags.
pub const MAX_CAPABILITIES: usize = 16;
/// Maximum length of a single capability tag.
pub const MAX_CAPABILITY_LEN: usize = 64;

impl Transaction {
    /// Compute the canonical hash of the signable portion (everything except signature).
    pub fn signable_hash(&self) -> Hash {
        let payload_bytes = bincode::serialize(&(
            &self.sender,
            &self.nonce,
            &self.payload,
            &self.timestamp,
            &self.chain_id,
        ))
        .expect("serialization of tx fields should never fail");
        Hash::digest(&payload_bytes)
    }

    /// Compute the full transaction hash (including signature).
    pub fn hash(&self) -> Hash {
        let bytes = bincode::serialize(self)
            .expect("serialization of tx should never fail");
        Hash::digest(&bytes)
    }

    /// Validate structural constraints (sizes, memo length, etc.) without
    /// checking the signature or state.
    pub fn validate_structure(&self) -> Result<(), crate::error::BaudError> {
        use crate::error::BaudError;

        let size = bincode::serialized_size(self)
            .map_err(|e| BaudError::Serialization(e.to_string()))? as usize;
        if size > MAX_TX_SIZE {
            return Err(BaudError::TransactionTooLarge {
                size,
                max: MAX_TX_SIZE,
            });
        }

        match &self.payload {
            TxPayload::Transfer { to, amount, memo } => {
                if self.sender == *to {
                    return Err(BaudError::SelfTransfer);
                }
                if *amount == 0 {
                    return Err(BaudError::ZeroAmount);
                }
                if let Some(m) = memo {
                    if m.len() > MAX_MEMO_LEN {
                        return Err(BaudError::TransactionTooLarge {
                            size: m.len(),
                            max: MAX_MEMO_LEN,
                        });
                    }
                }
            }
            TxPayload::EscrowCreate {
                recipient, amount, deadline, ..
            } => {
                if self.sender == *recipient {
                    return Err(BaudError::SelfTransfer);
                }
                if *amount == 0 {
                    return Err(BaudError::ZeroAmount);
                }
                if *deadline <= self.timestamp {
                    return Err(BaudError::EscrowDeadlineExceeded {
                        current: self.timestamp,
                        deadline: *deadline,
                    });
                }
            }
            TxPayload::EscrowRelease { preimage, .. } => {
                if preimage.len() > 1024 {
                    return Err(BaudError::InvalidEscrowProof(
                        "preimage too large".into(),
                    ));
                }
            }
            TxPayload::EscrowRefund { .. } => {}
            TxPayload::AgentRegister {
                name,
                endpoint,
                capabilities,
            } => {
                if name.len() > MAX_AGENT_NAME_LEN {
                    return Err(BaudError::TransactionTooLarge {
                        size: name.len(),
                        max: MAX_AGENT_NAME_LEN,
                    });
                }
                if endpoint.len() > MAX_ENDPOINT_LEN {
                    return Err(BaudError::TransactionTooLarge {
                        size: endpoint.len(),
                        max: MAX_ENDPOINT_LEN,
                    });
                }
                if capabilities.len() > MAX_CAPABILITIES {
                    return Err(BaudError::TransactionTooLarge {
                        size: capabilities.len(),
                        max: MAX_CAPABILITIES,
                    });
                }
                for cap in capabilities {
                    if cap.len() > MAX_CAPABILITY_LEN {
                        return Err(BaudError::TransactionTooLarge {
                            size: cap.len(),
                            max: MAX_CAPABILITY_LEN,
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

// ─── Block ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    /// Monotonically increasing block height (genesis = 0).
    pub height: u64,
    /// Hash of the previous block header (Hash::zero() for genesis).
    pub prev_hash: Hash,
    /// Merkle root of all transaction hashes in this block.
    pub tx_root: Hash,
    /// Root hash of the world state after applying this block.
    pub state_root: Hash,
    /// Unix-millisecond timestamp when the block was proposed.
    pub timestamp: u64,
    /// Address of the validator that proposed this block.
    pub proposer: Address,
    /// Number of transactions in this block.
    pub tx_count: u32,
    /// Proposer's signature over the header hash (computed without this field).
    pub signature: Signature,
}

impl BlockHeader {
    /// Hash of the header content (excluding the signature field).
    pub fn signable_hash(&self) -> Hash {
        let bytes = bincode::serialize(&(
            self.height,
            &self.prev_hash,
            &self.tx_root,
            &self.state_root,
            self.timestamp,
            &self.proposer,
            self.tx_count,
        ))
        .expect("header serialization should never fail");
        Hash::digest(&bytes)
    }

    pub fn hash(&self) -> Hash {
        let bytes = bincode::serialize(self)
            .expect("header serialization should never fail");
        Hash::digest(&bytes)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}

impl Block {
    pub fn hash(&self) -> Hash {
        self.header.hash()
    }
}

// ─── Account ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub address: Address,
    /// Balance in quanta.
    pub balance: Amount,
    /// Next expected nonce (starts at 0).
    pub nonce: u64,
    /// Optional agent metadata.
    pub agent_meta: Option<AgentMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMeta {
    pub name: Vec<u8>,
    pub endpoint: Vec<u8>,
    pub capabilities: Vec<Vec<u8>>,
}

impl Account {
    pub fn new(address: Address) -> Self {
        Self {
            address,
            balance: 0,
            nonce: 0,
            agent_meta: None,
        }
    }

    pub fn with_balance(address: Address, balance: Amount) -> Self {
        Self {
            address,
            balance,
            nonce: 0,
            agent_meta: None,
        }
    }
}

// ─── Escrow ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EscrowStatus {
    Active,
    Released,
    Refunded,
}

/// A hash-time-locked escrow contract between two agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Escrow {
    /// Unique ID derived from creation tx hash.
    pub id: Hash,
    pub sender: Address,
    pub recipient: Address,
    pub amount: Amount,
    /// The BLAKE3 hash of the secret pre-image.
    pub hash_lock: Hash,
    /// Unix-millisecond deadline for the recipient to claim.
    pub deadline: u64,
    pub status: EscrowStatus,
    /// Block height at which this escrow was created.
    pub created_at_height: u64,
}

// ─── Genesis ────────────────────────────────────────────────────────────────

/// Configuration for the genesis block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisConfig {
    /// Chain identifier.
    pub chain_id: String,
    /// Initial token allocations.
    pub allocations: Vec<GenesisAllocation>,
    /// Initial validator set.
    pub validators: Vec<ValidatorInfo>,
    /// Genesis timestamp (unix milliseconds).
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisAllocation {
    pub address: Address,
    pub balance: Amount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    pub address: Address,
    pub name: String,
}
