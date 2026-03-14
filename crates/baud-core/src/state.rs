use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Maximum number of active escrows (regular + milestone) a single account can hold.
const MAX_ESCROWS_PER_ACCOUNT: usize = 1000;

use crate::crypto::{verify_signature, Address, Hash};
use crate::error::{BaudError, BaudResult};
use crate::types::{
    Account, AgentMeta, Amount, Escrow, EscrowStatus, MilestoneEscrow, MilestoneState,
    SpendingPolicy, Transaction, TxPayload,
};

// ─── World State ────────────────────────────────────────────────────────────

/// The complete world state of the Baud ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldState {
    /// All accounts indexed by address.
    pub accounts: HashMap<Address, Account>,
    /// All active and finalized escrows indexed by ID.
    pub escrows: HashMap<Hash, Escrow>,
    /// Milestone-based escrows indexed by ID.
    pub milestone_escrows: HashMap<Hash, MilestoneEscrow>,
    /// Current block height.
    pub height: u64,
    /// Hash of the last finalized block header.
    pub last_block_hash: Hash,
    /// Chain ID for replay-protection across forks.
    pub chain_id: String,
}

impl WorldState {
    pub fn new(chain_id: String) -> Self {
        Self {
            accounts: HashMap::new(),
            escrows: HashMap::new(),
            milestone_escrows: HashMap::new(),
            height: 0,
            last_block_hash: Hash::zero(),
            chain_id,
        }
    }

    /// Initialize from genesis config.
    pub fn from_genesis(config: &crate::types::GenesisConfig) -> BaudResult<Self> {
        use crate::types::TOTAL_SUPPLY_QUANTA;
        let mut state = Self::new(config.chain_id.clone());

        let mut total_allocated: Amount = 0;
        for alloc in &config.allocations {
            total_allocated = total_allocated
                .checked_add(alloc.balance)
                .ok_or(BaudError::GenesisOverflow)?;
            let account = Account::with_balance(alloc.address, alloc.balance);
            state.accounts.insert(alloc.address, account);
        }

        if total_allocated > TOTAL_SUPPLY_QUANTA {
            return Err(BaudError::GenesisTotalSupplyExceeded {
                allocated: total_allocated,
                max: TOTAL_SUPPLY_QUANTA,
            });
        }

        debug!(
            accounts = state.accounts.len(),
            total_allocated_baud = total_allocated / crate::types::QUANTA_PER_BAUD,
            "genesis state initialized"
        );
        Ok(state)
    }

    /// Get account, returning a default (zero-balance) account if not found.
    pub fn get_account(&self, address: &Address) -> Account {
        self.accounts
            .get(address)
            .cloned()
            .unwrap_or_else(|| Account::new(*address))
    }

    /// Get account balance.
    pub fn balance_of(&self, address: &Address) -> Amount {
        self.accounts.get(address).map(|a| a.balance).unwrap_or(0)
    }

    /// Compute the state root: BLAKE3 hash of the deterministic serialization
    /// of all accounts sorted by address.
    pub fn state_root(&self) -> Hash {
        let mut sorted: Vec<_> = self.accounts.iter().collect();
        sorted.sort_by_key(|(addr, _)| addr.0);

        let bytes = bincode::serialize(&sorted).expect("state serialization should never fail");
        Hash::digest(&bytes)
    }

    // ── Transaction validation & application ────────────────────────────

