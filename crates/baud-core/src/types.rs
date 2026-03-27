use serde::{Deserialize, Serialize};

use crate::crypto::{Address, Hash, Signature};

// ─── Amount ─────────────────────────────────────────────────────────────────

/// Amount in quanta (the smallest indivisible unit).
/// 1 BAUD = 10^18 quanta, enabling extreme micro-transactions.
pub type Amount = u128;

/// 1 BAUD expressed in quanta.
pub const QUANTA_PER_BAUD: Amount = 1_000_000_000_000_000_000;

/// Total supply: 1 billion BAUD.
pub const TOTAL_SUPPLY_BAUD: u64 = 1_000_000_000;

/// Total supply in quanta (1 billion * 10^18).
pub const TOTAL_SUPPLY_QUANTA: Amount = (TOTAL_SUPPLY_BAUD as u128) * QUANTA_PER_BAUD;

// ─── Mining / block reward ──────────────────────────────────────────────────

/// Initial block reward: 500 BAUD per block (in quanta).
pub const INITIAL_BLOCK_REWARD: Amount = 500 * QUANTA_PER_BAUD;

/// Number of blocks between each halving (every ~58 days at 5s blocks).
pub const HALVING_INTERVAL: u64 = 1_000_000;

/// Maximum number of halvings before reward reaches zero.
/// After 21 halvings, reward < 1 quanta → effectively zero.
pub const MAX_HALVINGS: u32 = 21;

/// Compute the block reward for a given block height.
/// Follows Bitcoin's halving model: reward halves every HALVING_INTERVAL blocks.
pub fn block_reward_at(height: u64) -> Amount {
    if height == 0 {
        return 0; // Genesis block has no reward.
    }
    let era = height / HALVING_INTERVAL;
    if era >= MAX_HALVINGS as u64 {
        return 0;
    }
    INITIAL_BLOCK_REWARD >> era as u32
}

