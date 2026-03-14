use baud_core::crypto::{Hash, KeyPair, Signature};
use baud_core::mempool::Mempool;
use baud_core::state::WorldState;
use baud_core::types::*;

/// Full end-to-end test: genesis → transfers → escrow lifecycle → block production.
#[test]
fn full_lifecycle() {
    // ── Setup: 3 agents ─────────────────────────────────────────────────
    let alice = KeyPair::generate();
    let bob = KeyPair::generate();
    let charlie = KeyPair::generate();

    let genesis = GenesisConfig {
        chain_id: "baud-test".into(),
        allocations: vec![
            GenesisAllocation {
                address: alice.address(),
                balance: 10_000 * QUANTA_PER_BAUD,
            },
            GenesisAllocation {
                address: bob.address(),
                balance: 5_000 * QUANTA_PER_BAUD,
            },
            GenesisAllocation {
                address: charlie.address(),
                balance: 1_000 * QUANTA_PER_BAUD,
            },
        ],
        validators: vec![
            ValidatorInfo {
                address: alice.address(),
                name: "alice".into(),
            },
        ],
        timestamp: 1_000_000,
    };

    let mut state = WorldState::from_genesis(&genesis).unwrap();
    let _mempool = Mempool::new();

    // Verify genesis balances.
    assert_eq!(
        state.balance_of(&alice.address()),
        10_000 * QUANTA_PER_BAUD
    );
    assert_eq!(
        state.balance_of(&bob.address()),
        5_000 * QUANTA_PER_BAUD
    );

    // ── Transfer: Alice → Bob ───────────────────────────────────────────
    let tx1 = sign_transfer(&alice, bob.address(), 100 * QUANTA_PER_BAUD, 0);
    state
        .validate_transaction(&tx1, 1_000_000)
        .expect("transfer should validate");
    state.apply_transaction(&tx1).expect("transfer should apply");

    assert_eq!(
        state.balance_of(&alice.address()),
        9_900 * QUANTA_PER_BAUD
    );
    assert_eq!(
        state.balance_of(&bob.address()),
        5_100 * QUANTA_PER_BAUD
    );

    // ── Micro-transaction: Bob → Charlie (0.001 BAUD) ───────────────────
    let micro_amount = QUANTA_PER_BAUD / 1000; // 0.001 BAUD
    let tx2 = sign_transfer(&bob, charlie.address(), micro_amount, 0);
    state.validate_transaction(&tx2, 1_000_000).unwrap();
    state.apply_transaction(&tx2).unwrap();

    assert_eq!(
        state.balance_of(&charlie.address()),
        1_000 * QUANTA_PER_BAUD + micro_amount
    );

    // ── Escrow: Alice → Charlie (hash-time-locked) ──────────────────────
    let secret = b"agent_delivery_proof_2024";
    let hash_lock = Hash::digest(secret);
    let deadline = 5_000_000u64;

    let escrow_tx = sign_escrow_create(
        &alice,
        charlie.address(),
        500 * QUANTA_PER_BAUD,
        hash_lock,
        deadline,
        1, // nonce
    );

    state.validate_transaction(&escrow_tx, 1_500_000).unwrap();
    state.apply_transaction(&escrow_tx).unwrap();

    // Alice's balance should be reduced by escrow amount.
    assert_eq!(
        state.balance_of(&alice.address()),
        9_400 * QUANTA_PER_BAUD
    );

    let escrow_id = escrow_tx.hash();
    assert!(state.escrows.contains_key(&escrow_id));
    assert_eq!(
        state.escrows[&escrow_id].status,
        EscrowStatus::Active
    );

    // ── Escrow release: Charlie reveals preimage ────────────────────────
    let release_tx = sign_escrow_release(&charlie, escrow_id, secret, 0);
    state.validate_transaction(&release_tx, 2_000_000).unwrap();
    state.apply_transaction(&release_tx).unwrap();

    assert_eq!(
        state.balance_of(&charlie.address()),
        1_500 * QUANTA_PER_BAUD + micro_amount
    );
    assert_eq!(
        state.escrows[&escrow_id].status,
        EscrowStatus::Released
    );

    // ── Agent registration ──────────────────────────────────────────────
    let register_tx = sign_agent_register(
        &bob,
        "llm-inference-v2",
        "https://api.bob-agent.ai/v2",
        &["llm", "inference", "vision"],
        1, // nonce
    );
    state.validate_transaction(&register_tx, 2_000_000).unwrap();
    state.apply_transaction(&register_tx).unwrap();

    let bob_account = state.get_account(&bob.address());
    assert!(bob_account.agent_meta.is_some());
    let meta = bob_account.agent_meta.unwrap();
    assert_eq!(meta.name, b"llm-inference-v2");
    assert_eq!(meta.capabilities.len(), 3);

    // ── Verify state root is deterministic ──────────────────────────────
    let root1 = state.state_root();
    let root2 = state.state_root();
    assert_eq!(root1, root2);
}

