use thiserror::Error;

/// All errors that can occur in the Baud ledger core.
#[derive(Debug, Error)]
pub enum BaudError {
    // ── Cryptographic ───────────────────────────────────────────────
    #[error("invalid signature for address {0}")]
    InvalidSignature(String),

    #[error("invalid public key bytes")]
    InvalidPublicKey,

    #[error("invalid secret key bytes")]
    InvalidSecretKey,

    #[error("signature verification failed: {0}")]
    VerificationFailed(String),

    // ── Transaction ─────────────────────────────────────────────────
    #[error("insufficient balance: have {have}, need {need}")]
    InsufficientBalance { have: u128, need: u128 },

    #[error("invalid nonce: expected {expected}, got {got}")]
    InvalidNonce { expected: u64, got: u64 },

    #[error("transaction expired at timestamp {0}")]
    TransactionExpired(u64),

    #[error("duplicate transaction: {0}")]
    DuplicateTransaction(String),

    #[error("self-transfer is not permitted")]
    SelfTransfer,

    #[error("zero-amount transfer is not permitted")]
    ZeroAmount,

    #[error("transaction too large: {size} bytes exceeds max {max} bytes")]
    TransactionTooLarge { size: usize, max: usize },

    #[error("chain ID mismatch: expected {expected}, got {got}")]
    ChainIdMismatch { expected: String, got: String },

    // ── Escrow ──────────────────────────────────────────────────────
    #[error("escrow not found: {0}")]
    EscrowNotFound(String),

    #[error("escrow already finalized: {0}")]
    EscrowAlreadyFinalized(String),

    #[error("escrow deadline not reached: current {current}, deadline {deadline}")]
    EscrowDeadlineNotReached { current: u64, deadline: u64 },

    #[error("escrow deadline exceeded: current {current}, deadline {deadline}")]
    EscrowDeadlineExceeded { current: u64, deadline: u64 },

    #[error("unauthorized escrow operation: {0}")]
    EscrowUnauthorized(String),

    #[error("invalid escrow proof: {0}")]
    InvalidEscrowProof(String),

    #[error("invalid milestone count: {count} (max {max})")]
    InvalidMilestoneCount { count: usize, max: usize },

    #[error("milestone index {index} out of range (total: {total})")]
    MilestoneIndexOutOfRange { index: u32, total: usize },

    #[error("milestone {index} already completed")]
    MilestoneAlreadyCompleted { index: u32 },

    #[error("spending policy requires co-signer approval for amount {amount} (limit: {limit})")]
    SpendingPolicyViolation { amount: u128, limit: u128 },

    #[error("invalid spending policy: required_co_signers ({required}) exceeds co_signers count ({available})")]
    InvalidSpendingPolicy { required: u32, available: usize },

    #[error("nonce gap too large: current nonce {current}, got {got}, max gap {max_gap}")]
    NonceGapTooLarge {
        current: u64,
        got: u64,
        max_gap: u64,
    },

    #[error("too many active escrows for account (max: {max})")]
    TooManyEscrows { max: usize },

    // ── Block ───────────────────────────────────────────────────────
    #[error("invalid block: {0}")]
    InvalidBlock(String),

    #[error("block height mismatch: expected {expected}, got {got}")]
    BlockHeightMismatch { expected: u64, got: u64 },

    #[error("invalid previous block hash")]
    InvalidPrevHash,

    #[error("invalid state root")]
    InvalidStateRoot,

    #[error("invalid transactions root")]
    InvalidTxRoot,

    // ── State ───────────────────────────────────────────────────────
    #[error("account not found: {0}")]
    AccountNotFound(String),

    #[error("genesis already initialized")]
    GenesisAlreadyInitialized,

    #[error("genesis allocation overflow")]
    GenesisOverflow,

    #[error("genesis total supply exceeded: allocated {allocated} quanta, max {max} quanta")]
    GenesisTotalSupplyExceeded { allocated: u128, max: u128 },

    // ── Mempool ─────────────────────────────────────────────────────
    #[error("mempool is full (capacity: {0})")]
    MempoolFull(usize),

    // ── Serialization ───────────────────────────────────────────────
    #[error("serialization error: {0}")]
    Serialization(String),

    // ── Overflow ────────────────────────────────────────────────────
    #[error("arithmetic overflow in balance computation")]
    Overflow,
}

pub type BaudResult<T> = Result<T, BaudError>;