/// Compute total mined supply up to (and including) a given block height.
pub fn total_mined_at(height: u64) -> Amount {
    if height == 0 {
        return 0;
    }
    let mut total: Amount = 0;
    let mut era_start: u64 = 1; // Block 1 is first reward block
    for era in 0..=MAX_HALVINGS {
        let reward = INITIAL_BLOCK_REWARD >> era;
        if reward == 0 {
            break;
        }
        let era_end = (era as u64 + 1) * HALVING_INTERVAL;
        let blocks_in_era = if height >= era_end {
            era_end.saturating_sub(era_start)
        } else {
            height.saturating_sub(era_start.saturating_sub(1))
        };
        total = total.saturating_add(reward.saturating_mul(blocks_in_era as u128));
        if height < era_end {
            break;
        }
        era_start = era_end;
    }
    // Cap at total supply
    if total > TOTAL_SUPPLY_QUANTA {
        total = TOTAL_SUPPLY_QUANTA;
    }
    total
}

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
    EscrowRefund { escrow_id: Hash },
    /// Register / update agent metadata on-chain.
    AgentRegister {
        /// Human-readable agent name (max 64 bytes, UTF-8).
        name: Vec<u8>,
        /// Service endpoint URL or multiaddr (max 256 bytes).
        endpoint: Vec<u8>,
        /// Capability tags for discovery (e.g., ["llm", "vision"]).
        capabilities: Vec<Vec<u8>>,
    },
    /// Create a milestone-based escrow with multiple release stages.
    /// Each milestone has its own hash-lock and amount, enabling sub-task payments.
    MilestoneEscrowCreate {
        recipient: Address,
        /// Individual milestones, each with a hash-lock and release amount.
        milestones: Vec<Milestone>,
        /// Unix-millisecond deadline after which the sender can reclaim unreleased funds.
        deadline: u64,
    },
    /// Release a single milestone within a milestone-based escrow.
    MilestoneRelease {
        escrow_id: Hash,
        /// Index of the milestone to release (0-based).
        milestone_index: u32,
        /// Pre-image for this milestone's hash-lock.
        preimage: Vec<u8>,
    },
    /// Set spending policy for account abstraction (auto-approve rules).
    SetSpendingPolicy {
        /// Auto-approve transfers up to this amount (0 = disabled).
        auto_approve_limit: Amount,
        /// Addresses that can co-sign transactions above the auto-approve limit.
        co_signers: Vec<Address>,
        /// Number of co-signers required for high-value transactions (0 = disabled).
        required_co_signers: u32,
    },
    /// Transfer that includes co-signer approvals (for amounts above auto_approve_limit).
    CoSignedTransfer {
        to: Address,
        amount: Amount,
        memo: Option<Vec<u8>>,
        /// Co-signer signatures over the signable_hash of this transaction.
        co_signatures: Vec<(Address, Signature)>,
    },
    /// Update agent pricing information.
    UpdateAgentPricing {
        /// Price per request in quanta.
        price_per_request: Amount,
        /// Billing model (max 32 bytes, e.g. "per-request").
        billing_model: Vec<u8>,
        /// Optional SLA description (max 256 bytes).
        sla_description: Vec<u8>,
    },
    /// Rate another agent (requires prior interaction via escrow/agreement).
    RateAgent {
        /// The agent being rated.
        target: Address,
        /// Rating score (1-5).
        rating: u8,
    },
    /// Create a recurring payment schedule.
    CreateRecurringPayment {
        recipient: Address,
        /// Amount per period in quanta.
        amount_per_period: Amount,
        /// Interval between payments in milliseconds.
        interval_ms: u64,
        /// Maximum number of payments (0 = unlimited).
        max_payments: u32,
    },
    /// Cancel a recurring payment.
    CancelRecurringPayment { payment_id: Hash },
    /// Propose a service agreement to another agent.
    CreateServiceAgreement {
        provider: Address,
        /// Description of the service (max 512 bytes).
        description: Vec<u8>,
        /// Total payment amount in quanta.
        payment_amount: Amount,
        /// Deadline for completion (Unix ms).
        deadline: u64,
    },
    /// Accept a proposed service agreement (locks payment in escrow).
    AcceptServiceAgreement { agreement_id: Hash },
    /// Mark a service agreement as completed (releases payment).
    CompleteServiceAgreement { agreement_id: Hash },
    /// Dispute a service agreement.
    DisputeServiceAgreement { agreement_id: Hash },
    /// Create a governance proposal.
    CreateProposal {
        /// Title (max 128 bytes).
        title: Vec<u8>,
        /// Description (max 1024 bytes).
        description: Vec<u8>,
        /// Voting deadline (Unix ms).
        voting_deadline: u64,
    },
    /// Cast a vote on a governance proposal.
    CastVote {
        proposal_id: Hash,
        /// True = for, false = against.
        in_favor: bool,
    },
    /// Create a sub-account with a delegated budget.
    CreateSubAccount {
        /// Label for the sub-account (max 64 bytes).
        label: Vec<u8>,
        /// Maximum spend budget in quanta.
        budget: Amount,
        /// Optional expiry (Unix ms, 0 = no expiry).
        expiry: u64,
    },
    /// Transfer from a sub-account's delegated budget.
    DelegatedTransfer {
        /// Sub-account ID (hash of creation tx).
        sub_account_id: Hash,
        /// Recipient address.
        to: Address,
        /// Amount in quanta.
        amount: Amount,
    },
    /// Set an arbitrator for a disputed service agreement.
    SetArbitrator {
        /// The agreement to assign an arbitrator for.
        agreement_id: Hash,
        /// The address of the trusted third-party arbitrator.
        arbitrator: Address,
    },
    /// Arbitrator resolves a disputed agreement (splits payment).
    ArbitrateDispute {
        /// The disputed agreement.
        agreement_id: Hash,
        /// Amount (in quanta) to pay the provider; remainder refunded to client.
        provider_amount: Amount,
    },
    /// Submit a batch of transactions for atomic execution.
    BatchTransfer {
        /// Individual transfers within the batch (max 32).
        transfers: Vec<BatchEntry>,
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
/// Maximum milestones per escrow.
pub const MAX_MILESTONES: usize = 32;
/// Maximum co-signers per spending policy.
pub const MAX_CO_SIGNERS: usize = 8;
/// Maximum co-signatures in a CoSignedTransfer.
pub const MAX_CO_SIGNATURES: usize = 8;
/// Maximum sub-account label length.
pub const MAX_SUB_ACCOUNT_LABEL_LEN: usize = 64;
/// Maximum entries in a BatchTransfer.
pub const MAX_BATCH_ENTRIES: usize = 32;

/// A single entry in a batch transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchEntry {
    /// Recipient address.
    pub to: Address,
    /// Amount in quanta.
    pub amount: Amount,
}

