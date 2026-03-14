//! Stress-test benchmarks for Baud core operations.
//!
//! Run with: cargo bench -p baud-core

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

use baud_core::crypto::{Hash, KeyPair, Signature};
use baud_core::mempool::Mempool;
use baud_core::state::WorldState;
use baud_core::types::{
    Account, GenesisAllocation, GenesisConfig, Transaction, TxPayload, QUANTA_PER_BAUD,
};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn make_signed_transfer(
    kp: &KeyPair,
    to: &baud_core::crypto::Address,
    amount: u128,
    nonce: u64,
) -> Transaction {
    let mut tx = Transaction {
        sender: kp.address(),
        nonce,
        payload: TxPayload::Transfer {
            to: *to,
            amount,
            memo: None,
        },
        timestamp: now_ms(),
        chain_id: "bench".into(),
        signature: Signature::zero(),
    };
    let hash = tx.signable_hash();
    tx.signature = kp.sign(hash.as_bytes());
    tx
}

/// Set up a genesis state with `n` funded accounts and return (state, keypairs).
fn setup_state(n: usize) -> (WorldState, Vec<KeyPair>) {
    let keypairs: Vec<KeyPair> = (0..n).map(|_| KeyPair::generate()).collect();
    let allocs: Vec<GenesisAllocation> = keypairs
        .iter()
        .map(|kp| GenesisAllocation {
            address: kp.address(),
            balance: 1_000_000 * QUANTA_PER_BAUD,
        })
        .collect();
    let genesis = GenesisConfig {
        chain_id: "bench".into(),
        allocations: allocs,
        validators: vec![],
        timestamp: now_ms(),
    };
    let state = WorldState::from_genesis(&genesis).unwrap();
    (state, keypairs)
}

fn bench_keygen(c: &mut Criterion) {
    c.bench_function("keygen", |b| {
        b.iter(|| KeyPair::generate());
    });
}

fn bench_sign_transfer(c: &mut Criterion) {
    let kp = KeyPair::generate();
    let recipient = KeyPair::generate().address();
    c.bench_function("sign_transfer", |b| {
        let mut nonce = 0u64;
        b.iter(|| {
            nonce += 1;
            make_signed_transfer(&kp, &recipient, 1000, nonce)
        });
    });
}

fn bench_validate_transfer(c: &mut Criterion) {
    let (state, keypairs) = setup_state(2);
    let sender = &keypairs[0];
    let recipient = keypairs[1].address();
    let tx = make_signed_transfer(sender, &recipient, 1000, 1);
    let now = now_ms();

    c.bench_function("validate_transfer", |b| {
        b.iter(|| state.validate_transaction(&tx, now).unwrap());
    });
}

fn bench_apply_transfer(c: &mut Criterion) {
    let (state, keypairs) = setup_state(2);
    let sender = &keypairs[0];
    let recipient = keypairs[1].address();

    c.bench_function("apply_transfer", |b| {
        b.iter_batched(
            || {
                let mut s = state.clone();
                let nonce = s.get_account(&sender.address()).nonce + 1;
                let tx = make_signed_transfer(sender, &recipient, 1000, nonce);
                (s, tx)
            },
            |(mut s, tx)| {
                s.apply_transaction(&tx).unwrap();
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_apply_1000_transfers(c: &mut Criterion) {
    let (state, keypairs) = setup_state(100);

    c.bench_function("apply_1000_transfers", |b| {
        b.iter_batched(
            || {
                let mut s = state.clone();
                let now = now_ms();
                let mut txs = Vec::with_capacity(1000);
                for i in 0..1000 {
                    let sender = &keypairs[i % keypairs.len()];
                    let recipient = keypairs[(i + 1) % keypairs.len()].address();
                    let nonce = (i / keypairs.len()) as u64 + 1;
                    txs.push(make_signed_transfer(sender, &recipient, 100, nonce));
                }
                (s, txs)
            },
            |(mut s, txs)| {
                for tx in &txs {
                    let _ = s.apply_transaction(tx);
                }
            },
            BatchSize::LargeInput,
        );
    });
}

fn bench_state_root(c: &mut Criterion) {
    let (state, _) = setup_state(1000);

    c.bench_function("state_root_1000_accounts", |b| {
        b.iter(|| state.state_root());
    });
}

fn bench_mempool_add(c: &mut Criterion) {
    let sender = KeyPair::generate();
    let recipient = KeyPair::generate().address();

    c.bench_function("mempool_add", |b| {
        let mempool = Mempool::new();
        let mut nonce = 0u64;
        b.iter(|| {
            nonce += 1;
            let tx = make_signed_transfer(&sender, &recipient, 100, nonce);
            let _ = mempool.add(tx);
        });
    });
}

fn bench_blake3_hash(c: &mut Criterion) {
    let data = vec![0u8; 1024];
    c.bench_function("blake3_1kb", |b| {
        b.iter(|| Hash::digest(&data));
    });
}

criterion_group!(
    benches,
    bench_keygen,
    bench_sign_transfer,
    bench_validate_transfer,
    bench_apply_transfer,
    bench_apply_1000_transfers,
    bench_state_root,
    bench_mempool_add,
    bench_blake3_hash,
);
criterion_main!(benches);
