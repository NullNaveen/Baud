# Baud MCP Server

Model Context Protocol (MCP) server for the Baud M2M Agent Ledger. Allows AI assistants to interact with the Baud blockchain as a tool.

## Setup

```bash
cd mcp-server
npm install
```

**Prerequisites:**
- A running Baud node (default: `http://localhost:8080`)
- The `baud` CLI on your PATH (`cargo install --path crates/baud-cli`)

## Configuration

Add to your MCP client config (e.g., Claude Desktop `claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "baud": {
      "command": "node",
      "args": ["path/to/Baud/mcp-server/index.js"],
      "env": {
        "BAUD_NODE_URL": "http://localhost:8080",
        "BAUD_BIN": "baud"
      }
    }
  }
}
```

## Available Tools

| Tool | Description |
|------|-------------|
| `baud_keygen` | Generate a new Ed25519 agent keypair |
| `baud_balance` | Query account balance |
| `baud_account` | Get full account details with agent metadata |
| `baud_transfer` | Sign and submit a transfer |
| `baud_escrow_create` | Create a hash-time-locked escrow |
| `baud_escrow_release` | Release escrow with preimage |
| `baud_escrow_refund` | Refund escrow after deadline |
| `baud_register_agent` | Register agent metadata on-chain |
| `baud_status` | Node status |
| `baud_escrow_info` | Look up escrow by ID |
| `baud_mempool` | List pending transactions |
