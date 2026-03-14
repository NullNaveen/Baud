# Baud

**Ultra-secure Machine-to-Machine ledger for autonomous AI agents.**

Baud is a feeless micro-transaction cryptocurrency purpose-built for AI-agent economies. It provides hash-time-locked escrow contracts, BFT consensus, and a zero-UI API-first interface designed for headless, programmatic operation.

## Key Features

- **Feeless micro-transactions** — 1 BAUD = 10¹⁸ quanta; agents can settle sub-cent amounts with zero network fees
- **Hash-time-locked escrow** — Trustless payment channels using BLAKE3 hash-locks with configurable deadlines
- **BFT consensus** — Round-robin leader selection with >⅔ quorum Byzantine fault tolerance
- **Agent-native identity** — Ed25519 keypairs as first-class agent identifiers with optional metadata (name, endpoint, capabilities)
- **API-first** — REST endpoint set designed for programmatic consumption; no browser UI required
- **P2P networking** — WebSocket-based gossip protocol with message deduplication

## Architecture

```
crates/
├── baud-core        # Crypto, types, state machine, mempool, error types
├── baud-consensus   # BFT consensus engine with round-robin leader rotation
├── baud-network     # P2P WebSocket networking with gossip protocol
├── baud-api         # Axum REST API server
├── baud-cli         # Headless CLI for key management and transaction signing
└── baud-node        # Full node binary wiring all subsystems together
```

### Cryptography

| Primitive | Algorithm | Library |
|-----------|-----------|---------|
| Identity / Signing | Ed25519 | `ed25519-dalek` v2 |
| Hashing | BLAKE3 | `blake3` v1 |
| Serialization | Bincode | `bincode` v1 |

### Token Model

| Property | Value |
|----------|-------|
| Unit | BAUD |
| Smallest unit | 1 quantum |
| Quanta per BAUD | 10¹⁸ |
| Balance type | `u128` (checked arithmetic) |
| Transaction fees | 0 |

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) stable toolchain (1.70+)

### Build

```bash
cargo build --release
```

### Run Tests

```bash
cargo test
```

31 tests across unit and integration suites covering crypto, state transitions, escrow lifecycle, mempool, consensus, overflow protection, and replay resistance.

## CLI Usage

The CLI (`baud`) creates signed transactions offline and submits them to nodes.

### Generate an Agent Identity

```bash
baud keygen
```

Outputs a hex-encoded secret key and the corresponding address.

### Create a Transfer

```bash
baud transfer \
  --secret <hex-secret-key> \
  --to <hex-recipient-address> \
  --amount 1000000000000000000 \
  --nonce 0
```

Prints a signed transaction as JSON to stdout.

### Create an Escrow

```bash
# Compute hash-lock from secret preimage
baud hash-data --data "my_delivery_proof"

# Create escrow
baud escrow-create \
  --secret <hex-secret-key> \
  --recipient <hex-address> \
  --amount 500000000000000000000 \
  --preimage "my_delivery_proof" \
  --deadline 1700000000000 \
  --nonce 1
```

### Release Escrow (Recipient)

```bash
baud escrow-release \
  --secret <recipient-secret-key> \
  --escrow-id <hex-escrow-id> \
  --preimage "my_delivery_proof" \
  --nonce 0
```

### Refund Escrow (Sender, After Deadline)

```bash
baud escrow-refund \
  --secret <sender-secret-key> \
  --escrow-id <hex-escrow-id> \
  --nonce 2
```

### Register Agent Metadata

```bash
baud agent-register \
  --secret <hex-secret-key> \
  --name "llm-inference-v2" \
  --endpoint "https://api.myagent.ai/v2" \
  --capabilities "llm,inference,vision" \
  --nonce 0
```

### Submit Transaction to Node

```bash
baud submit --node http://localhost:8080 --tx-file signed_tx.json
```

### Query Balance

```bash
baud balance --node http://localhost:8080 --address <hex-address>
```

### Generate Genesis

```bash
baud genesis \
  --chain-id baud-mainnet \
  --validators <hex-secret1>,<hex-secret2>,<hex-secret3> \
  --initial-balance 1000000 \
  --output genesis.json
```

## REST API

Default listen address: `http://0.0.0.0:8080`

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/status` | Node status (height, chain ID, validator count) |
| `GET` | `/account/{address}` | Account balance, nonce, agent metadata |
| `POST` | `/tx` | Submit a signed transaction |
| `GET` | `/tx/{hash}` | Look up transaction by hash |
| `GET` | `/escrow/{id}` | Look up escrow contract by ID |
| `GET` | `/mempool` | List pending transactions |

### Submit Transaction

```bash
curl -X POST http://localhost:8080/tx \
  -H "Content-Type: application/json" \
  -d @signed_tx.json
```

### Query Account

```bash
curl http://localhost:8080/account/<hex-address>
```

## Running a Node

```bash
# Generate genesis with 3 validators
baud genesis --validators <key1>,<key2>,<key3> --output genesis.json

# Start node
baud-node \
  --genesis genesis.json \
  --secret-key <validator-hex-secret> \
  --listen-addr 0.0.0.0:9000 \
  --api-port 8080
```

### Node Configuration

| Flag | Default | Description |
|------|---------|-------------|
| `--genesis` | required | Path to genesis.json |
| `--secret-key` | required | Hex-encoded validator secret key |
| `--listen-addr` | `0.0.0.0:9000` | P2P listen address |
| `--api-port` | `8080` | REST API port |
| `--bootstrap` | none | Comma-separated peer addresses to connect to |

## Security Model

- **Ed25519 signatures** on every transaction; replay-protected by sequential nonces
- **Checked arithmetic** (`u128` overflow/underflow prevention) on all balance mutations
- **Structural validation** — size limits, self-transfer rejection, zero-amount rejection
- **Escrow authorization** — only recipient can release (with valid preimage), only sender can refund (after deadline)
- **CORS + body limits** (128 KiB) on the API layer
- **Message deduplication** in P2P gossip to prevent amplification
- **Deterministic state root** — sorted account hashes for Merkle-proof readiness

## License

MIT