    /// Fully validate a transaction against the current state.
    /// This checks: structure, signature, nonce, balance, and payload-specific rules.
    pub fn validate_transaction(&self, tx: &Transaction, current_time: u64) -> BaudResult<()> {
        // 1. Structural validation
        tx.validate_structure()?;

        // 2. Chain ID must match (prevents cross-chain replay attacks).
        if tx.chain_id != self.chain_id {
            return Err(BaudError::ChainIdMismatch {
                expected: self.chain_id.clone(),
                got: tx.chain_id.clone(),
            });
        }

        // 3. Reject transactions too far in the future (30 seconds tolerance).
        if tx.timestamp > current_time.saturating_add(30_000) {
            return Err(BaudError::TransactionExpired(tx.timestamp));
        }

        // 3. Signature verification
        let signable = tx.signable_hash();
        verify_signature(&tx.sender, signable.as_bytes(), &tx.signature)?;

        // 4. Nonce check
        let account = self.get_account(&tx.sender);
        if tx.nonce != account.nonce {
            return Err(BaudError::InvalidNonce {
                expected: account.nonce,
                got: tx.nonce,
            });
        }

        // 5. Payload-specific state validation
        match &tx.payload {
            TxPayload::Transfer { amount, .. } => {
                if account.balance < *amount {
                    return Err(BaudError::InsufficientBalance {
                        have: account.balance,
                        need: *amount,
                    });
                }
                // Spending policy enforcement
                if let Some(ref policy) = account.spending_policy {
                    if *amount > policy.auto_approve_limit && policy.required_co_signers > 0 {
                        return Err(BaudError::SpendingPolicyViolation {
                            amount: *amount,
                            limit: policy.auto_approve_limit,
                        });
                    }
                }
            }
            TxPayload::EscrowCreate { amount, .. } => {
                if account.balance < *amount {
                    return Err(BaudError::InsufficientBalance {
                        have: account.balance,
                        need: *amount,
                    });
                }
                // Spending policy enforcement
                if let Some(ref policy) = account.spending_policy {
                    if *amount > policy.auto_approve_limit && policy.required_co_signers > 0 {
                        return Err(BaudError::SpendingPolicyViolation {
                            amount: *amount,
                            limit: policy.auto_approve_limit,
                        });
                    }
                }
            }
            TxPayload::EscrowRelease {
                escrow_id,
                preimage,
            } => {
                let escrow = self
                    .escrows
                    .get(escrow_id)
                    .ok_or_else(|| BaudError::EscrowNotFound(escrow_id.to_hex()))?;
                if escrow.status != EscrowStatus::Active {
                    return Err(BaudError::EscrowAlreadyFinalized(escrow_id.to_hex()));
                }
                // Only the recipient can release.
                if tx.sender != escrow.recipient {
                    return Err(BaudError::EscrowUnauthorized(
                        "only recipient can release".into(),
                    ));
                }
                // Verify hash-lock: BLAKE3(preimage) must equal hash_lock.
                let preimage_hash = Hash::digest(preimage);
                if preimage_hash != escrow.hash_lock {
                    return Err(BaudError::InvalidEscrowProof(
                        "preimage does not match hash_lock".into(),
                    ));
                }
                // Must be before deadline.
                if current_time > escrow.deadline {
                    return Err(BaudError::EscrowDeadlineExceeded {
                        current: current_time,
                        deadline: escrow.deadline,
                    });
                }
            }
            TxPayload::EscrowRefund { escrow_id } => {
                let escrow = self
                    .escrows
                    .get(escrow_id)
                    .ok_or_else(|| BaudError::EscrowNotFound(escrow_id.to_hex()))?;
                if escrow.status != EscrowStatus::Active {
                    return Err(BaudError::EscrowAlreadyFinalized(escrow_id.to_hex()));
                }
                // Only the sender can refund.
                if tx.sender != escrow.sender {
                    return Err(BaudError::EscrowUnauthorized(
                        "only sender can refund".into(),
                    ));
                }
                // Deadline must have passed.
                if current_time < escrow.deadline {
                    return Err(BaudError::EscrowDeadlineNotReached {
                        current: current_time,
                        deadline: escrow.deadline,
                    });
                }
            }
            TxPayload::AgentRegister { .. } => {
                // No additional state checks needed.
            }
            TxPayload::MilestoneEscrowCreate { milestones, .. } => {
                // Sum all milestone amounts and check balance.
                let total: Amount = milestones
                    .iter()
                    .try_fold(0u128, |acc, m| acc.checked_add(m.amount))
                    .ok_or(BaudError::Overflow)?;
                if account.balance < total {
                    return Err(BaudError::InsufficientBalance {
                        have: account.balance,
                        need: total,
                    });
                }
            }
            TxPayload::MilestoneRelease {
                escrow_id,
                milestone_index,
                preimage,
            } => {
                let escrow = self
                    .milestone_escrows
                    .get(escrow_id)
                    .ok_or_else(|| BaudError::EscrowNotFound(escrow_id.to_hex()))?;
                if escrow.status != EscrowStatus::Active {
                    return Err(BaudError::EscrowAlreadyFinalized(escrow_id.to_hex()));
                }
                if tx.sender != escrow.recipient {
                    return Err(BaudError::EscrowUnauthorized(
                        "only recipient can release milestones".into(),
                    ));
                }
                let idx = *milestone_index as usize;
                if idx >= escrow.milestones.len() {
                    return Err(BaudError::MilestoneIndexOutOfRange {
                        index: *milestone_index,
                        total: escrow.milestones.len(),
                    });
                }
                if escrow.milestones[idx].completed {
                    return Err(BaudError::MilestoneAlreadyCompleted {
                        index: *milestone_index,
                    });
                }
                let preimage_hash = Hash::digest(preimage);
                if preimage_hash != escrow.milestones[idx].hash_lock {
                    return Err(BaudError::InvalidEscrowProof(
                        "preimage does not match milestone hash_lock".into(),
                    ));
                }
                if current_time > escrow.deadline {
                    return Err(BaudError::EscrowDeadlineExceeded {
                        current: current_time,
                        deadline: escrow.deadline,
                    });
                }
            }
            TxPayload::SetSpendingPolicy { .. } => {
                // Sender is setting their own policy — no additional checks.
            }
        }

        Ok(())
    }

