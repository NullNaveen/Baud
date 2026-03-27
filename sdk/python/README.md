# baud-sdk

Python SDK for the [Baud](https://github.com/NullNaveen/Baud) M2M Agent Ledger.

## Installation

```bash
pip install baud-sdk
```

This installs everything needed — **no external binaries required**.

## Quick Start

```python
from baud_sdk import BaudClient, NativeKeyPair, QUANTA_PER_BAUD

# Generate a new agent identity (pure Python, no CLI needed)
kp = NativeKeyPair.generate()
print(f"Address: {kp.address_hex}")
print(f"Secret:  {kp.secret_hex}")

# Connect to a node with pure Python signing
client = BaudClient.from_secret(kp.secret_hex, node_url="http://localhost:8080")

# Send 1 BAUD
result = client.native_send(to="<recipient-address>", amount=QUANTA_PER_BAUD)
print(f"TX hash: {result['tx_hash']}")

# Check balance
balance = client.balance(kp.address_hex)
print(f"Balance: {balance} quanta = {balance / QUANTA_PER_BAUD} BAUD")

# Create escrow
client.native_create_escrow(
    recipient="<worker-address>",
    amount=500 * QUANTA_PER_BAUD,
    preimage="proof_of_delivery",
    deadline=1700000000000,
)

# Register agent identity
client.native_register_agent(
    name="my-agent",
    endpoint="https://api.myagent.ai",
    capabilities=["llm", "inference"],
)
```

## For AI Agents

An AI agent only needs to run `pip install baud-sdk` — no Rust, no compiling, no CLI binary.

```python
from baud_sdk import BaudClient

# Agent connects with its secret key
client = BaudClient.from_secret("your_secret_key_hex", node_url="http://node-ip:8080")

# Pay for a service
client.native_send(to="service_provider_address", amount=10**18, memo="image-gen-job-42")

# Check balance
client.balance(client.address)
```

## API Reference

### `NativeKeyPair` (recommended — pure Python)

- `NativeKeyPair.generate()` — Generate a new random keypair
- `NativeKeyPair.from_secret_hex(hex)` — Restore from a hex secret key
- `.address_hex` — Hex-encoded agent address
- `.secret_hex` — Hex-encoded secret key
- `.sign(message)` — Sign raw bytes

### `BaudClient`

**Create:**
- `BaudClient.from_secret(secret_hex, node_url, chain_id)` — Pure Python client (recommended)
- `BaudClient(node_url, keypair=kp)` — CLI-based client (legacy)

**Query methods:**
- `client.status()` — Node status
- `client.account(address)` — Account details
- `client.balance(address)` — Balance in quanta
- `client.nonce(address)` — Current nonce
- `client.get_escrow(id)` — Escrow details
- `client.get_tx(hash)` — Transaction lookup
- `client.mempool()` — Pending transactions

**Native signing methods (pure Python, recommended):**
- `client.native_send(to, amount, memo=None, nonce=None)`
- `client.native_create_escrow(recipient, amount, preimage, deadline, nonce=None)`
- `client.native_release_escrow(escrow_id, preimage, nonce=None)`
- `client.native_refund_escrow(escrow_id, nonce=None)`
- `client.native_register_agent(name, endpoint, capabilities, nonce=None)`
- `client.native_batch_transfer(transfers, nonce=None)` — Atomic multi-recipient transfer
- `client.native_create_sub_account(label, budget, expiry=0, nonce=None)` — Delegated budget
- `client.native_delegated_transfer(sub_account_id, to, amount, nonce=None)` — Spend from sub-account

**CLI-based methods (requires baud binary, legacy):**
- `client.send(to, amount, memo=None, nonce=None)`
- `client.create_escrow(recipient, amount, preimage, deadline, nonce=None)`
- `client.release_escrow(escrow_id, preimage, nonce=None)`
- `client.refund_escrow(escrow_id, nonce=None)`
- `client.register_agent(name, endpoint, capabilities, nonce=None)`

### Constants

- `QUANTA_PER_BAUD = 10**18`

## Advanced Features

### Batch Transfers

Send to multiple recipients atomically in a single transaction (up to 32):

```python
client.native_batch_transfer([
    ("recipient_addr_1", 100 * QUANTA_PER_BAUD),
    ("recipient_addr_2", 50 * QUANTA_PER_BAUD),
    ("recipient_addr_3", 25 * QUANTA_PER_BAUD),
])
```

### Sub-accounts (Delegated Budgets)

Create a sub-account with a spending budget, then spend from it:

```python
# Create sub-account with 1000 BAUD budget
result = client.native_create_sub_account(
    label="marketing",
    budget=1000 * QUANTA_PER_BAUD,
)

# Spend from the sub-account
client.native_delegated_transfer(
    sub_account_id=result["tx_hash"],
    to="vendor_address",
    amount=100 * QUANTA_PER_BAUD,
)
```

### Full Transaction Type List

The Baud protocol supports 24 transaction types:

| Type | Description |
|------|-------------|
| `Transfer` | Simple value transfer |
| `EscrowCreate` | Hash-time-locked escrow |
| `EscrowRelease` | Release escrow with preimage |
| `EscrowRefund` | Refund escrow after deadline |
| `AgentRegister` | Register agent identity |
| `MilestoneEscrowCreate` | Multi-stage escrow |
| `MilestoneRelease` | Release a single milestone |
| `SetSpendingPolicy` | Account spending rules |
| `CoSignedTransfer` | Multi-sig transfer |
| `UpdateAgentPricing` | Set service pricing |
| `RateAgent` | Rate another agent (1-5) |
| `CreateRecurringPayment` | Scheduled payments |
| `CancelRecurringPayment` | Cancel scheduled payment |
| `CreateServiceAgreement` | Propose a service deal |
| `AcceptServiceAgreement` | Accept a proposed deal |
| `CompleteServiceAgreement` | Mark deal as done |
| `DisputeServiceAgreement` | Dispute a deal |
| `CreateProposal` | Governance proposal |
| `CastVote` | Vote on a proposal |
| `CreateSubAccount` | Delegated budget account |
| `DelegatedTransfer` | Spend from sub-account |
| `SetArbitrator` | Assign dispute arbitrator |
| `ArbitrateDispute` | Resolve a dispute |
| `BatchTransfer` | Atomic multi-recipient transfer |