/// A single milestone in a milestone-based escrow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    /// Amount released when this milestone is completed.
    pub amount: Amount,
    /// BLAKE3 hash of the secret that proves milestone completion.
    pub hash_lock: Hash,
}

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
        let bytes = bincode::serialize(self).expect("serialization of tx should never fail");
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
                recipient,
                amount,
                deadline,
                ..
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
                    return Err(BaudError::InvalidEscrowProof("preimage too large".into()));
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
            TxPayload::MilestoneEscrowCreate {
                recipient,
                milestones,
                deadline,
            } => {
                if self.sender == *recipient {
                    return Err(BaudError::SelfTransfer);
                }
                if milestones.is_empty() || milestones.len() > MAX_MILESTONES {
                    return Err(BaudError::InvalidMilestoneCount {
                        count: milestones.len(),
                        max: MAX_MILESTONES,
                    });
                }
                for m in milestones {
                    if m.amount == 0 {
                        return Err(BaudError::ZeroAmount);
                    }
                }
                if *deadline <= self.timestamp {
                    return Err(BaudError::EscrowDeadlineExceeded {
                        current: self.timestamp,
                        deadline: *deadline,
                    });
                }
            }
            TxPayload::MilestoneRelease { preimage, .. } => {
                if preimage.len() > 1024 {
                    return Err(BaudError::InvalidEscrowProof("preimage too large".into()));
                }
            }
            TxPayload::SetSpendingPolicy {
                co_signers,
                required_co_signers,
                ..
            } => {
                if co_signers.len() > MAX_CO_SIGNERS {
                    return Err(BaudError::TransactionTooLarge {
                        size: co_signers.len(),
                        max: MAX_CO_SIGNERS,
                    });
                }
                if *required_co_signers > co_signers.len() as u32 {
                    return Err(BaudError::InvalidSpendingPolicy {
                        required: *required_co_signers,
                        available: co_signers.len(),
                    });
                }
            }
            TxPayload::CoSignedTransfer {
                to,
                amount,
                memo,
                co_signatures,
            } => {
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
                if co_signatures.len() > MAX_CO_SIGNATURES {
                    return Err(BaudError::TransactionTooLarge {
                        size: co_signatures.len(),
                        max: MAX_CO_SIGNATURES,
                    });
                }
            }
            TxPayload::UpdateAgentPricing {
                billing_model,
                sla_description,
                ..
            } => {
                if billing_model.len() > MAX_BILLING_MODEL_LEN {
                    return Err(BaudError::TransactionTooLarge {
                        size: billing_model.len(),
                        max: MAX_BILLING_MODEL_LEN,
                    });
                }
                if sla_description.len() > MAX_SLA_DESCRIPTION_LEN {
                    return Err(BaudError::TransactionTooLarge {
                        size: sla_description.len(),
                        max: MAX_SLA_DESCRIPTION_LEN,
                    });
                }
            }
            TxPayload::RateAgent { rating, target } => {
                if *rating < MIN_RATING || *rating > MAX_RATING {
                    return Err(BaudError::InvalidRating {
                        value: *rating,
                        min: MIN_RATING,
                        max: MAX_RATING,
                    });
                }
                if self.sender == *target {
                    return Err(BaudError::SelfTransfer);
                }
            }
            TxPayload::CreateRecurringPayment {
                recipient,
                amount_per_period,
                interval_ms,
                ..
            } => {
                if self.sender == *recipient {
                    return Err(BaudError::SelfTransfer);
                }
                if *amount_per_period == 0 {
                    return Err(BaudError::ZeroAmount);
                }
                if *interval_ms < MIN_RECURRING_INTERVAL || *interval_ms > MAX_RECURRING_INTERVAL {
                    return Err(BaudError::InvalidRecurringInterval {
                        interval: *interval_ms,
                    });
                }
            }
            TxPayload::CancelRecurringPayment { .. } => {}
            TxPayload::CreateServiceAgreement {
                provider,
                description,
                payment_amount,
                deadline,
            } => {
                if self.sender == *provider {
                    return Err(BaudError::SelfTransfer);
                }
                if *payment_amount == 0 {
                    return Err(BaudError::ZeroAmount);
                }
                if description.len() > MAX_AGREEMENT_DESCRIPTION_LEN {
                    return Err(BaudError::TransactionTooLarge {
                        size: description.len(),
                        max: MAX_AGREEMENT_DESCRIPTION_LEN,
                    });
                }
                if *deadline <= self.timestamp {
                    return Err(BaudError::EscrowDeadlineExceeded {
                        current: self.timestamp,
                        deadline: *deadline,
                    });
                }
            }
            TxPayload::AcceptServiceAgreement { .. } => {}
            TxPayload::CompleteServiceAgreement { .. } => {}
            TxPayload::DisputeServiceAgreement { .. } => {}
            TxPayload::CreateProposal {
                title,
                description,
                voting_deadline,
            } => {
                if title.len() > MAX_PROPOSAL_TITLE_LEN {
                    return Err(BaudError::TransactionTooLarge {
                        size: title.len(),
                        max: MAX_PROPOSAL_TITLE_LEN,
                    });
                }
                if description.len() > MAX_PROPOSAL_DESCRIPTION_LEN {
                    return Err(BaudError::TransactionTooLarge {
                        size: description.len(),
                        max: MAX_PROPOSAL_DESCRIPTION_LEN,
                    });
                }
                if *voting_deadline <= self.timestamp + MIN_VOTING_PERIOD {
                    return Err(BaudError::VotingPeriodTooShort {
                        minimum_ms: MIN_VOTING_PERIOD,
                    });
                }
            }
            TxPayload::CastVote { .. } => {}
            TxPayload::CreateSubAccount { label, budget, .. } => {
                if label.len() > MAX_SUB_ACCOUNT_LABEL_LEN {
                    return Err(BaudError::TransactionTooLarge {
                        size: label.len(),
                        max: MAX_SUB_ACCOUNT_LABEL_LEN,
                    });
                }
                if *budget == 0 {
                    return Err(BaudError::ZeroAmount);
                }
            }
            TxPayload::DelegatedTransfer { amount, .. } => {
                if *amount == 0 {
                    return Err(BaudError::ZeroAmount);
                }
            }
            TxPayload::SetArbitrator { .. } => {}
            TxPayload::ArbitrateDispute { .. } => {}
            TxPayload::BatchTransfer { transfers } => {
                if transfers.is_empty() || transfers.len() > MAX_BATCH_ENTRIES {
                    return Err(BaudError::TransactionTooLarge {
                        size: transfers.len(),
                        max: MAX_BATCH_ENTRIES,
                    });
                }
                for entry in transfers {
                    if entry.amount == 0 {
                        return Err(BaudError::ZeroAmount);
                    }
                    if self.sender == entry.to {
                        return Err(BaudError::SelfTransfer);
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
        let bytes = bincode::serialize(self).expect("header serialization should never fail");
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
    /// Optional spending policy for account abstraction.
    pub spending_policy: Option<SpendingPolicy>,
}

/// Programmable spending rules for account abstraction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendingPolicy {
    /// Auto-approve transfers up to this amount without co-signing.
    pub auto_approve_limit: Amount,
    /// Addresses authorized to co-sign high-value transactions.
    pub co_signers: Vec<Address>,
    /// Number of co-signers required (m-of-n).
    pub required_co_signers: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMeta {
    pub name: Vec<u8>,
    pub endpoint: Vec<u8>,
    pub capabilities: Vec<Vec<u8>>,
}

// ─── Agent Pricing ──────────────────────────────────────────────────────────

/// Pricing model for agent services (stored in ExtendedState).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPricing {
    /// Price per request in quanta.
    pub price_per_request: Amount,
    /// Billing model: "per-request", "per-second", "flat-rate".
    pub billing_model: Vec<u8>,
    /// Optional SLA description (max 256 bytes).
    pub sla_description: Vec<u8>,
}

/// Maximum SLA description length.
pub const MAX_SLA_DESCRIPTION_LEN: usize = 256;
/// Maximum billing model length.
pub const MAX_BILLING_MODEL_LEN: usize = 32;

// ─── Reputation ─────────────────────────────────────────────────────────────

/// Reputation score for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reputation {
    /// Cumulative score (sum of all ratings).
    pub total_score: u64,
    /// Number of ratings received.
    pub rating_count: u64,
    /// Number of successful jobs.
    pub successful_jobs: u64,
    /// Number of failed/disputed jobs.
    pub failed_jobs: u64,
}

