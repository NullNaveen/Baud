# Baud: A Feeless Machine-to-Machine Ledger for Autonomous AI Agent Economies

**Version 1.0 — June 2025**

---

## Abstract

Baud is a purpose-built distributed ledger designed exclusively for machine-to-machine (M2M) value transfer between autonomous AI agents. Unlike general-purpose blockchains, Baud eliminates transaction fees, provides native hash-time-locked escrow for trustless agent-to-agent service settlement, and offers an agent identity registry — all accessible through a headless, API-first interface with no browser UI. Baud uses Ed25519 signatures, BLAKE3 hashing, BFT consensus with round-robin leader rotation, and u128 checked arithmetic to deliver a secure, deterministic settlement layer for the emerging AI agent economy.

---

## 1. Introduction

### 1.1 The Problem

The AI agent economy is growing rapidly. Autonomous agents perform inference, data retrieval, code generation, orchestration, and physical-world tasks on behalf of users and other agents. These agents increasingly need to **pay each other** for services in real time, without human intervention.

Existing payment infrastructure fails this use case:

- **Traditional payment rails** (credit cards, bank transfers) require human identity, KYC processes, and are designed for human-speed interactions with multi-day settlement.
- **General-purpose blockchains** (Bitcoin, Ethereum) impose per-transaction gas fees that make micro-transactions uneconomical. A $0.001 inference call cannot bear a $2.50 gas fee.
- **Layer-2 solutions** add complexity (channels, rollups, bridges) that headless agents must navigate with no UI or human guidance.
- **Centralized payment APIs** (Stripe, PayPal) create single points of failure and require corporate accounts that autonomous agents cannot hold.

### 1.2 The Solution

Baud is a ledger built from scratch for this problem:

- **Zero fees**: Every transaction is free. Agents can settle 1-quantum ($10^{-18}$ BAUD) payments without overhead.
- **Native escrow**: Hash-time-locked contracts are a first-class transaction type, not a smart contract layered on top. Agents lock funds, prove delivery with a preimage, and settle atomically.
- **Agent identity**: On-chain registration of name, endpoint URL, and capability tags makes agents discoverable by other agents.
- **API-first**: No browser wallet, no MetaMask pop-up, no human-readable UI. Every interaction is a JSON REST call or CLI command.
- **BFT consensus**: Deterministic finality with round-robin leader rotation and >⅔ quorum, suitable for a known-validator-set deployment model.

---

## 2. Architecture

### 2.1 System Overview

```
┌──────────────────────────────────────────────────────────┐
│                        baud-node                         │
│  ┌────────────┐  ┌──────────────┐  ┌──────────────────┐ │
│  │  baud-api   │  │baud-consensus│  │  baud-network    │ │
│  │  (REST)     │  │  (BFT)       │  │  (P2P WebSocket) │ │
│  └─────┬──────┘  └──────┬───────┘  └────────┬─────────┘ │
│        │                │                    │           │
│        └────────────────┼────────────────────┘           │
│                         │                                │
│                  ┌──────┴───────┐                        │
│                  │  baud-core   │                        │
│                  │  (state,     │                        │
│                  │   crypto,    │                        │
│                  │   mempool,   │                        │
│                  │   types)     │                        │
│                  └──────────────┘                        │
└──────────────────────────────────────────────────────────┘
```

The system is composed of six Rust crates organized as a Cargo workspace:

| Crate | Responsibility |
|-------|---------------|
| **baud-core** | Cryptographic primitives, transaction types, world state, mempool, validation |
| **baud-consensus** | BFT consensus engine with round-robin leader selection |
| **baud-network** | WebSocket-based P2P gossip protocol with message deduplication |
| **baud-api** | Axum REST API server with CORS and body-size limits |
| **baud-cli** | Offline transaction signing and key management |
| **baud-node** | Full node binary composing all subsystems |

### 2.2 Cryptographic Primitives

| Primitive | Algorithm | Library | Size |
|-----------|-----------|---------|------|
| Identity key | Ed25519 | `ed25519-dalek` v2 | 32 bytes (public), 32 bytes (secret) |
| Address | Ed25519 public key | — | 32 bytes |
| Transaction signature | Ed25519 | `ed25519-dalek` v2 | 64 bytes |
| Hash function | BLAKE3 | `blake3` v1 | 32 bytes |
| Serialization | Bincode | `bincode` v1 | Variable |

**Why Ed25519?** Deterministic signatures (no nonce reuse risk), 128-bit security level, fast verification (~70,000 verifications/sec on modern hardware), and compact 64-byte signatures.

**Why BLAKE3?** Fastest cryptographic hash available (~1 GiB/s single-threaded), 256-bit output, tree-hashing for parallelism, and no length-extension attacks.

### 2.3 Token Model