    /// Apply a **validated** transaction to the state.
    /// Caller MUST have called `validate_transaction` first.
    ///
    /// All balance mutations use checked arithmetic to prevent overflow/underflow.
    pub fn apply_transaction(&mut self, tx: &Transaction) -> BaudResult<()> {
        let sender_addr = tx.sender;

        // Increment sender nonce first (prevents reentrancy-like issues).
        {
            let sender = self
                .accounts
                .entry(sender_addr)
                .or_insert_with(|| Account::new(sender_addr));
            sender.nonce = sender.nonce.checked_add(1).ok_or(BaudError::Overflow)?;
        }

        match &tx.payload {
            TxPayload::Transfer { to, amount, .. } => {
                // Debit sender (checked).
                {
                    let sender = self.accounts.get_mut(&sender_addr).unwrap();
                    sender.balance = sender.balance.checked_sub(*amount).ok_or(
                        BaudError::InsufficientBalance {
                            have: sender.balance,
                            need: *amount,
                        },
                    )?;
                }
                // Credit recipient (checked).
                {
                    let recipient = self
                        .accounts
                        .entry(*to)
                        .or_insert_with(|| Account::new(*to));
                    recipient.balance = recipient
                        .balance
                        .checked_add(*amount)
                        .ok_or(BaudError::Overflow)?;
                }
                debug!(
                    from = %sender_addr,
                    to = %to,
                    amount = %amount,
                    "transfer applied"
                );
            }

            TxPayload::EscrowCreate {
                recipient,
                amount,
                hash_lock,
                deadline,
            } => {
                // Enforce per-account escrow limit.
                let sender_escrow_count = self
                    .escrows
                    .values()
                    .filter(|e| e.sender == sender_addr && e.status == EscrowStatus::Active)
                    .count()
                    + self
                        .milestone_escrows
                        .values()
                        .filter(|e| e.sender == sender_addr && e.status == EscrowStatus::Active)
                        .count();
                if sender_escrow_count >= MAX_ESCROWS_PER_ACCOUNT {
                    return Err(BaudError::TooManyEscrows {
                        max: MAX_ESCROWS_PER_ACCOUNT,
                    });
                }
                // Debit sender.
                {
                    let sender = self.accounts.get_mut(&sender_addr).unwrap();
                    sender.balance = sender.balance.checked_sub(*amount).ok_or(
                        BaudError::InsufficientBalance {
                            have: sender.balance,
                            need: *amount,
                        },
                    )?;
                }
                // Create escrow entry.
                let escrow_id = tx.hash();
                let escrow = Escrow {
                    id: escrow_id,
                    sender: sender_addr,
                    recipient: *recipient,
                    amount: *amount,
                    hash_lock: *hash_lock,
                    deadline: *deadline,
                    status: EscrowStatus::Active,
                    created_at_height: self.height,
                };
                self.escrows.insert(escrow_id, escrow);
                debug!(escrow_id = %escrow_id, amount = %amount, "escrow created");
            }

            TxPayload::EscrowRelease {
                escrow_id,
                preimage: _,
            } => {
                // State change first (prevents logical reentrancy).
                let escrow = self.escrows.get_mut(escrow_id).unwrap();
                escrow.status = EscrowStatus::Released;
                let recipient = escrow.recipient;
                let amount = escrow.amount;

                // Credit recipient.
                let rec = self
                    .accounts
                    .entry(recipient)
                    .or_insert_with(|| Account::new(recipient));
                rec.balance = rec.balance.checked_add(amount).ok_or(BaudError::Overflow)?;
                debug!(escrow_id = %escrow_id, "escrow released");
            }

            TxPayload::EscrowRefund { escrow_id } => {
                let escrow = self.escrows.get_mut(escrow_id).unwrap();
                escrow.status = EscrowStatus::Refunded;
                let sender = escrow.sender;
                let amount = escrow.amount;

                // Refund to original sender.
                let acc = self
                    .accounts
                    .entry(sender)
                    .or_insert_with(|| Account::new(sender));
                acc.balance = acc.balance.checked_add(amount).ok_or(BaudError::Overflow)?;
                debug!(escrow_id = %escrow_id, "escrow refunded");
            }

            TxPayload::AgentRegister {
                name,
                endpoint,
                capabilities,
            } => {
                let acc = self.accounts.get_mut(&sender_addr).unwrap();
                acc.agent_meta = Some(AgentMeta {
                    name: name.clone(),
                    endpoint: endpoint.clone(),
                    capabilities: capabilities.clone(),
                });
                debug!(address = %sender_addr, "agent registered");
            }

            TxPayload::MilestoneEscrowCreate {
                recipient,
                milestones,
                deadline,
            } => {
                // Enforce per-account escrow limit.
                let sender_escrow_count = self
                    .escrows
                    .values()
                    .filter(|e| e.sender == sender_addr && e.status == EscrowStatus::Active)
                    .count()
                    + self
                        .milestone_escrows
                        .values()
                        .filter(|e| e.sender == sender_addr && e.status == EscrowStatus::Active)
                        .count();
                if sender_escrow_count >= MAX_ESCROWS_PER_ACCOUNT {
                    return Err(BaudError::TooManyEscrows {
                        max: MAX_ESCROWS_PER_ACCOUNT,
                    });
                }
                // Sum total and debit sender.
                let total: Amount = milestones
                    .iter()
                    .try_fold(0u128, |acc, m| acc.checked_add(m.amount))
                    .ok_or(BaudError::Overflow)?;
                {
                    let sender = self.accounts.get_mut(&sender_addr).unwrap();
                    sender.balance = sender.balance.checked_sub(total).ok_or(
                        BaudError::InsufficientBalance {
                            have: sender.balance,
                            need: total,
                        },
                    )?;
                }
                let escrow_id = tx.hash();
                let milestone_states: Vec<MilestoneState> = milestones
                    .iter()
                    .map(|m| MilestoneState {
                        amount: m.amount,
                        hash_lock: m.hash_lock,
                        completed: false,
                    })
                    .collect();
                let escrow = MilestoneEscrow {
                    id: escrow_id,
                    sender: sender_addr,
                    recipient: *recipient,
                    total_amount: total,
                    milestones: milestone_states,
                    released_amount: 0,
                    deadline: *deadline,
                    status: EscrowStatus::Active,
                    created_at_height: self.height,
                };
                self.milestone_escrows.insert(escrow_id, escrow);
                debug!(escrow_id = %escrow_id, milestones = milestones.len(), total = %total, "milestone escrow created");
            }

            TxPayload::MilestoneRelease {
                escrow_id,
                milestone_index,
                preimage: _,
            } => {
                let escrow = self.milestone_escrows.get_mut(escrow_id).unwrap();
                let idx = *milestone_index as usize;
                let amount = escrow.milestones[idx].amount;
                escrow.milestones[idx].completed = true;
                escrow.released_amount = escrow
                    .released_amount
                    .checked_add(amount)
                    .ok_or(BaudError::Overflow)?;

                // If all milestones completed, mark escrow as released.
                if escrow.milestones.iter().all(|m| m.completed) {
                    escrow.status = EscrowStatus::Released;
                }

                // Credit recipient.
                let recipient = escrow.recipient;
                let rec = self
                    .accounts
                    .entry(recipient)
                    .or_insert_with(|| Account::new(recipient));
                rec.balance = rec.balance.checked_add(amount).ok_or(BaudError::Overflow)?;
                debug!(escrow_id = %escrow_id, milestone = idx, amount = %amount, "milestone released");
            }

            TxPayload::SetSpendingPolicy {
                auto_approve_limit,
                co_signers,
                required_co_signers,
            } => {
                let acc = self.accounts.get_mut(&sender_addr).unwrap();
                acc.spending_policy = Some(SpendingPolicy {
                    auto_approve_limit: *auto_approve_limit,
                    co_signers: co_signers.clone(),
                    required_co_signers: *required_co_signers,
                });
                debug!(address = %sender_addr, limit = %auto_approve_limit, "spending policy set");
            }
        }

        Ok(())
    }