impl Reputation {
    pub fn new() -> Self {
        Self {
            total_score: 0,
            rating_count: 0,
            successful_jobs: 0,
            failed_jobs: 0,
        }
    }

    /// Average score (1-5 scale, 0 if no ratings).
    pub fn average_score(&self) -> f64 {
        if self.rating_count == 0 {
            0.0
        } else {
            self.total_score as f64 / self.rating_count as f64
        }
    }
}

impl Default for Reputation {
    fn default() -> Self {
        Self::new()
    }
}

/// Maximum rating value (1-5 scale).
pub const MAX_RATING: u8 = 5;
/// Minimum rating value.
pub const MIN_RATING: u8 = 1;

// ─── Recurring Payments ─────────────────────────────────────────────────────

/// A recurring payment schedule between two agents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecurringPaymentStatus {
    Active,
    Cancelled,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringPayment {
    pub id: Hash,
    pub sender: Address,
    pub recipient: Address,
    /// Amount per payment in quanta.
    pub amount_per_period: Amount,
    /// Interval between payments in milliseconds.
    pub interval_ms: u64,
    /// Unix-ms timestamp of last payment execution.
    pub last_executed: u64,
    /// Maximum number of payments (0 = unlimited).
    pub max_payments: u32,
    /// Number of payments executed so far.
    pub payments_made: u32,
    pub status: RecurringPaymentStatus,
    pub created_at_height: u64,
}

