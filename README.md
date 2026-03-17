# Baud

[![CI](https://github.com/NullNaveen/Baud/actions/workflows/ci.yml/badge.svg)](https://github.com/NullNaveen/Baud/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)

**Feeless cryptocurrency for AI agent economies.**

Baud is a feeless micro-transaction cryptocurrency purpose-built for AI-agent economies. It provides hash-time-locked escrow contracts, BFT consensus, encrypted wallet management, and a zero-UI API-first interface designed for headless, programmatic operation.

## Key Features

- **Feeless micro-transactions** — 1 BAUD = 10¹⁸ quanta; agents can settle sub-cent amounts with zero network fees
- **Hash-time-locked escrow** — Trustless payment channels using BLAKE3 hash-locks with configurable deadlines
- **BFT consensus** — Round-robin leader selection with >⅔ quorum Byzantine fault tolerance
- **Agent-native identity** — Ed25519 keypairs as first-class agent identifiers with optional metadata (name, endpoint, capabilities)
- **Cross-chain replay protection** — Chain ID embedded in every transaction signature
- **Encrypted wallet** — AES-256-GCM + Argon2id key derivation for secure key storage
- **Rate limiting** — Token-bucket middleware (60 burst, 20 req/s) on the API layer
- **API-first** — REST endpoint set designed for programmatic consumption; no browser UI required
- **Python SDK** — Full client library with high-level payment wrappers for agent integration
- **MCP server** — Model Context Protocol server for LLM tool-use
- **Block explorer** — Dark-themed web SPA for chain inspection
- **P2P networking** — WebSocket-based gossip protocol with message deduplication
- **Persistent storage** — sled embedded database for durable state

## Architecture

```
crates/
├── baud-core        # Crypto, types, state machine, mempool, error types
├── baud-consensus   # BFT consensus engine with round-robin leader rotation
├── baud-network     # P2P WebSocket networking with gossip protocol
├── baud-api         # Axum REST API server with rate limiting
├── baud-cli         # Headless CLI for key management and transaction signing
├── baud-wallet      # AES-256-GCM encrypted wallet with Argon2id key derivation
├── baud-storage     # Persistent storage layer (sled)
└── baud-node        # Full node binary wiring all subsystems together

sdk/python/          # Python SDK (baud_sdk) with BaudPay payment wrappers
mcp-server/          # Model Context Protocol server for LLM integration
docs/                # Whitepaper, block explorer, landing page
examples/            # LangChain, CrewAI, and autonomous agent templates
scripts/             # Testnet launcher (PowerShell)
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

40 tests across unit and integration suites covering crypto, state transitions, escrow lifecycle, mempool, consensus, wallet encryption, overflow protection, and replay/cross-chain attack resistance.

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
  --nonce 0 \
  --chain-id baud-mainnet
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
| `GET` | `/v1/status` | Node status (height, chain ID, validator count) |
| `GET` | `/v1/account/{address}` | Account balance, nonce, agent metadata |
| `POST` | `/v1/tx` | Submit a signed transaction |
| `GET` | `/v1/tx/{hash}` | Look up transaction by hash |
| `GET` | `/v1/escrow/{id}` | Look up escrow contract by ID |
| `GET` | `/v1/mempool` | List pending transactions |
| `POST` | `/v1/keygen` | Generate a new Ed25519 keypair |
| `POST` | `/v1/sign-and-submit` | Sign & submit a transaction in one step |
| `GET` | `/v1/mining` | Current mining status and configuration |
| `GET` | `/dashboard` | Web dashboard (browser UI) |

### Submit Transaction

```bash
curl -X POST http://localhost:8080/v1/tx \
  -H "Content-Type: application/json" \
  -d @signed_tx.json
```

### Query Account

```bash
curl http://localhost:8080/v1/account/<hex-address>
```

## Running a Node

```bash
# Generate genesis with 3 validators
baud genesis --validators <key1>,<key2>,<key3> --output genesis.json

# Start node
baud-node \
  --genesis genesis.json \
  --secret-key <validator-hex-secret> \
  --api-addr 0.0.0.0:8080 \
  --p2p-addr 0.0.0.0:9944 \
  --peers ws://127.0.0.1:9945
```

### Node Configuration

| Flag | Default | Description |
|------|---------|-------------|
| `--genesis` | required | Path to genesis.json |
| `--secret-key` | required | Hex-encoded validator secret key |
| `--api-addr` | `0.0.0.0:8080` | REST API listen address |
| `--p2p-addr` | `0.0.0.0:9944` | P2P WebSocket listen address |
| `--peers` | none | Comma-separated peer WebSocket URLs |
| `--data-dir` | `data/` | Persistent storage directory |
| `--block-interval` | `5000` | Block interval in milliseconds |

## Security Model

- **Ed25519 signatures** on every transaction; replay-protected by sequential nonces
- **Cross-chain replay protection** — chain ID included in signed transaction hash
- **Checked arithmetic** (`u128` overflow/underflow prevention) on all balance mutations
- **Structural validation** — size limits, self-transfer rejection, zero-amount rejection
- **Escrow authorization** — only recipient can release (with valid preimage), only sender can refund (after deadline)
- **Encrypted wallets** — AES-256-GCM encryption with Argon2id KDF (64 MiB, 3 iterations)
- **Rate limiting** — Token-bucket rate limiter on API (60 burst, 20/s sustained)
- **CORS + body limits** (128 KiB) on the API layer
- **Message deduplication** in P2P gossip to prevent amplification
- **Constant-time signature verification** via `ed25519-dalek`
- **Deterministic state root** — sorted account hashes for Merkle-proof readiness

## Python SDK

```bash
pip install -e sdk/python/
```

```python
from baud_sdk import BaudClient, BaudPay

# Low-level client
client = BaudClient("http://localhost:8080")
status = client.status()

# High-level payment wrapper for agents
pay = BaudPay.from_secret("hex-secret-key", node="http://localhost:8080")
receipt = pay.send(to="hex-address", amount_baud=1.0, memo="service payment")
receipt = pay.escrow(recipient="hex-address", amount_baud=5.0, preimage="secret")
print(pay.balance())
```

## Agent Templates

Ready-to-use examples in `examples/`:

- **`langchain_agent.py`** — LangChain tools for balance, send, and escrow
- **`crewai_team.py`** — CrewAI buyer/seller team with automated payment flow
- **`autonomous_agent.py`** — Minimal standalone agent with identity management

## Web Dashboard

Start the node and open `http://localhost:8080/dashboard` in a browser. The dashboard provides a full GUI for all operations — no terminal needed:

- **Dashboard** — Balance overview, network stats, recent transactions
- **Mining** — Live mining status, block height, hash rate
- **Wallet** — Generate new keypairs, view secret/public key
- **Send** — Transfer BAUD to any address (sign-and-submit in one step)
- **Accounts** — Look up any account's balance, nonce, and agent metadata
- **Mempool** — View pending transactions with auto-refresh
- **Node Info** — Chain ID, peer count, validator status
- **Explorer** — Look up transactions and escrows by hash/ID; release & refund escrows
- **Agents** — Register your AI agent's name, endpoint, and capabilities on-chain

### Windows Quick Start

Run `scripts\start-node.bat` or search "Baud" in the Start Menu (after running `scripts\install-shortcuts.bat`). The node starts and the dashboard opens automatically in your default browser.

## Block Explorer

Open `docs/explorer.html` in a browser. Connects to a local node and displays:
- Chain stats (height, accounts, escrows, mempool size)
- Account and escrow lookup by address/ID
- Live mempool view with auto-refresh

## Testnet

Launch a local multi-node testnet:

```powershell
# Start 3-node testnet
.\scripts\testnet.ps1 -Nodes 3

# Stop all nodes
.\scripts\testnet.ps1 -Stop
```

## Benchmarks

```bash
cargo bench -p baud-core
```

8 Criterion benchmarks: keygen, sign, validate, apply, 1000-tx batch, state root, mempool, BLAKE3.

## License

MIT
