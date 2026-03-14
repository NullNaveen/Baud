# baud-sdk

Python SDK for the [Baud](https://github.com/NullNaveen/Baud) M2M Agent Ledger.

## Installation

```bash
pip install baud-sdk
```

**Prerequisite:** The `baud` CLI binary must be installed and on your PATH:

```bash
# From the Baud repo root
cargo install --path crates/baud-cli
```

## Quick Start

```python
from baud_sdk import BaudClient, KeyPair, QUANTA_PER_BAUD

# Generate a new agent identity
kp = KeyPair.generate()
print(f"Address: {kp.address}")

# Connect to a node
client = BaudClient("http://localhost:8080", keypair=kp)

# Send 1 BAUD
result = client.send(to="<recipient-address>", amount=QUANTA_PER_BAUD)
print(f"TX hash: {result['tx_hash']}")

# Check balance
balance = client.balance(kp.address)
print(f"Balance: {balance} quanta = {balance / QUANTA_PER_BAUD} BAUD")

# Create escrow
client.create_escrow(
    recipient="<worker-address>",
    amount=500 * QUANTA_PER_BAUD,
    preimage="proof_of_delivery",
    deadline=1700000000000,
)

# Register agent identity
client.register_agent(
    name="my-agent",
    endpoint="https://api.myagent.ai",
    capabilities=["llm", "inference"],
)
```

## API Reference

### `KeyPair`

- `KeyPair.generate()` — Generate a new random keypair
- `KeyPair.from_secret(hex)` — Restore from a hex secret key
- `.address` — Hex-encoded agent address
- `.secret_key` — Hex-encoded secret key

### `BaudClient`

**Query methods:**
- `client.status()` — Node status
- `client.account(address)` — Account details
- `client.balance(address)` — Balance in quanta
- `client.nonce(address)` — Current nonce
- `client.get_escrow(id)` — Escrow details
- `client.get_tx(hash)` — Transaction lookup
- `client.mempool()` — Pending transactions

**Transaction methods (auto-sign + submit):**
- `client.send(to, amount, memo=None, nonce=None)`
- `client.create_escrow(recipient, amount, preimage, deadline, nonce=None)`
- `client.release_escrow(escrow_id, preimage, nonce=None)`
- `client.refund_escrow(escrow_id, nonce=None)`
- `client.register_agent(name, endpoint, capabilities, nonce=None)`

### Constants

- `QUANTA_PER_BAUD = 10**18`