/// Maximum interval for recurring payments (1 year in ms).
pub const MAX_RECURRING_INTERVAL: u64 = 365 * 24 * 60 * 60 * 1000;
/// Minimum interval for recurring payments (1 minute in ms).
pub const MIN_RECURRING_INTERVAL: u64 = 60_000;

// ─── Service Agreements ─────────────────────────────────────────────────────

/// Status of a service agreement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgreementStatus {
    Proposed,
    Accepted,
    Completed,
    Disputed,
    Cancelled,
}

/// A bilateral service agreement between two agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceAgreement {
    pub id: Hash,
    pub client: Address,
    pub provider: Address,
    /// Description of the service (max 512 bytes).
    pub description: Vec<u8>,
    /// Total payment amount in quanta (locked in escrow on acceptance).
    pub payment_amount: Amount,
    /// Deadline for service completion (Unix ms).
    pub deadline: u64,
    pub status: AgreementStatus,
    pub created_at_height: u64,
}

/// Maximum service agreement description length.
pub const MAX_AGREEMENT_DESCRIPTION_LEN: usize = 512;

// ─── Governance ─────────────────────────────────────────────────────────────

/// Status of a governance proposal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProposalStatus {
    Active,
    Passed,
    Rejected,
    Executed,
}

/// A governance proposal that token holders can vote on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: Hash,
    pub proposer: Address,
    /// Title (max 128 bytes).
    pub title: Vec<u8>,
    /// Description (max 1024 bytes).
    pub description: Vec<u8>,
    /// Voting deadline (Unix ms).
    pub voting_deadline: u64,
    /// Total votes for.
    pub votes_for: Amount,
    /// Total votes against.
    pub votes_against: Amount,
    /// Minimum quorum required (total votes / total supply).
    pub quorum: Amount,
    pub status: ProposalStatus,
    pub created_at_height: u64,
}