/// Test that replay protection (nonce) works correctly.
#[test]
fn replay_protection() {
    let alice = KeyPair::generate();
    let bob = KeyPair::generate();

    let mut state = WorldState::new("baud-test".into());
    state.accounts.insert(
        alice.address(),
        Account::with_balance(alice.address(), 10_000),
    );

    let tx = sign_transfer(&alice, bob.address(), 100, 0);
    state.validate_transaction(&tx, 1_000_000).unwrap();
    state.apply_transaction(&tx).unwrap();

    // Replaying the same tx (same nonce) should fail.
    let result = state.validate_transaction(&tx, 1_000_000);
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("nonce"));
}

/// Test that self-transfers are rejected.
#[test]
fn reject_self_transfer() {
    let alice = KeyPair::generate();
    let tx = sign_transfer(&alice, alice.address(), 100, 0);
    assert!(tx.validate_structure().is_err());
}

/// Test that zero-amount transfers are rejected.
#[test]
fn reject_zero_amount() {
    let alice = KeyPair::generate();
    let bob = KeyPair::generate();
    let tx = sign_transfer(&alice, bob.address(), 0, 0);
    assert!(tx.validate_structure().is_err());
}

/// Test escrow refund is blocked before deadline.
#[test]
fn escrow_refund_before_deadline_fails() {
    let alice = KeyPair::generate();
    let bob = KeyPair::generate();

    let mut state = WorldState::new("baud-test".into());
    state.accounts.insert(
        alice.address(),
        Account::with_balance(alice.address(), 10_000),
    );

    let hash_lock = Hash::digest(b"secret");
    let deadline = 5_000_000u64;
    let escrow_tx =
        sign_escrow_create(&alice, bob.address(), 5_000, hash_lock, deadline, 0);
    state.apply_transaction(&escrow_tx).unwrap();

    let escrow_id = escrow_tx.hash();

    // Try refund before deadline — should fail.
    let refund_tx = sign_escrow_refund(&alice, escrow_id, 1);
    let result = state.validate_transaction(&refund_tx, 2_000_000); // before deadline
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("deadline"));
}

/// Test wrong preimage fails escrow release.
#[test]
fn escrow_wrong_preimage_fails() {
    let alice = KeyPair::generate();
    let bob = KeyPair::generate();

    let mut state = WorldState::new("baud-test".into());
    state.accounts.insert(
        alice.address(),
        Account::with_balance(alice.address(), 10_000),
    );

    let real_secret = b"correct_secret";
    let hash_lock = Hash::digest(real_secret);
    let deadline = 5_000_000u64;
    let escrow_tx =
        sign_escrow_create(&alice, bob.address(), 5_000, hash_lock, deadline, 0);
    state.apply_transaction(&escrow_tx).unwrap();

    let escrow_id = escrow_tx.hash();

    // Try release with wrong preimage.
    let release_tx = sign_escrow_release(&bob, escrow_id, b"wrong_secret", 0);
    let result = state.validate_transaction(&release_tx, 2_000_000);
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("preimage"));
}

/// Test unauthorized escrow operations.
#[test]
fn escrow_unauthorized_release() {
    let alice = KeyPair::generate();
    let bob = KeyPair::generate();
    let mallory = KeyPair::generate();

    let mut state = WorldState::new("baud-test".into());
    state.accounts.insert(
        alice.address(),
        Account::with_balance(alice.address(), 10_000),
    );

    let secret = b"secret";
    let hash_lock = Hash::digest(secret);
    let deadline = 5_000_000u64;
    let escrow_tx =
        sign_escrow_create(&alice, bob.address(), 5_000, hash_lock, deadline, 0);
    state.apply_transaction(&escrow_tx).unwrap();

    let escrow_id = escrow_tx.hash();

    // Mallory (not the recipient) tries to release — should fail.
    let release_tx = sign_escrow_release(&mallory, escrow_id, secret, 0);
    let result = state.validate_transaction(&release_tx, 2_000_000);
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("only recipient"));
}

