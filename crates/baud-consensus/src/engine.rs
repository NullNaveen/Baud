use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

use baud_core::crypto::{merkle_root, verify_signature, Address, Hash, KeyPair, Signature};
use baud_core::error::{BaudError, BaudResult};
use baud_core::mempool::Mempool;
use baud_core::state::WorldState;
use baud_core::types::{Block, BlockHeader};

// ─── Configuration ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    /// Maximum transactions per block.
    pub max_txs_per_block: usize,
    /// Block production interval in milliseconds.
    pub block_interval_ms: u64,
    /// Number of votes required (must be > 2/3 of validators).
    /// Computed automatically from the validator set size.
    pub quorum_threshold: usize,
    /// Timeout for collecting votes (milliseconds).
    pub vote_timeout_ms: u64,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            max_txs_per_block: 10_000,
            block_interval_ms: 1_000,
            quorum_threshold: 1, // Will be recomputed.
            vote_timeout_ms: 5_000,
        }
    }
}

// ─── Consensus Messages ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusMessage {
    /// Leader proposes a new block.
    Proposal(Block),
    /// Validator votes to accept/reject a proposed block.
    VoteMsg(Vote),
    /// Block has been finalized with sufficient votes.
    Finalized(FinalizedBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    /// The block hash being voted on.
    pub block_hash: Hash,
    /// Height of the proposed block.
    pub height: u64,
    /// The validator casting this vote.
    pub voter: Address,
    /// Accept or reject.
    pub accept: bool,
    /// Signature over (block_hash, height, accept).
    pub signature: Signature,
}