/// A single vote cast by a token holder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub voter: Address,
    pub proposal_id: Hash,
    /// True = for, false = against.
    pub in_favor: bool,
    /// Weight = voter's balance at time of voting.
    pub weight: Amount,
}

/// Maximum proposal title length.
pub const MAX_PROPOSAL_TITLE_LEN: usize = 128;
/// Maximum proposal description length.
pub const MAX_PROPOSAL_DESCRIPTION_LEN: usize = 1024;
/// Minimum voting period (1 hour in ms).
pub const MIN_VOTING_PERIOD: u64 = 3_600_000;

// ─── Sub-accounts ───────────────────────────────────────────────────────────

/// A delegated sub-account with a spending budget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAccount {
    pub id: Hash,
    /// The parent account that created this sub-account.
    pub owner: Address,
    /// Human-readable label.
    pub label: Vec<u8>,
    /// Maximum budget in quanta.
    pub budget: Amount,
    /// Amount already spent.
    pub spent: Amount,
    /// Expiry timestamp (Unix ms, 0 = no expiry).
    pub expiry: u64,
    /// Block height at which the sub-account was created.
    pub created_at_height: u64,
}

// ─── Extended State ─────────────────────────────────────────────────────────

/// Extended state for new features (stored separately from WorldState to
/// maintain backward compatibility with existing serialized chain data).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtendedState {
    /// Agent pricing indexed by address.
    pub agent_pricing: std::collections::HashMap<Address, AgentPricing>,
    /// Reputation scores indexed by address.
    pub reputation: std::collections::HashMap<Address, Reputation>,
    /// Recurring payments indexed by ID.
    pub recurring_payments: std::collections::HashMap<Hash, RecurringPayment>,
    /// Service agreements indexed by ID.
    pub service_agreements: std::collections::HashMap<Hash, ServiceAgreement>,
    /// Governance proposals indexed by ID.
    pub proposals: std::collections::HashMap<Hash, Proposal>,
    /// Votes per proposal: proposal_id → list of votes.
    pub votes: std::collections::HashMap<Hash, Vec<Vote>>,
    /// Sub-accounts indexed by ID.
    pub sub_accounts: std::collections::HashMap<Hash, SubAccount>,
    /// Arbitrators assigned to disputed agreements: agreement_id → arbitrator address.
    pub arbitrators: std::collections::HashMap<Hash, Address>,
}

impl Account {
    pub fn new(address: Address) -> Self {
        Self {
            address,
            balance: 0,
            nonce: 0,
            agent_meta: None,
            spending_policy: None,
        }
    }

    pub fn with_balance(address: Address, balance: Amount) -> Self {
        Self {
            address,
            balance,
            nonce: 0,
            agent_meta: None,
            spending_policy: None,
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

/// A milestone-based escrow with multiple sub-task release stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneEscrow {
    pub id: Hash,
    pub sender: Address,
    pub recipient: Address,
    /// Total amount locked (sum of all milestone amounts).
    pub total_amount: Amount,
    /// Individual milestones with completion status.
    pub milestones: Vec<MilestoneState>,
    /// Amount already released across completed milestones.
    pub released_amount: Amount,
    pub deadline: u64,
    pub status: EscrowStatus,
    pub created_at_height: u64,
}

/// Runtime state of a single milestone within a milestone escrow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneState {
    pub amount: Amount,
    pub hash_lock: Hash,
    pub completed: bool,
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