    /// Apply a full block of transactions. Returns error on the first invalid tx.
    pub fn apply_block(&mut self, block: &crate::types::Block) -> BaudResult<()> {
        let expected_height = self.height.checked_add(1).ok_or(BaudError::Overflow)?;
        if block.header.height != expected_height {
            if block.header.height == 0 && self.height == 0 {
                // Genesis block.
            } else {
                return Err(BaudError::BlockHeightMismatch {
                    expected: expected_height,
                    got: block.header.height,
                });
            }
        }

        // Verify prev_hash (skip for genesis).
        if block.header.height > 0 && block.header.prev_hash != self.last_block_hash {
            return Err(BaudError::InvalidPrevHash);
        }

        // Apply all transactions.
        for tx in &block.transactions {
            self.apply_transaction(tx)?;
        }

        // Update chain tip.
        self.height = block.header.height;
        self.last_block_hash = block.header.hash();

        // Verify state root after application.
        let computed_root = self.state_root();
        if block.header.state_root != computed_root {
            warn!(
                expected = %block.header.state_root,
                computed = %computed_root,
                "state root mismatch — this indicates a consensus divergence"
            );
            return Err(BaudError::InvalidStateRoot);
        }

        Ok(())
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KeyPair;

    fn make_transfer(kp: &KeyPair, to: Address, amount: Amount, nonce: u64) -> Transaction {
        let payload = TxPayload::Transfer {
            to,
            amount,
            memo: None,
        };
        let timestamp = 1_000_000u64;
        let mut tx = Transaction {
            sender: kp.address(),
            nonce,
            payload,
            timestamp,
            chain_id: "test".into(),
            signature: crate::crypto::Signature::zero(),
        };
        let hash = tx.signable_hash();
        tx.signature = kp.sign(hash.as_bytes());
        tx
    }

    #[test]
    fn transfer_basic() {
        let alice = KeyPair::generate();
        let bob = KeyPair::generate();

        let mut state = WorldState::new("test".into());
        state.accounts.insert(
            alice.address(),
            Account::with_balance(alice.address(), 1000),
        );

        let tx = make_transfer(&alice, bob.address(), 100, 0);
        state
            .validate_transaction(&tx, 1_000_000)
            .expect("should validate");
        state.apply_transaction(&tx).expect("should apply");

        assert_eq!(state.balance_of(&alice.address()), 900);
        assert_eq!(state.balance_of(&bob.address()), 100);
    }

    #[test]
    fn transfer_insufficient_balance() {
        let alice = KeyPair::generate();
        let bob = KeyPair::generate();

        let mut state = WorldState::new("test".into());
        state
            .accounts
            .insert(alice.address(), Account::with_balance(alice.address(), 50));

        let tx = make_transfer(&alice, bob.address(), 100, 0);
        let result = state.validate_transaction(&tx, 1_000_000);
        assert!(matches!(result, Err(BaudError::InsufficientBalance { .. })));
    }

    #[test]
    fn transfer_wrong_nonce() {
        let alice = KeyPair::generate();
        let bob = KeyPair::generate();

        let mut state = WorldState::new("test".into());
        state.accounts.insert(
            alice.address(),
            Account::with_balance(alice.address(), 1000),
        );

        let tx = make_transfer(&alice, bob.address(), 100, 5);
        let result = state.validate_transaction(&tx, 1_000_000);
        assert!(matches!(result, Err(BaudError::InvalidNonce { .. })));
    }

    #[test]
    fn escrow_create_release() {
        let alice = KeyPair::generate();
        let bob = KeyPair::generate();

        let mut state = WorldState::new("test".into());
        state.accounts.insert(
            alice.address(),
            Account::with_balance(alice.address(), 1000),
        );

        let secret = b"my_secret_preimage";
        let hash_lock = Hash::digest(secret);
        let deadline = 2_000_000u64;

        // Create escrow
        let create_payload = TxPayload::EscrowCreate {
            recipient: bob.address(),
            amount: 500,
            hash_lock,
            deadline,
        };
        let mut create_tx = Transaction {
            sender: alice.address(),
            nonce: 0,
            payload: create_payload,
            timestamp: 1_000_000,
            chain_id: "test".into(),
            signature: crate::crypto::Signature::zero(),
        };
        let h = create_tx.signable_hash();
        create_tx.signature = alice.sign(h.as_bytes());

        state
            .validate_transaction(&create_tx, 1_000_000)
            .expect("create should validate");
        state
            .apply_transaction(&create_tx)
            .expect("create should apply");

        assert_eq!(state.balance_of(&alice.address()), 500);
        let escrow_id = create_tx.hash();

        // Release escrow
        let release_payload = TxPayload::EscrowRelease {
            escrow_id,
            preimage: secret.to_vec(),
        };
        let mut release_tx = Transaction {
            sender: bob.address(),
            nonce: 0,
            payload: release_payload,
            timestamp: 1_500_000,
            chain_id: "test".into(),
            signature: crate::crypto::Signature::zero(),
        };
        let h = release_tx.signable_hash();
        release_tx.signature = bob.sign(h.as_bytes());

        state
            .validate_transaction(&release_tx, 1_500_000)
            .expect("release should validate");
        state
            .apply_transaction(&release_tx)
            .expect("release should apply");

        assert_eq!(state.balance_of(&bob.address()), 500);
        assert_eq!(
            state.escrows.get(&escrow_id).unwrap().status,
            EscrowStatus::Released
        );
    }

    #[test]
    fn escrow_refund_after_deadline() {
        let alice = KeyPair::generate();
        let bob = KeyPair::generate();

        let mut state = WorldState::new("test".into());
        state.accounts.insert(
            alice.address(),
            Account::with_balance(alice.address(), 1000),
        );

        let hash_lock = Hash::digest(b"secret");
        let deadline = 2_000_000u64;

        let create_payload = TxPayload::EscrowCreate {
            recipient: bob.address(),
            amount: 300,
            hash_lock,
            deadline,
        };
        let mut create_tx = Transaction {
            sender: alice.address(),
            nonce: 0,
            payload: create_payload,
            timestamp: 1_000_000,
            chain_id: "test".into(),
            signature: crate::crypto::Signature::zero(),
        };
        let h = create_tx.signable_hash();
        create_tx.signature = alice.sign(h.as_bytes());
        state.apply_transaction(&create_tx).unwrap();

        let escrow_id = create_tx.hash();

        // Refund after deadline
        let refund_payload = TxPayload::EscrowRefund { escrow_id };
        let mut refund_tx = Transaction {
            sender: alice.address(),
            nonce: 1,
            payload: refund_payload,
            timestamp: 3_000_000,
            chain_id: "test".into(),
            signature: crate::crypto::Signature::zero(),
        };
        let h = refund_tx.signable_hash();
        refund_tx.signature = alice.sign(h.as_bytes());

        state
            .validate_transaction(&refund_tx, 3_000_000)
            .expect("refund should validate after deadline");
        state
            .apply_transaction(&refund_tx)
            .expect("refund should apply");

        assert_eq!(state.balance_of(&alice.address()), 1000);
    }
}
