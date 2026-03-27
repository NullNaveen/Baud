use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Maximum number of active escrows (regular + milestone) a single account can hold.
const MAX_ESCROWS_PER_ACCOUNT: usize = 1000;

use crate::crypto::{verify_signature, Address, Hash};
use crate::error::{BaudError, BaudResult};
use crate::types::{
    Account, AgentMeta, AgentPricing, AgreementStatus, Amount, Escrow, EscrowStatus, ExtendedState,
    MilestoneEscrow, MilestoneState, Proposal, ProposalStatus, RecurringPayment,
    RecurringPaymentStatus, ServiceAgreement, SpendingPolicy, SubAccount, Transaction,
    TxPayload, Vote,
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
    /// Extended state for new features (stored separately, not in main serialization).
    #[serde(skip)]
    pub extended: ExtendedState,
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
            extended: ExtendedState::default(),
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
    /// of all accounts, escrows, and milestone escrows.
    pub fn state_root(&self) -> Hash {
        let mut sorted_accounts: Vec<_> = self.accounts.iter().collect();
        sorted_accounts.sort_by_key(|(addr, _)| addr.0);

        let mut sorted_escrows: Vec<_> = self.escrows.iter().collect();
        sorted_escrows.sort_by_key(|(id, _)| id.0);

        let mut sorted_milestone_escrows: Vec<_> = self.milestone_escrows.iter().collect();
        sorted_milestone_escrows.sort_by_key(|(id, _)| id.0);

        let data = (&sorted_accounts, &sorted_escrows, &sorted_milestone_escrows);
        let bytes = bincode::serialize(&data).expect("state serialization should never fail");
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
        // For CoSignedTransfer, the sender signs the hash with empty
        // co_signatures to avoid a circular dependency.
        let signable = match &tx.payload {
            TxPayload::CoSignedTransfer { .. } => {
                let mut verify_tx = tx.clone();
                if let TxPayload::CoSignedTransfer {
                    ref mut co_signatures,
                    ..
                } = verify_tx.payload
                {
                    co_signatures.clear();
                }
                verify_tx.signable_hash()
            }
            _ => tx.signable_hash(),
        };
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
            TxPayload::CoSignedTransfer {
                amount,
                co_signatures,
                ..
            } => {
                if account.balance < *amount {
                    return Err(BaudError::InsufficientBalance {
                        have: account.balance,
                        need: *amount,
                    });
                }
                // Verify co-signer signatures against spending policy.
                // Co-signers sign the hash computed with empty co_signatures
                // to avoid circular dependency.
                if let Some(ref policy) = account.spending_policy {
                    if *amount > policy.auto_approve_limit && policy.required_co_signers > 0 {
                        let mut verify_tx = tx.clone();
                        if let TxPayload::CoSignedTransfer {
                            co_signatures: ref mut sigs,
                            ..
                        } = verify_tx.payload
                        {
                            sigs.clear();
                        }
                        let co_sign_hash = verify_tx.signable_hash();

                        let mut valid_co_signers = 0u32;
                        for (co_addr, co_sig) in co_signatures {
                            if !policy.co_signers.contains(co_addr) {
                                return Err(BaudError::CoSignerValidationFailed(format!(
                                    "{} is not an authorized co-signer",
                                    co_addr
                                )));
                            }
                            verify_signature(co_addr, co_sign_hash.as_bytes(), co_sig)?;
                            valid_co_signers += 1;
                        }
                        if valid_co_signers < policy.required_co_signers {
                            return Err(BaudError::CoSignerValidationFailed(format!(
                                "need {} co-signers, got {}",
                                policy.required_co_signers, valid_co_signers
                            )));
                        }
                    }
                }
            }
            TxPayload::UpdateAgentPricing { .. } => {
                // Sender updates their own pricing — no additional state checks.
            }
            TxPayload::RateAgent { target, .. } => {
                // Target must exist as a registered agent.
                let target_account = self.get_account(target);
                if target_account.agent_meta.is_none() {
                    return Err(BaudError::AccountNotFound(format!(
                        "target {} is not a registered agent",
                        target
                    )));
                }
            }
            TxPayload::CreateRecurringPayment {
                amount_per_period, ..
            } => {
                // Sender must have at least one period's worth.
                if account.balance < *amount_per_period {
                    return Err(BaudError::InsufficientBalance {
                        have: account.balance,
                        need: *amount_per_period,
                    });
                }
            }
            TxPayload::CancelRecurringPayment { payment_id } => {
                let payment = self
                    .extended
                    .recurring_payments
                    .get(payment_id)
                    .ok_or_else(|| BaudError::RecurringPaymentNotFound(payment_id.to_hex()))?;
                if payment.sender != tx.sender {
                    return Err(BaudError::CoSignerValidationFailed(
                        "only the sender can cancel a recurring payment".into(),
                    ));
                }
                if payment.status != RecurringPaymentStatus::Active {
                    return Err(BaudError::RecurringPaymentNotFound(
                        "payment is not active".into(),
                    ));
                }
            }
            TxPayload::CreateServiceAgreement { payment_amount, .. } => {
                if account.balance < *payment_amount {
                    return Err(BaudError::InsufficientBalance {
                        have: account.balance,
                        need: *payment_amount,
                    });
                }
            }
            TxPayload::AcceptServiceAgreement { agreement_id } => {
                let agreement = self
                    .extended
                    .service_agreements
                    .get(agreement_id)
                    .ok_or_else(|| BaudError::AgreementNotFound(agreement_id.to_hex()))?;
                if agreement.provider != tx.sender {
                    return Err(BaudError::AgreementUnauthorized(
                        "only the provider can accept".into(),
                    ));
                }
                if agreement.status != AgreementStatus::Proposed {
                    return Err(BaudError::InvalidAgreementStatus(format!(
                        "{:?}",
                        agreement.status
                    )));
                }
            }
            TxPayload::CompleteServiceAgreement { agreement_id } => {
                let agreement = self
                    .extended
                    .service_agreements
                    .get(agreement_id)
                    .ok_or_else(|| BaudError::AgreementNotFound(agreement_id.to_hex()))?;
                // Client confirms completion.
                if agreement.client != tx.sender {
                    return Err(BaudError::AgreementUnauthorized(
                        "only the client can mark as completed".into(),
                    ));
                }
                if agreement.status != AgreementStatus::Accepted {
                    return Err(BaudError::InvalidAgreementStatus(format!(
                        "{:?}",
                        agreement.status
                    )));
                }
            }
            TxPayload::DisputeServiceAgreement { agreement_id } => {
                let agreement = self
                    .extended
                    .service_agreements
                    .get(agreement_id)
                    .ok_or_else(|| BaudError::AgreementNotFound(agreement_id.to_hex()))?;
                // Either party can dispute.
                if agreement.client != tx.sender && agreement.provider != tx.sender {
                    return Err(BaudError::AgreementUnauthorized(
                        "only client or provider can dispute".into(),
                    ));
                }
                if agreement.status != AgreementStatus::Accepted {
                    return Err(BaudError::InvalidAgreementStatus(format!(
                        "{:?}",
                        agreement.status
                    )));
                }
            }
            TxPayload::CreateProposal { .. } => {
                // Any account can create a proposal.
            }
            TxPayload::CastVote { proposal_id, .. } => {
                let proposal = self
                    .extended
                    .proposals
                    .get(proposal_id)
                    .ok_or_else(|| BaudError::ProposalNotFound(proposal_id.to_hex()))?;
                // Check for duplicate votes BEFORE status check so the
                // error is correct even if the proposal already resolved.
                if let Some(votes) = self.extended.votes.get(proposal_id) {
                    if votes.iter().any(|v| v.voter == tx.sender) {
                        return Err(BaudError::AlreadyVoted);
                    }
                }
                if proposal.status != ProposalStatus::Active {
                    return Err(BaudError::ProposalNotFound("proposal is not active".into()));
                }
                if current_time > proposal.voting_deadline {
                    return Err(BaudError::VotingPeriodEnded);
                }
            }
            TxPayload::CreateSubAccount { budget, .. } => {
                if account.balance < *budget {
                    return Err(BaudError::InsufficientBalance {
                        have: account.balance,
                        need: *budget,
                    });
                }
            }
            TxPayload::DelegatedTransfer {
                sub_account_id,
                amount,
                ..
            } => {
                let sub = self
                    .extended
                    .sub_accounts
                    .get(sub_account_id)
                    .ok_or_else(|| BaudError::SubAccountNotFound(sub_account_id.to_hex()))?;
                if sub.owner != tx.sender {
                    return Err(BaudError::SubAccountUnauthorized(
                        "only the owner can spend from a sub-account".into(),
                    ));
                }
                if sub.expiry > 0 && current_time > sub.expiry {
                    return Err(BaudError::SubAccountExpired(sub.expiry));
                }
                let remaining = sub.budget.saturating_sub(sub.spent);
                if *amount > remaining {
                    return Err(BaudError::SubAccountBudgetExceeded {
                        remaining,
                        need: *amount,
                    });
                }
            }
            TxPayload::SetArbitrator { agreement_id, .. } => {
                let agreement = self
                    .extended
                    .service_agreements
                    .get(agreement_id)
                    .ok_or_else(|| BaudError::AgreementNotFound(agreement_id.to_hex()))?;
                if agreement.status != AgreementStatus::Disputed {
                    return Err(BaudError::InvalidAgreementStatus(format!(
                        "{:?} (must be Disputed)",
                        agreement.status
                    )));
                }
                // Both client and provider must agree on arbitrator;
                // either party can propose one.
                if agreement.client != tx.sender && agreement.provider != tx.sender {
                    return Err(BaudError::AgreementUnauthorized(
                        "only client or provider can set an arbitrator".into(),
                    ));
                }
            }
            TxPayload::ArbitrateDispute {
                agreement_id,
                provider_amount,
            } => {
                let agreement = self
                    .extended
                    .service_agreements
                    .get(agreement_id)
                    .ok_or_else(|| BaudError::AgreementNotFound(agreement_id.to_hex()))?;
                if agreement.status != AgreementStatus::Disputed {
                    return Err(BaudError::InvalidAgreementStatus(format!(
                        "{:?} (must be Disputed)",
                        agreement.status
                    )));
                }
                let arbitrator = self
                    .extended
                    .arbitrators
                    .get(agreement_id)
                    .ok_or_else(|| BaudError::ArbitratorNotSet(agreement_id.to_hex()))?;
                if tx.sender != *arbitrator {
                    return Err(BaudError::ArbitratorUnauthorized(
                        "only the assigned arbitrator can resolve".into(),
                    ));
                }
                if *provider_amount > agreement.payment_amount {
                    return Err(BaudError::InsufficientBalance {
                        have: agreement.payment_amount,
                        need: *provider_amount,
                    });
                }
            }
            TxPayload::BatchTransfer { transfers } => {
                let total: Amount = transfers
                    .iter()
                    .try_fold(0u128, |acc, e| acc.checked_add(e.amount))
                    .ok_or(BaudError::Overflow)?;
                if account.balance < total {
                    return Err(BaudError::BatchTotalExceedsBalance {
                        have: account.balance,
                        need: total,
                    });
                }
            }
        }

        Ok(())
    }

    /// Apply a **validated** transaction to the state.
    /// Caller MUST have called `validate_transaction` first.
    ///
    /// All balance mutations use checked arithmetic to prevent overflow/underflow.
    /// Nonce is incremented last so a payload failure does not consume a nonce.
    pub fn apply_transaction(&mut self, tx: &Transaction) -> BaudResult<()> {
        let sender_addr = tx.sender;

        match &tx.payload {
            TxPayload::Transfer { to, amount, .. } => {
                // Debit sender (checked).
                {
                    let sender = self
                        .accounts
                        .get_mut(&sender_addr)
                        .ok_or_else(|| BaudError::AccountNotFound(sender_addr.to_hex()))?;
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
                    let sender = self
                        .accounts
                        .get_mut(&sender_addr)
                        .ok_or_else(|| BaudError::AccountNotFound(sender_addr.to_hex()))?;
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
                let escrow = self
                    .escrows
                    .get_mut(escrow_id)
                    .ok_or_else(|| BaudError::EscrowNotFound(escrow_id.to_hex()))?;
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
                let escrow = self
                    .escrows
                    .get_mut(escrow_id)
                    .ok_or_else(|| BaudError::EscrowNotFound(escrow_id.to_hex()))?;
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
                let acc = self
                    .accounts
                    .get_mut(&sender_addr)
                    .ok_or_else(|| BaudError::AccountNotFound(sender_addr.to_hex()))?;
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
                    let sender = self
                        .accounts
                        .get_mut(&sender_addr)
                        .ok_or_else(|| BaudError::AccountNotFound(sender_addr.to_hex()))?;
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
                let escrow = self
                    .milestone_escrows
                    .get_mut(escrow_id)
                    .ok_or_else(|| BaudError::EscrowNotFound(escrow_id.to_hex()))?;
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
                let acc = self
                    .accounts
                    .get_mut(&sender_addr)
                    .ok_or_else(|| BaudError::AccountNotFound(sender_addr.to_hex()))?;
                acc.spending_policy = Some(SpendingPolicy {
                    auto_approve_limit: *auto_approve_limit,
                    co_signers: co_signers.clone(),
                    required_co_signers: *required_co_signers,
                });
                debug!(address = %sender_addr, limit = %auto_approve_limit, "spending policy set");
            }

            TxPayload::CoSignedTransfer { to, amount, .. } => {
                // Same as Transfer but co-signatures already validated.
                {
                    let sender = self
                        .accounts
                        .get_mut(&sender_addr)
                        .ok_or_else(|| BaudError::AccountNotFound(sender_addr.to_hex()))?;
                    sender.balance = sender.balance.checked_sub(*amount).ok_or(
                        BaudError::InsufficientBalance {
                            have: sender.balance,
                            need: *amount,
                        },
                    )?;
                }
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
                    "co-signed transfer applied"
                );
            }

            TxPayload::UpdateAgentPricing {
                price_per_request,
                billing_model,
                sla_description,
            } => {
                self.extended.agent_pricing.insert(
                    sender_addr,
                    AgentPricing {
                        price_per_request: *price_per_request,
                        billing_model: billing_model.clone(),
                        sla_description: sla_description.clone(),
                    },
                );
                debug!(address = %sender_addr, price = %price_per_request, "agent pricing updated");
            }

            TxPayload::RateAgent { target, rating } => {
                let rep = self
                    .extended
                    .reputation
                    .entry(*target)
                    .or_default();
                rep.total_score = rep.total_score.saturating_add(*rating as u64);
                rep.rating_count = rep.rating_count.saturating_add(1);
                debug!(target = %target, rating = rating, avg = %rep.average_score(), "agent rated");
            }

            TxPayload::CreateRecurringPayment {
                recipient,
                amount_per_period,
                interval_ms,
                max_payments,
            } => {
                let payment_id = tx.hash();
                let payment = RecurringPayment {
                    id: payment_id,
                    sender: sender_addr,
                    recipient: *recipient,
                    amount_per_period: *amount_per_period,
                    interval_ms: *interval_ms,
                    last_executed: tx.timestamp,
                    max_payments: *max_payments,
                    payments_made: 0,
                    status: RecurringPaymentStatus::Active,
                    created_at_height: self.height,
                };
                self.extended.recurring_payments.insert(payment_id, payment);
                debug!(id = %payment_id, amount = %amount_per_period, "recurring payment created");
            }

            TxPayload::CancelRecurringPayment { payment_id } => {
                if let Some(payment) = self.extended.recurring_payments.get_mut(payment_id) {
                    payment.status = RecurringPaymentStatus::Cancelled;
                }
                debug!(id = %payment_id, "recurring payment cancelled");
            }

            TxPayload::CreateServiceAgreement {
                provider,
                description,
                payment_amount,
                deadline,
            } => {
                // Lock funds from client.
                {
                    let sender = self
                        .accounts
                        .get_mut(&sender_addr)
                        .ok_or_else(|| BaudError::AccountNotFound(sender_addr.to_hex()))?;
                    sender.balance = sender.balance.checked_sub(*payment_amount).ok_or(
                        BaudError::InsufficientBalance {
                            have: sender.balance,
                            need: *payment_amount,
                        },
                    )?;
                }
                let agreement_id = tx.hash();
                let agreement = ServiceAgreement {
                    id: agreement_id,
                    client: sender_addr,
                    provider: *provider,
                    description: description.clone(),
                    payment_amount: *payment_amount,
                    deadline: *deadline,
                    status: AgreementStatus::Proposed,
                    created_at_height: self.height,
                };
                self.extended
                    .service_agreements
                    .insert(agreement_id, agreement);
                debug!(id = %agreement_id, amount = %payment_amount, "service agreement created");
            }

            TxPayload::AcceptServiceAgreement { agreement_id } => {
                if let Some(agreement) = self.extended.service_agreements.get_mut(agreement_id) {
                    agreement.status = AgreementStatus::Accepted;
                }
                debug!(id = %agreement_id, "service agreement accepted");
            }

            TxPayload::CompleteServiceAgreement { agreement_id } => {
                let agreement = self
                    .extended
                    .service_agreements
                    .get_mut(agreement_id)
                    .ok_or_else(|| BaudError::AgreementNotFound(agreement_id.to_hex()))?;
                agreement.status = AgreementStatus::Completed;
                let provider = agreement.provider;
                let amount = agreement.payment_amount;

                // Release payment to provider.
                let acc = self
                    .accounts
                    .entry(provider)
                    .or_insert_with(|| Account::new(provider));
                acc.balance = acc.balance.checked_add(amount).ok_or(BaudError::Overflow)?;

                // Update reputation: successful job for provider.
                let rep = self
                    .extended
                    .reputation
                    .entry(provider)
                    .or_default();
                rep.successful_jobs = rep.successful_jobs.saturating_add(1);
                debug!(id = %agreement_id, "service agreement completed, payment released");
            }

            TxPayload::DisputeServiceAgreement { agreement_id } => {
                let agreement = self
                    .extended
                    .service_agreements
                    .get_mut(agreement_id)
                    .ok_or_else(|| BaudError::AgreementNotFound(agreement_id.to_hex()))?;
                agreement.status = AgreementStatus::Disputed;
                let client = agreement.client;
                let provider = agreement.provider;
                let amount = agreement.payment_amount;

                // Refund full amount to client on dispute (simple v1 resolution).
                let acc = self
                    .accounts
                    .entry(client)
                    .or_insert_with(|| Account::new(client));
                acc.balance = acc.balance.checked_add(amount).ok_or(BaudError::Overflow)?;

                // Update reputation: failed job for provider.
                let rep = self
                    .extended
                    .reputation
                    .entry(provider)
                    .or_default();
                rep.failed_jobs = rep.failed_jobs.saturating_add(1);
                debug!(id = %agreement_id, "service agreement disputed, payment refunded");
            }

            TxPayload::CreateProposal {
                title,
                description,
                voting_deadline,
            } => {
                let proposal_id = tx.hash();
                // Quorum = 10% of total accounts (minimum 1).
                let quorum = std::cmp::max(1, self.accounts.len() / 10) as u128;
                let proposal = Proposal {
                    id: proposal_id,
                    proposer: sender_addr,
                    title: title.clone(),
                    description: description.clone(),
                    voting_deadline: *voting_deadline,
                    votes_for: 0,
                    votes_against: 0,
                    quorum,
                    status: ProposalStatus::Active,
                    created_at_height: self.height,
                };
                self.extended.proposals.insert(proposal_id, proposal);
                debug!(id = %proposal_id, "governance proposal created");
            }

            TxPayload::CastVote {
                proposal_id,
                in_favor,
            } => {
                let voter_balance = self.balance_of(&sender_addr);
                let vote = Vote {
                    voter: sender_addr,
                    proposal_id: *proposal_id,
                    in_favor: *in_favor,
                    weight: voter_balance,
                };
                // Update proposal tallies.
                if let Some(proposal) = self.extended.proposals.get_mut(proposal_id) {
                    if *in_favor {
                        proposal.votes_for = proposal
                            .votes_for
                            .checked_add(voter_balance)
                            .ok_or(BaudError::Overflow)?;
                    } else {
                        proposal.votes_against = proposal
                            .votes_against
                            .checked_add(voter_balance)
                            .ok_or(BaudError::Overflow)?;
                    }
                    // Check if quorum reached and resolve.
                    let total_votes = proposal.votes_for.saturating_add(proposal.votes_against);
                    if total_votes >= proposal.quorum {
                        if proposal.votes_for > proposal.votes_against {
                            proposal.status = ProposalStatus::Passed;
                        } else {
                            proposal.status = ProposalStatus::Rejected;
                        }
                    }
                }
                self.extended
                    .votes
                    .entry(*proposal_id)
                    .or_default()
                    .push(vote);
                debug!(proposal = %proposal_id, in_favor = in_favor, "vote cast");
            }

            TxPayload::CreateSubAccount {
                label,
                budget,
                expiry,
            } => {
                // Debit owner's balance to fund the sub-account budget.
                {
                    let sender = self
                        .accounts
                        .get_mut(&sender_addr)
                        .ok_or_else(|| BaudError::AccountNotFound(sender_addr.to_hex()))?;
                    sender.balance = sender.balance.checked_sub(*budget).ok_or(
                        BaudError::InsufficientBalance {
                            have: sender.balance,
                            need: *budget,
                        },
                    )?;
                }
                let sub_id = tx.hash();
                let sub = SubAccount {
                    id: sub_id,
                    owner: sender_addr,
                    label: label.clone(),
                    budget: *budget,
                    spent: 0,
                    expiry: *expiry,
                    created_at_height: self.height,
                };
                self.extended.sub_accounts.insert(sub_id, sub);
                debug!(id = %sub_id, budget = %budget, "sub-account created");
            }

            TxPayload::DelegatedTransfer {
                sub_account_id,
                to,
                amount,
            } => {
                // Debit from sub-account budget (funds already locked).
                let sub = self
                    .extended
                    .sub_accounts
                    .get_mut(sub_account_id)
                    .ok_or_else(|| BaudError::SubAccountNotFound(sub_account_id.to_hex()))?;
                sub.spent = sub.spent.checked_add(*amount).ok_or(BaudError::Overflow)?;

                // Credit recipient.
                let rec = self
                    .accounts
                    .entry(*to)
                    .or_insert_with(|| Account::new(*to));
                rec.balance = rec
                    .balance
                    .checked_add(*amount)
                    .ok_or(BaudError::Overflow)?;
                debug!(sub = %sub_account_id, to = %to, amount = %amount, "delegated transfer");
            }

            TxPayload::SetArbitrator {
                agreement_id,
                arbitrator,
            } => {
                self.extended.arbitrators.insert(*agreement_id, *arbitrator);
                debug!(agreement = %agreement_id, arbitrator = %arbitrator, "arbitrator set");
            }

            TxPayload::ArbitrateDispute {
                agreement_id,
                provider_amount,
            } => {
                let agreement = self
                    .extended
                    .service_agreements
                    .get_mut(agreement_id)
                    .ok_or_else(|| BaudError::AgreementNotFound(agreement_id.to_hex()))?;

                let total = agreement.payment_amount;
                let client_refund = total.saturating_sub(*provider_amount);
                let provider_addr = agreement.provider;
                let client_addr = agreement.client;

                // Mark as resolved (Completed after arbitration).
                agreement.status = AgreementStatus::Completed;

                // Pay provider their share.
                if *provider_amount > 0 {
                    let prov = self
                        .accounts
                        .entry(provider_addr)
                        .or_insert_with(|| Account::new(provider_addr));
                    prov.balance = prov
                        .balance
                        .checked_add(*provider_amount)
                        .ok_or(BaudError::Overflow)?;
                }
                // Refund client the remainder.
                if client_refund > 0 {
                    let cli = self
                        .accounts
                        .entry(client_addr)
                        .or_insert_with(|| Account::new(client_addr));
                    cli.balance = cli
                        .balance
                        .checked_add(client_refund)
                        .ok_or(BaudError::Overflow)?;
                }

                // Remove arbitrator assignment.
                self.extended.arbitrators.remove(agreement_id);
                debug!(
                    agreement = %agreement_id,
                    provider_amount = %provider_amount,
                    client_refund = %client_refund,
                    "dispute arbitrated"
                );
            }

            TxPayload::BatchTransfer { transfers } => {
                // Debit total from sender.
                let total: Amount = transfers
                    .iter()
                    .try_fold(0u128, |acc, e| acc.checked_add(e.amount))
                    .ok_or(BaudError::Overflow)?;
                {
                    let sender = self
                        .accounts
                        .get_mut(&sender_addr)
                        .ok_or_else(|| BaudError::AccountNotFound(sender_addr.to_hex()))?;
                    sender.balance = sender.balance.checked_sub(total).ok_or(
                        BaudError::InsufficientBalance {
                            have: sender.balance,
                            need: total,
                        },
                    )?;
                }
                // Credit each recipient.
                for entry in transfers {
                    let rec = self
                        .accounts
                        .entry(entry.to)
                        .or_insert_with(|| Account::new(entry.to));
                    rec.balance = rec
                        .balance
                        .checked_add(entry.amount)
                        .ok_or(BaudError::Overflow)?;
                }
                debug!(count = transfers.len(), total = %total, "batch transfer applied");
            }
        }

        // Increment sender nonce last — only after successful payload application.
        {
            let sender = self
                .accounts
                .entry(sender_addr)
                .or_insert_with(|| Account::new(sender_addr));
            sender.nonce = sender.nonce.checked_add(1).ok_or(BaudError::Overflow)?;
        }

        Ok(())
    }

    /// Apply a full block of transactions. Returns error on the first invalid tx.
    /// Application is atomic: if any transaction fails or the state root
    /// doesn't match, the original state is left unchanged.
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

        // Clone state for atomic application — mutations go to `new` first.
        let mut new = self.clone();

        // Apply all transactions.
        for tx in &block.transactions {
            new.apply_transaction(tx)?;
        }

        // ── Mining reward ───────────────────────────────────────────────
        // Distribute block reward to the proposer (coinbase).
        let reward = crate::types::block_reward_at(block.header.height);
        if reward > 0 {
            let proposer = block.header.proposer;
            let account = new
                .accounts
                .entry(proposer)
                .or_insert_with(|| Account::new(proposer));
            account.balance = account
                .balance
                .checked_add(reward)
                .ok_or(BaudError::Overflow)?;

            debug!(
                height = block.header.height,
                proposer = %proposer,
                reward_baud = reward / crate::types::QUANTA_PER_BAUD,
                "block reward distributed"
            );
        }

        // Update chain tip.
        new.height = block.header.height;
        new.last_block_hash = block.header.hash();

        // Verify state root after application.
        let computed_root = new.state_root();
        if block.header.state_root != computed_root {
            warn!(
                expected = %block.header.state_root,
                computed = %computed_root,
                "state root mismatch — this indicates a consensus divergence"
            );
            return Err(BaudError::InvalidStateRoot);
        }

        // All checks passed — commit the new state atomically.
        *self = new;
        Ok(())
    }

    /// Get total mined supply so far.
    pub fn total_mined(&self) -> Amount {
        crate::types::total_mined_at(self.height)
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