/// Test mempool dedup and ordering.
#[test]
fn mempool_integration() {
    let pool = Mempool::new();
    let agent = KeyPair::generate();

    // Add 50 transactions.
    let mut hashes = Vec::new();
    for i in 0..50u64 {
        let tx = sign_transfer(
            &agent,
            KeyPair::generate().address(),
            1,
            i,
        );
        let h = pool.add(tx).unwrap();
        hashes.push(h);
    }

    assert_eq!(pool.len(), 50);

    // Remove first 10 (simulating block inclusion).
    pool.remove_batch(&hashes[..10]);
    assert_eq!(pool.len(), 40);

    // Get pending — should be ordered.
    let pending = pool.get_pending(100);
    assert_eq!(pending.len(), 40);
}

/// Test checked arithmetic prevents overflow.
#[test]
fn balance_overflow_protection() {
    let alice = KeyPair::generate();
    let bob = KeyPair::generate();

    let mut state = WorldState::new("baud-test".into());
    state.accounts.insert(
        alice.address(),
        Account::with_balance(alice.address(), u128::MAX),
    );
    state.accounts.insert(
        bob.address(),
        Account::with_balance(bob.address(), u128::MAX),
    );

    // Transfer from alice → bob would overflow bob's balance.
    let tx = sign_transfer(&alice, bob.address(), 1, 0);
    state.validate_transaction(&tx, 1_000_000).unwrap();
    let result = state.apply_transaction(&tx);
    assert!(result.is_err());
}

// ─── Helper functions ───────────────────────────────────────────────────────

fn sign_transfer(
    kp: &KeyPair,
    to: baud_core::Address,
    amount: u128,
    nonce: u64,
) -> Transaction {
    let mut tx = Transaction {
        sender: kp.address(),
        nonce,
        payload: TxPayload::Transfer {
            to,
            amount,
            memo: None,
        },
        timestamp: 1_000_000,
        chain_id: "baud-test".into(),
        signature: Signature::zero(),
    };
    let h = tx.signable_hash();
    tx.signature = kp.sign(h.as_bytes());
    tx
}

fn sign_escrow_create(
    kp: &KeyPair,
    recipient: baud_core::Address,
    amount: u128,
    hash_lock: Hash,
    deadline: u64,
    nonce: u64,
) -> Transaction {
    let mut tx = Transaction {
        sender: kp.address(),
        nonce,
        payload: TxPayload::EscrowCreate {
            recipient,
            amount,
            hash_lock,
            deadline,
        },
        timestamp: 1_000_000,
        chain_id: "baud-test".into(),
        signature: Signature::zero(),
    };
    let h = tx.signable_hash();
    tx.signature = kp.sign(h.as_bytes());
    tx
}

fn sign_escrow_release(
    kp: &KeyPair,
    escrow_id: Hash,
    preimage: &[u8],
    nonce: u64,
) -> Transaction {
    let mut tx = Transaction {
        sender: kp.address(),
        nonce,
        payload: TxPayload::EscrowRelease {
            escrow_id,
            preimage: preimage.to_vec(),
        },
        timestamp: 1_000_000,
        chain_id: "baud-test".into(),
        signature: Signature::zero(),
    };
    let h = tx.signable_hash();
    tx.signature = kp.sign(h.as_bytes());
    tx
}

fn sign_escrow_refund(
    kp: &KeyPair,
    escrow_id: Hash,
    nonce: u64,
) -> Transaction {
    let mut tx = Transaction {
        sender: kp.address(),
        nonce,
        payload: TxPayload::EscrowRefund { escrow_id },
        timestamp: 1_000_000,
        chain_id: "baud-test".into(),
        signature: Signature::zero(),
    };
    let h = tx.signable_hash();
    tx.signature = kp.sign(h.as_bytes());
    tx
}

fn sign_agent_register(
    kp: &KeyPair,
    name: &str,
    endpoint: &str,
    capabilities: &[&str],
    nonce: u64,
) -> Transaction {
    let mut tx = Transaction {
        sender: kp.address(),
        nonce,
        payload: TxPayload::AgentRegister {
            name: name.as_bytes().to_vec(),
            endpoint: endpoint.as_bytes().to_vec(),
            capabilities: capabilities
                .iter()
                .map(|c| c.as_bytes().to_vec())
                .collect(),
        },
        timestamp: 1_000_000,
        chain_id: "baud-test".into(),
        signature: Signature::zero(),
    };
    let h = tx.signable_hash();
    tx.signature = kp.sign(h.as_bytes());
    tx
}