impl Vote {
    pub fn signable_bytes(&self) -> Vec<u8> {
        bincode::serialize(&(&self.block_hash, self.height, self.accept))
            .expect("vote serialization should never fail")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizedBlock {
    pub block: Block,
    pub votes: Vec<Vote>,
}

// ─── Consensus Engine ───────────────────────────────────────────────────────

/// A simplified BFT consensus engine with round-robin leader selection.
///
/// Security properties:
/// - Requires >2/3 validator votes for finality (BFT assumption).
/// - Round-robin leader rotation prevents single-point-of-failure.
/// - All votes are signature-verified.
/// - State transitions are deterministic — all honest validators converge.
pub struct ConsensusEngine {
    /// Our validator identity.
    keypair: Arc<KeyPair>,
    /// The current validator set (address → weight, all weight=1 for now).
    validators: Arc<RwLock<Vec<Address>>>,
    /// The shared world state.
    state: Arc<RwLock<WorldState>>,
    /// The shared mempool.
    mempool: Arc<Mempool>,
    /// Configuration.
    config: ConsensusConfig,
    /// Channel to broadcast finalized blocks to other subsystems.
    finalized_tx: broadcast::Sender<FinalizedBlock>,
    /// Channel to receive consensus messages from the network.
    consensus_rx: Arc<RwLock<Option<mpsc::Receiver<ConsensusMessage>>>>,
    /// Channel to send consensus messages to the network.
    consensus_tx: mpsc::Sender<ConsensusMessage>,
    /// Collected votes for the current round.
    pending_votes: Arc<RwLock<HashMap<Hash, Vec<Vote>>>>,
}

impl ConsensusEngine {
    pub fn new(
        keypair: Arc<KeyPair>,
        validators: Vec<Address>,
        state: Arc<RwLock<WorldState>>,
        mempool: Arc<Mempool>,
        config: ConsensusConfig,
    ) -> (Self, broadcast::Receiver<FinalizedBlock>, mpsc::Sender<ConsensusMessage>) {
        let (finalized_tx, finalized_rx) = broadcast::channel(256);
        let (consensus_tx, consensus_rx) = mpsc::channel(1024);

        let quorum = Self::compute_quorum(validators.len());

        let mut cfg = config;
        cfg.quorum_threshold = quorum;

        let engine = Self {
            keypair,
            validators: Arc::new(RwLock::new(validators)),
            state,
            mempool,
            config: cfg,
            finalized_tx,
            consensus_rx: Arc::new(RwLock::new(Some(consensus_rx))),
            consensus_tx: consensus_tx.clone(),
            pending_votes: Arc::new(RwLock::new(HashMap::new())),
        };

        (engine, finalized_rx, consensus_tx)
    }

    /// Byzantine quorum: floor(2n/3) + 1
    fn compute_quorum(n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (2 * n / 3) + 1
    }

    /// Determine the leader for a given block height (round-robin).
    pub fn leader_for_height(&self, height: u64) -> Address {
        let validators = self.validators.read();
        let idx = (height as usize) % validators.len();
        validators[idx]
    }

    /// Check if we are the leader for the next block.
    pub fn is_our_turn(&self) -> bool {
        let next_height = self.state.read().height.saturating_add(1);
        self.leader_for_height(next_height) == self.keypair.address()
    }

    /// Propose a new block from the current mempool.
    pub fn propose_block(&self) -> BaudResult<Block> {
        let state = self.state.read();
        let height = state.height.checked_add(1).ok_or(BaudError::Overflow)?;
        let prev_hash = state.last_block_hash;

        // Gather transactions from mempool.
        let txs = self.mempool.get_pending(self.config.max_txs_per_block);

        // Compute transaction Merkle root.
        let tx_hashes: Vec<Hash> = txs.iter().map(|tx| tx.hash()).collect();
        let _tx_root = merkle_root(&tx_hashes);

        // We need to compute the state root after applying these txs.
        // Clone state, apply txs, get root.
        let mut preview_state = state.clone();
        drop(state); // Release the read lock.

        let mut valid_txs = Vec::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        for tx in txs {
            match preview_state.validate_transaction(&tx, now) {
                Ok(()) => {
                    if let Err(e) = preview_state.apply_transaction(&tx) {
                        warn!(tx = %tx.hash(), err = %e, "tx apply failed in proposal");
                        continue;
                    }
                    valid_txs.push(tx);
                }
                Err(e) => {
                    debug!(tx = %tx.hash(), err = %e, "tx invalid for proposal, skipping");
                }
            }
        }

        // Recompute roots with only valid txs.
        let valid_tx_hashes: Vec<Hash> = valid_txs.iter().map(|tx| tx.hash()).collect();
        let final_tx_root = merkle_root(&valid_tx_hashes);
        let state_root = preview_state.state_root();

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut header = BlockHeader {
            height,
            prev_hash,
            tx_root: final_tx_root,
            state_root,
            timestamp,
            proposer: self.keypair.address(),
            tx_count: valid_txs.len() as u32,
            signature: Signature::zero(),
        };

        // Sign the header.
        let header_hash = header.signable_hash();
        header.signature = self.keypair.sign(header_hash.as_bytes());

        let block = Block {
            header,
            transactions: valid_txs,
        };

        info!(
            height = block.header.height,
            txs = block.transactions.len(),
            "block proposed"
        );

        Ok(block)
    }

    /// Cast our vote on a proposed block.
    pub fn vote_on_proposal(&self, block: &Block) -> BaudResult<Vote> {
        // Validate the block.
        let accept = self.validate_proposal(block);

        let block_hash = block.header.hash();
        let mut vote = Vote {
            block_hash,
            height: block.header.height,
            voter: self.keypair.address(),
            accept,
            signature: Signature::zero(),
        };

        let signable = vote.signable_bytes();
        vote.signature = self.keypair.sign(&signable);

        Ok(vote)
    }

    /// Validate whether a proposed block is acceptable.
    fn validate_proposal(&self, block: &Block) -> bool {
        let state = self.state.read();

        // Check height continuity.
        let expected = state.height.saturating_add(1);
        if block.header.height != expected {
            warn!(
                expected = expected,
                got = block.header.height,
                "proposal height mismatch"
            );
            return false;
        }

        // Check prev_hash.
        if block.header.prev_hash != state.last_block_hash {
            warn!("proposal has wrong prev_hash");
            return false;
        }

        // Check leader.
        let expected_leader = self.leader_for_height(block.header.height);
        if block.header.proposer != expected_leader {
            warn!(
                expected = %expected_leader,
                got = %block.header.proposer,
                "wrong proposer for this height"
            );
            return false;
        }

        // Verify proposer signature.
        let header_hash = block.header.signable_hash();
        if verify_signature(&block.header.proposer, header_hash.as_bytes(), &block.header.signature)
            .is_err()
        {
            warn!("invalid proposer signature");
            return false;
        }

        // Verify tx root.
        let tx_hashes: Vec<Hash> = block.transactions.iter().map(|tx| tx.hash()).collect();
        let computed_tx_root = merkle_root(&tx_hashes);
        if computed_tx_root != block.header.tx_root {
            warn!("tx root mismatch");
            return false;
        }

        // Verify each transaction signature.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut preview = state.clone();
        drop(state);

        for tx in &block.transactions {
            if let Err(e) = preview.validate_transaction(tx, now) {
                warn!(tx = %tx.hash(), err = %e, "invalid tx in proposal");
                return false;
            }
            if let Err(e) = preview.apply_transaction(tx) {
                warn!(tx = %tx.hash(), err = %e, "tx application failed");
                return false;
            }
        }

        // Verify state root.
        if preview.state_root() != block.header.state_root {
            warn!("state root mismatch after applying proposal");
            return false;
        }

        true
    }

    /// Process an incoming vote message.
    pub fn process_vote(&self, vote: &Vote) -> BaudResult<Option<FinalizedBlock>> {
        // Verify voter is in the validator set.
        {
            let validators = self.validators.read();
            if !validators.contains(&vote.voter) {
                return Err(BaudError::InvalidSignature(
                    "voter is not a validator".into(),
                ));
            }
        }

        // Verify vote signature.
        let signable = vote.signable_bytes();
        verify_signature(&vote.voter, &signable, &vote.signature)?;

        if !vote.accept {
            debug!(voter = %vote.voter, height = vote.height, "received reject vote");
            return Ok(None);
        }

        // Collect the vote.
        let mut pending = self.pending_votes.write();
        let votes = pending.entry(vote.block_hash).or_default();

        // Prevent double-voting by same validator.
        if votes.iter().any(|v| v.voter == vote.voter) {
            debug!(voter = %vote.voter, "duplicate vote ignored");
            return Ok(None);
        }

        votes.push(vote.clone());
        let vote_count = votes.len();

        debug!(
            block = %vote.block_hash,
            votes = vote_count,
            quorum = self.config.quorum_threshold,
            "vote collected"
        );

        // Check if quorum is reached.
        if vote_count >= self.config.quorum_threshold {
            info!(
                block = %vote.block_hash,
                height = vote.height,
                votes = vote_count,
                "quorum reached — block finalized"
            );
            // We'll return the finalized info. The caller must have the block.
            // For now return None; finalization happens in the run loop.
        }

        Ok(None)
    }

    /// Check if a block has reached quorum.
    pub fn has_quorum(&self, block_hash: &Hash) -> bool {
        let pending = self.pending_votes.read();
        pending
            .get(block_hash)
            .map(|v| v.len() >= self.config.quorum_threshold)
            .unwrap_or(false)
    }

    /// Get collected votes for a block.
    pub fn get_votes(&self, block_hash: &Hash) -> Vec<Vote> {
        self.pending_votes
            .read()
            .get(block_hash)
            .cloned()
            .unwrap_or_default()
    }

    /// Finalize a block: apply it to the authoritative state and clean up.
    pub fn finalize_block(&self, block: &Block) -> BaudResult<()> {
        let mut state = self.state.write();
        state.apply_block(block)?;

        // Remove included transactions from mempool.
        let tx_hashes: Vec<Hash> = block.transactions.iter().map(|tx| tx.hash()).collect();
        self.mempool.remove_batch(&tx_hashes);

        // Clear pending votes for this block.
        let block_hash = block.header.hash();
        self.pending_votes.write().remove(&block_hash);

        info!(
            height = block.header.height,
            txs = block.transactions.len(),
            "block finalized and applied"
        );

        Ok(())
    }

    /// Run the consensus loop. This is the main entry point for the engine.
    pub async fn run(self: Arc<Self>, mut shutdown: broadcast::Receiver<()>) {
        let mut rx = self.consensus_rx.write().take()
            .expect("consensus_rx already taken — engine can only be run once");

        let block_interval = Duration::from_millis(self.config.block_interval_ms);
        let mut tick = tokio::time::interval(block_interval);

        // Track proposed blocks so we can finalize them.
        let mut proposed_blocks: HashMap<Hash, Block> = HashMap::new();

        info!(
            address = %self.keypair.address(),
            "consensus engine started"
        );

        loop {
            tokio::select! {
                // Periodic block proposal tick.
                _ = tick.tick() => {
                    if self.is_our_turn() {
                        match self.propose_block() {
                            Ok(block) => {
                                let block_hash = block.header.hash();
                                proposed_blocks.insert(block_hash, block.clone());

                                // Self-vote.
                                if let Ok(vote) = self.vote_on_proposal(&block) {
                                    let _ = self.process_vote(&vote);
                                }

                                // Broadcast proposal.
                                let msg = ConsensusMessage::Proposal(block);
                                let _ = self.consensus_tx.send(msg).await;
                            }
                            Err(e) => {
                                error!(err = %e, "failed to propose block");
                            }
                        }
                    }
                }

                // Process incoming consensus messages.
                Some(msg) = rx.recv() => {
                    match msg {
                        ConsensusMessage::Proposal(block) => {
                            let block_hash = block.header.hash();

                            // Vote on it.
                            match self.vote_on_proposal(&block) {
                                Ok(vote) => {
                                    proposed_blocks.insert(block_hash, block);
                                    let _ = self.process_vote(&vote);

                                    // Broadcast our vote.
                                    let msg = ConsensusMessage::VoteMsg(vote);
                                    let _ = self.consensus_tx.send(msg).await;
                                }
                                Err(e) => {
                                    warn!(err = %e, "failed to vote on proposal");
                                }
                            }
                        }
                        ConsensusMessage::VoteMsg(vote) => {
                            if let Err(e) = self.process_vote(&vote) {
                                warn!(err = %e, "invalid vote received");
                                continue;
                            }

                            // Check if quorum reached.
                            if self.has_quorum(&vote.block_hash) {
                                if let Some(block) = proposed_blocks.remove(&vote.block_hash) {
                                    let votes = self.get_votes(&vote.block_hash);
                                    match self.finalize_block(&block) {
                                        Ok(()) => {
                                            let finalized = FinalizedBlock {
                                                block,
                                                votes,
                                            };
                                            let _ = self.finalized_tx.send(finalized.clone());
                                            let _ = self.consensus_tx.send(
                                                ConsensusMessage::Finalized(finalized)
                                            ).await;
                                        }
                                        Err(e) => {
                                            error!(err = %e, "failed to finalize block");
                                        }
                                    }
                                }
                            }
                        }
                        ConsensusMessage::Finalized(finalized) => {
                            // If we haven't finalized this block yet, do so.
                            let block_hash = finalized.block.header.hash();
                            proposed_blocks.remove(&block_hash);

                            let current_height = self.state.read().height;
                            if finalized.block.header.height > current_height {
                                if let Err(e) = self.finalize_block(&finalized.block) {
                                    warn!(err = %e, "failed to apply finalized block from peer");
                                }
                            }

                            // Periodic cleanup: remove stale proposed blocks to prevent memory exhaustion.
                            if proposed_blocks.len() > 100 {
                                let cutoff = current_height.saturating_sub(10);
                                proposed_blocks.retain(|_, block| block.header.height > cutoff);
                            }
                        }
                    }
                }

                // Shutdown signal.
                _ = shutdown.recv() => {
                    info!("consensus engine shutting down");
                    break;
                }
            }
        }
    }

    // ── Accessors ───────────────────────────────────────────────────────

    pub fn state(&self) -> Arc<RwLock<WorldState>> {
        Arc::clone(&self.state)
    }

    pub fn mempool(&self) -> Arc<Mempool> {
        Arc::clone(&self.mempool)
    }

    pub fn address(&self) -> Address {
        self.keypair.address()
    }

    pub fn validator_count(&self) -> usize {
        self.validators.read().len()
    }

    pub fn config(&self) -> &ConsensusConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baud_core::types::Account;

    fn setup_single_validator() -> (Arc<ConsensusEngine>, broadcast::Receiver<FinalizedBlock>) {
        let kp = Arc::new(KeyPair::generate());
        let addr = kp.address();

        let mut state = WorldState::new("test-chain".into());
        state.accounts.insert(addr, Account::with_balance(addr, 1_000_000));

        let state = Arc::new(RwLock::new(state));
        let mempool = Arc::new(Mempool::new());
        let config = ConsensusConfig::default();

        let (engine, rx, _tx) = ConsensusEngine::new(
            kp,
            vec![addr],
            state,
            mempool,
            config,
        );

        (Arc::new(engine), rx)
    }

    #[test]
    fn single_validator_proposes_empty_block() {
        let (engine, _rx) = setup_single_validator();

        assert!(engine.is_our_turn());
        let block = engine.propose_block().unwrap();
        assert_eq!(block.header.height, 1);
        assert_eq!(block.transactions.len(), 0);
    }

    #[test]
    fn vote_and_finalize() {
        let (engine, _rx) = setup_single_validator();

        let block = engine.propose_block().unwrap();
        let vote = engine.vote_on_proposal(&block).unwrap();
        assert!(vote.accept);

        engine.process_vote(&vote).unwrap();
        assert!(engine.has_quorum(&block.header.hash()));

        engine.finalize_block(&block).unwrap();
        assert_eq!(engine.state().read().height, 1);
    }

    #[test]
    fn quorum_threshold_computation() {
        assert_eq!(ConsensusEngine::compute_quorum(1), 1);
        assert_eq!(ConsensusEngine::compute_quorum(3), 3);
        assert_eq!(ConsensusEngine::compute_quorum(4), 3);
        assert_eq!(ConsensusEngine::compute_quorum(7), 5);
        assert_eq!(ConsensusEngine::compute_quorum(10), 7);
        assert_eq!(ConsensusEngine::compute_quorum(100), 67);
    }

    #[test]
    fn leader_rotation() {
        let kp1 = Arc::new(KeyPair::generate());
        let kp2 = KeyPair::generate();
        let kp3 = KeyPair::generate();

        let validators = vec![kp1.address(), kp2.address(), kp3.address()];

        let state = Arc::new(RwLock::new(WorldState::new("test".into())));
        let mempool = Arc::new(Mempool::new());

        let (engine, _, _) = ConsensusEngine::new(
            kp1.clone(),
            validators.clone(),
            state,
            mempool,
            ConsensusConfig::default(),
        );

        // Heights 1,2,3 should cycle through validators.
        assert_eq!(engine.leader_for_height(1), validators[1]);
        assert_eq!(engine.leader_for_height(2), validators[2]);
        assert_eq!(engine.leader_for_height(3), validators[0]);
        assert_eq!(engine.leader_for_height(4), validators[1]);
    }
}