| Property | Value |
|----------|-------|
| Unit | BAUD |
| Smallest unit | 1 quantum |
| Quanta per BAUD | $10^{18}$ |
| Balance type | `u128` (max $3.4 \times 10^{38}$) |
| Balance arithmetic | Checked (overflow/underflow = rejection) |
| Transaction fees | 0 |
| Total supply | 1,000,000,000 BAUD (hard cap enforced at genesis) |
| Issuance model | Pre-minted at genesis; no mining, no block rewards |

The u128 balance type with $10^{18}$ subdivision provides 18 decimal places of precision — matching Ethereum's wei — while the checked arithmetic ensures no balance can overflow or underflow. Zero fees mean agents never need to "top up gas" or estimate fee markets.

---

## 3. Transaction Types

All transactions share a common envelope:

```
Transaction {
    sender:    Address,      // 32-byte Ed25519 public key
    nonce:     u64,          // Sequential per-sender (replay protection)
    timestamp: u64,          // Unix milliseconds
    chain_id:  String,       // Chain identifier (cross-chain replay protection)
    payload:   Payload,      // One of the types below
    signature: Signature,    // Ed25519 signature over (sender, nonce, timestamp, chain_id, payload)
}
```

### 3.1 Transfer

Moves `amount` quanta from sender to recipient.

| Field | Type | Constraints |
|-------|------|-------------|
| `to` | Address | Must differ from sender |
| `amount` | u128 | Must be > 0, ≤ sender balance |
| `memo` | Option\<Vec\<u8\>\> | Optional, max 256 bytes |

### 3.2 EscrowCreate

Locks funds in a hash-time-locked contract. Sender's balance is debited immediately; funds are held in escrow state.

| Field | Type | Constraints |
|-------|------|-------------|
| `recipient` | Address | Must differ from sender |
| `amount` | u128 | Must be > 0, ≤ sender balance |
| `hash_lock` | Hash | BLAKE3 hash of a secret preimage |
| `deadline` | u64 | Unix ms; must be in the future |

### 3.3 EscrowRelease

Recipient claims escrowed funds by revealing the preimage.

| Field | Type | Constraints |
|-------|------|-------------|
| `escrow_id` | Hash | Must reference an active escrow |
| `preimage` | Vec\<u8\> | BLAKE3(preimage) must equal hash_lock |

Only the designated recipient can release. The escrow must still be active and the deadline must not have passed.

### 3.4 EscrowRefund

Sender reclaims escrowed funds after the deadline expires.

| Field | Type | Constraints |
|-------|------|-------------|
| `escrow_id` | Hash | Must reference an active escrow |

Only the original sender can refund. The deadline must have passed.

### 3.5 AgentRegister

Registers or updates agent metadata on-chain, making the agent discoverable.

| Field | Type | Constraints |
|-------|------|-------------|
| `name` | Vec\<u8\> | Max 64 bytes |
| `endpoint` | Vec\<u8\> | Max 256 bytes |
| `capabilities` | Vec\<Vec\<u8\>\> | Max 16 tags, each max 64 bytes |

---

## 4. Consensus

### 4.1 BFT with Round-Robin Leader Rotation

Baud uses a Byzantine Fault Tolerant consensus protocol with deterministic round-robin leader selection:

1. **Block proposal**: The leader for round $r$ is `validators[r \mod n]`. The leader collects transactions from the mempool, constructs a block, signs it, and broadcasts a `Propose` message.

2. **Pre-vote**: Each validator verifies the proposed block (structural validity, transaction signatures, state transitions). If valid, the validator signs a `PreVote` and broadcasts it.

3. **Pre-commit**: Once a validator sees $>⅔$ PreVotes for the same block, it broadcasts a `PreCommit`.

4. **Commit**: Once $>⅔$ PreCommits are collected, the block is committed. All validators apply the block's transactions to their world state.

### 4.2 Safety Guarantees

- **Agreement**: No two honest validators commit different blocks at the same height (requires $>⅔$ quorum).
- **Liveness**: As long as $>⅔$ of validators are honest, new blocks are produced every `block_interval` milliseconds (default: 1000ms).
- **Fault tolerance**: The network tolerates up to $\lfloor(n-1)/3\rfloor$ Byzantine validators.

### 4.3 Block Structure

```
Block {
    height:     u64,
    prev_hash:  Hash,
    tx_root:    Hash,        // Merkle root of transaction hashes
    state_root: Hash,        // Sorted-account Merkle root
    timestamp:  u64,
    proposer:   Address,
    transactions: Vec<Transaction>,
    signature:  Signature,   // Proposer's signature
}
```

---

## 5. Networking

### 5.1 P2P Protocol

Nodes communicate over WebSocket connections using a gossip protocol:

- **Connection**: Nodes connect to bootstrap peers on startup and accept incoming connections.
- **Message types**: `NewTransaction`, `BlockProposal`, `PreVote`, `PreCommit`, `PeerDiscovery`.
- **Deduplication**: Every message is tagged with a unique hash. Nodes track seen hashes and drop duplicates, preventing gossip amplification.
- **Serialization**: All P2P messages are bincode-encoded for efficiency.

### 5.2 API Layer

The REST API runs on Axum with:

- **CORS**: Permissive headers for cross-origin agent access.
- **Body limit**: 128 KiB maximum request body to prevent abuse.
- **JSON**: All request/response bodies are JSON-encoded.

---

## 6. Agent Identity System

Baud's agent identity system transforms the ledger from a simple payment network into an **agent directory**:

1. An agent generates an Ed25519 keypair (`baud keygen`).
2. The agent submits an `AgentRegister` transaction with its name, API endpoint, and capability tags.
3. Any other agent can query `GET /account/{address}` to discover the agent's metadata.
4. Capability tags (e.g., `["llm", "inference", "vision"]`) enable programmatic service discovery.

This creates a decentralized, on-chain registry where agents can find and verify each other without a central directory.

---

## 7. Escrow: Trustless Agent-to-Agent Settlement

The escrow system enables trustless pay-for-service between agents that have never interacted before:

### 7.1 Protocol Flow

```
Requester                          Worker
    │                                │
    │  1. EscrowCreate               │
    │  (amount, hash_lock, deadline) │
    │──────────────────────────────>│
    │                                │
    │  2. Observe escrow on-chain    │
    │                                │
    │  3. Perform work               │
    │                                │
    │  4. EscrowRelease (preimage)   │
    │<──────────────────────────────│
    │                                │
    │  Funds transferred to worker   │
    │                                │
```

**Safety properties**:
- The requester cannot withdraw funds before the deadline (locked in escrow).
- The worker can only claim funds by revealing the preimage (proof of delivery).
- If the worker fails to deliver, the requester reclaims funds after the deadline.
- The hash-lock can encode any verifiable condition (file hash, API response hash, model output hash).

### 7.2 Use Cases

| Scenario | Hash-Lock Condition |
|----------|-------------------|
| Inference payment | Hash of the model's output |
| Data purchase | Hash of the dataset |
| Code generation | Hash of the delivered code artifact |
| Multi-step pipeline | Chain of escrows, each gated by the previous step's output |

---

## 8. Security Model

### 8.1 Cryptographic Security

- **Ed25519 signatures**: 128-bit security level. Every transaction must include a valid signature from the sender's key.
- **BLAKE3 hashing**: 256-bit collision resistance. Used for block hashes, Merkle roots, escrow hash-locks, and message deduplication.
- **Replay protection**: Monotonically increasing per-account nonces. Each transaction's nonce must exactly equal the account's current nonce.
- **Cross-chain replay protection**: The `chain_id` is embedded in the signed hash of every transaction. A transaction signed for one Baud chain is cryptographically invalid on any other Baud chain with a different chain ID.

### 8.2 State Machine Safety

- **Checked arithmetic**: All balance operations use Rust's checked_add/checked_sub. Overflow or underflow causes transaction rejection, not silent wraparound.
- **Self-transfer rejection**: Sending to yourself is rejected to prevent nonce-manipulation attacks.
- **Zero-amount rejection**: Zero-value transfers are rejected.
- **Size limits**: Transactions limited to 64 KiB. Memos limited to 256 bytes. Agent names limited to 64 bytes.

### 8.3 Network Security

- **Body-size limits**: REST API rejects requests larger than 128 KiB.
- **Message deduplication**: P2P gossip protocol tracks seen message hashes, preventing amplification attacks.
- **Deterministic state root**: Sorted-account Merkle root ensures all honest nodes converge on the same state.

---

## 9. Economics

### 9.1 Supply

Total supply is hard-capped at **1,000,000,000 BAUD** (1 billion = $10^{27}$ quanta). This cap is enforced in the genesis initialization code — any genesis configuration whose total allocations exceed this limit is rejected. There is no mining, no block rewards, and no inflation mechanism. All tokens exist from the first block.

### 9.2 Distribution

| Allocation | Percentage | Purpose |
|-----------|------------|---------|
| Founder | 10% | Continued development, infrastructure costs |
| Validator rewards pool | 20% | Long-term incentives for validators |
| Agent ecosystem grants | 30% | Grants for agent developers integrating Baud |
| Initial circulation | 40% | Distributed via testnet participation, early adopters |

### 9.3 Fee Model

**Baud has zero transaction fees.** This is a deliberate design choice:

- Micro-transactions (fractions of a cent) are the primary use case. Fees would make most transactions uneconomical.
- Spam prevention is achieved through rate limiting at the API layer rather than economic fee markets.
- Validators are incentivized through block rewards from the validator rewards pool, not transaction fees.

### 9.4 Value Proposition

BAUD derives value from being the **settlement medium** in AI agent economies:

1. **Network effects**: As more agents adopt Baud, the utility of holding BAUD increases.
2. **Escrow demand**: Every active escrow locks BAUD, reducing circulating supply.
3. **Agent registration**: Registering agent identity requires a funded account, creating baseline demand.
4. **Interoperability**: A common settlement token eliminates the need for agent-to-agent currency negotiation.

---

## 10. Comparison with Alternatives

| Feature | Baud | Ethereum | Bitcoin | Solana | Centralized API |
|---------|------|----------|---------|--------|----------------|
| Transaction fee | 0 | $0.50–$50 | $1–$30 | $0.001 | 2.9% + $0.30 |
| Finality | ~1s (BFT) | ~12min | ~60min | ~0.4s | Instant |
| Native escrow | Yes | Smart contract | Script | Smart contract | Vendor-specific |
| Agent identity | Built-in | ENS (separate) | No | No | OAuth/API keys |
| API-first | Yes | Partial | No | Partial | Yes |
| Human UI required | No | MetaMask etc. | Wallet app | Phantom etc. | Dashboard |
| Micro-tx viable | Yes | No (gas) | No (fees) | Mostly | No (minimums) |

---

## 11. Implementation

Baud is implemented in Rust for performance, safety, and correctness:

- **Memory safety**: No garbage collector pauses, no null pointer dereferences, no data races.
- **Performance**: Native compilation, zero-cost abstractions, async I/O via Tokio.
- **Correctness**: Rust's type system and borrow checker prevent entire classes of bugs at compile time.
- **Test coverage**: 36 tests covering crypto operations, state transitions, escrow lifecycle, wallet encryption, mempool behavior, consensus protocol, overflow protection, cross-chain replay resistance, and nonce replay resistance.

### 11.1 Dependencies

| Library | Version | Purpose |
|---------|---------|---------|
| `ed25519-dalek` | 2 | Ed25519 signing and verification |
| `blake3` | 1 | BLAKE3 hashing |
| `axum` | 0.7 | REST API framework |
| `tokio` | 1 | Async runtime |
| `tokio-tungstenite` | 0.21 | WebSocket P2P networking |
| `serde` | 1 | Serialization framework |
| `clap` | 4 | CLI argument parsing |

### 11.2 Lines of Code

~4,000 lines of Rust across 6 crates, with no unsafe code.

---

## 12. Roadmap

### Phase 1: Foundation (Complete)
- [x] Core cryptographic primitives (Ed25519, BLAKE3)
- [x] Transaction types (Transfer, Escrow, AgentRegister)
- [x] World state machine with checked arithmetic
- [x] BFT consensus engine
- [x] P2P WebSocket networking
- [x] REST API server
- [x] CLI for key management and offline signing
- [x] Full node binary
- [x] 31 tests passing, zero warnings

### Phase 2: Production Hardening (In Progress)
- [ ] Persistent storage (RocksDB) — survive node restarts
- [ ] Wallet encryption (AES-256-GCM) — protect keys at rest
- [ ] Rate limiting — DDoS protection at the API layer
- [ ] Block explorer — web UI for inspecting chain state
- [ ] Stress testing — benchmarks under load

### Phase 3: Ecosystem
- [ ] Python SDK (`baud-sdk`) — pip-installable agent integration
- [ ] MCP server — Model Context Protocol tool for AI assistants
- [ ] LangChain / CrewAI agent templates
- [ ] Payment wrapper library for common agent frameworks
- [ ] Landing page and documentation site

### Phase 4: Network Launch
- [ ] Testnet deployment with multiple validators
- [ ] Genesis ceremony with initial allocation
- [ ] Moltbook and AI agent community announcements
- [ ] Mainnet launch

---

## 13. Conclusion

Baud fills a specific gap in the AI agent infrastructure stack: a feeless, escrow-native, identity-aware settlement layer designed from the ground up for machine-to-machine economies. By eliminating fees, providing trustless escrow as a primitive, and making agent identity a first-class concept, Baud removes the friction that prevents autonomous agents from transacting freely.

The ledger is open-source (MIT license), implemented in safe Rust, and available at https://github.com/NullNaveen/Baud.

---

## References

1. Bernstein, D.J. et al. "Ed25519: High-speed high-security signatures." 2012.
2. O'Connor, J. et al. "BLAKE3: One function, fast everywhere." 2020.
3. Castro, M. & Liskov, B. "Practical Byzantine Fault Tolerance." OSDI 1999.
4. Buchman, E. et al. "Tendermint: Byzantine Fault Tolerance in the Age of Blockchains." 2016.
