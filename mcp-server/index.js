#!/usr/bin/env node

/**
 * Baud MCP Server
 *
 * Model Context Protocol server that exposes Baud ledger operations
 * as tools for AI assistants. Connects to a running Baud node and
 * provides key generation, transfers, escrow, and query capabilities.
 */

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

const NODE_URL = process.env.BAUD_NODE_URL || "http://localhost:8080";
const BAUD_BIN = process.env.BAUD_BIN || "baud";

// ── Helpers ─────────────────────────────────────────────────────────────────

async function fetchJSON(path) {
  const resp = await fetch(`${NODE_URL}${path}`);
  if (!resp.ok) {
    const body = await resp.text();
    throw new Error(`HTTP ${resp.status}: ${body}`);
  }
  return resp.json();
}

async function postJSON(path, data) {
  const resp = await fetch(`${NODE_URL}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  if (!resp.ok) {
    const body = await resp.text();
    throw new Error(`HTTP ${resp.status}: ${body}`);
  }
  return resp.json();
}

async function runBaud(args) {
  const { stdout, stderr } = await execFileAsync(BAUD_BIN, args, {
    timeout: 30000,
  });
  if (stderr && stderr.trim()) {
    throw new Error(stderr.trim());
  }
  return JSON.parse(stdout);
}

// ── Tool Definitions ────────────────────────────────────────────────────────

const TOOLS = [
  {
    name: "baud_keygen",
    description: "Generate a new Ed25519 keypair for a Baud agent identity. Returns address and secret key.",
    inputSchema: { type: "object", properties: {}, required: [] },
  },
  {
    name: "baud_balance",
    description: "Query the BAUD balance of an address (in quanta, where 1 BAUD = 10^18 quanta).",
    inputSchema: {
      type: "object",
      properties: {
        address: { type: "string", description: "Hex-encoded 32-byte address" },
      },
      required: ["address"],
    },
  },
  {
    name: "baud_account",
    description: "Get full account details: balance, nonce, and agent metadata.",
    inputSchema: {
      type: "object",
      properties: {
        address: { type: "string", description: "Hex-encoded 32-byte address" },
      },
      required: ["address"],
    },
  },
  {
    name: "baud_transfer",
    description: "Create, sign, and submit a BAUD transfer. Amount is in quanta (1 BAUD = 10^18 quanta).",
    inputSchema: {
      type: "object",
      properties: {
        secret_key: { type: "string", description: "Hex-encoded sender secret key" },
        to: { type: "string", description: "Hex-encoded recipient address" },
        amount: { type: "string", description: "Amount in quanta (as string for large numbers)" },
        nonce: { type: "integer", description: "Sender nonce" },
        memo: { type: "string", description: "Optional memo" },
      },
      required: ["secret_key", "to", "amount", "nonce"],
    },
  },
  {
    name: "baud_escrow_create",
    description: "Create a hash-time-locked escrow. Funds are locked until the recipient reveals the preimage or the deadline passes.",
    inputSchema: {
      type: "object",
      properties: {
        secret_key: { type: "string", description: "Hex-encoded sender secret key" },
        recipient: { type: "string", description: "Hex-encoded recipient address" },
        amount: { type: "string", description: "Amount in quanta" },
        preimage: { type: "string", description: "Secret preimage (plaintext)" },
        deadline: { type: "integer", description: "Deadline as Unix milliseconds" },
        nonce: { type: "integer", description: "Sender nonce" },
      },
      required: ["secret_key", "recipient", "amount", "preimage", "deadline", "nonce"],
    },
  },
  {
    name: "baud_escrow_release",
    description: "Release escrowed funds by revealing the preimage (must be called by the escrow recipient).",
    inputSchema: {
      type: "object",
      properties: {
        secret_key: { type: "string", description: "Hex-encoded recipient secret key" },
        escrow_id: { type: "string", description: "Hex-encoded escrow ID" },
        preimage: { type: "string", description: "Secret preimage (plaintext)" },
        nonce: { type: "integer", description: "Sender nonce" },
      },
      required: ["secret_key", "escrow_id", "preimage", "nonce"],
    },
  },
  {
    name: "baud_escrow_refund",
    description: "Refund escrowed funds after the deadline has passed (must be called by the escrow sender).",
    inputSchema: {
      type: "object",
      properties: {
        secret_key: { type: "string", description: "Hex-encoded sender secret key" },
        escrow_id: { type: "string", description: "Hex-encoded escrow ID" },
        nonce: { type: "integer", description: "Sender nonce" },
      },
      required: ["secret_key", "escrow_id", "nonce"],
    },
  },
  {
    name: "baud_register_agent",
    description: "Register agent metadata (name, endpoint, capabilities) on-chain for discovery.",
    inputSchema: {
      type: "object",
      properties: {
        secret_key: { type: "string", description: "Hex-encoded agent secret key" },
        name: { type: "string", description: "Agent name (max 64 bytes)" },
        endpoint: { type: "string", description: "Service endpoint URL (max 256 bytes)" },
        capabilities: { type: "string", description: "Comma-separated capability tags" },
        nonce: { type: "integer", description: "Agent nonce" },
      },
      required: ["secret_key", "name", "endpoint", "capabilities", "nonce"],
    },
  },
  {
    name: "baud_status",
    description: "Get node status: chain ID, block height, mempool size, account count.",
    inputSchema: { type: "object", properties: {}, required: [] },
  },
  {
    name: "baud_escrow_info",
    description: "Look up an escrow contract by its ID.",
    inputSchema: {
      type: "object",
      properties: {
        escrow_id: { type: "string", description: "Hex-encoded escrow ID" },
      },
      required: ["escrow_id"],
    },
  },
  {
    name: "baud_mempool",
    description: "List pending transactions in the mempool.",
    inputSchema: { type: "object", properties: {}, required: [] },
  },
];

// ── Tool Handlers ───────────────────────────────────────────────────────────

async function handleTool(name, args) {
  switch (name) {
    case "baud_keygen":
      return await runBaud(["keygen"]);

    case "baud_balance": {
      const data = await fetchJSON(`/account/${args.address}`);
      return { address: args.address, balance: data.balance, balance_baud: (BigInt(data.balance) / BigInt(10 ** 18)).toString() };
    }

    case "baud_account":
      return await fetchJSON(`/account/${args.address}`);

    case "baud_transfer": {
      const cliArgs = ["transfer", "--secret", args.secret_key, "--to", args.to, "--amount", args.amount, "--nonce", String(args.nonce)];
      if (args.memo) cliArgs.push("--memo", args.memo);
      const signed = await runBaud(cliArgs);
      const result = await postJSON("/tx", {
        sender: signed.sender,
        nonce: signed.nonce,
        payload: signed.payload,
        timestamp: signed.timestamp,
        signature: signed.signature,
      });
      return { ...result, local_tx_hash: signed.tx_hash };
    }

    case "baud_escrow_create": {
      const signed = await runBaud([
        "escrow-create", "--secret", args.secret_key,
        "--recipient", args.recipient, "--amount", args.amount,
        "--preimage", args.preimage, "--deadline", String(args.deadline),
        "--nonce", String(args.nonce),
      ]);
      const result = await postJSON("/tx", {
        sender: signed.sender, nonce: signed.nonce,
        payload: signed.payload, timestamp: signed.timestamp,
        signature: signed.signature,
      });
      return { ...result, escrow_id: signed.escrow_id };
    }

    case "baud_escrow_release": {
      const signed = await runBaud([
        "escrow-release", "--secret", args.secret_key,
        "--escrow-id", args.escrow_id, "--preimage", args.preimage,
        "--nonce", String(args.nonce),
      ]);
      const result = await postJSON("/tx", {
        sender: signed.sender, nonce: signed.nonce,
        payload: signed.payload, timestamp: signed.timestamp,
        signature: signed.signature,
      });
      return result;
    }

    case "baud_escrow_refund": {
      const signed = await runBaud([
        "escrow-refund", "--secret", args.secret_key,
        "--escrow-id", args.escrow_id, "--nonce", String(args.nonce),
      ]);
      const result = await postJSON("/tx", {
        sender: signed.sender, nonce: signed.nonce,
        payload: signed.payload, timestamp: signed.timestamp,
        signature: signed.signature,
      });
      return result;
    }

    case "baud_register_agent": {
      const signed = await runBaud([
        "agent-register", "--secret", args.secret_key,
        "--name", args.name, "--endpoint", args.endpoint,
        "--capabilities", args.capabilities, "--nonce", String(args.nonce),
      ]);
      const result = await postJSON("/tx", {
        sender: signed.sender, nonce: signed.nonce,
        payload: signed.payload, timestamp: signed.timestamp,
        signature: signed.signature,
      });
      return result;
    }

    case "baud_status":
      return await fetchJSON("/status");

    case "baud_escrow_info":
      return await fetchJSON(`/escrow/${args.escrow_id}`);

    case "baud_mempool":
      return await fetchJSON("/mempool");

    default:
      throw new Error(`Unknown tool: ${name}`);
  }
}

// ── Server Setup ────────────────────────────────────────────────────────────

const server = new Server(
  { name: "baud-mcp-server", version: "0.1.0" },
  { capabilities: { tools: {} } }
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: TOOLS,
}));

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;
  try {
    const result = await handleTool(name, args || {});
    return {
      content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
    };
  } catch (error) {
    return {
      content: [{ type: "text", text: `Error: ${error.message}` }],
      isError: true,
    };
  }
});

async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error("Baud MCP server running on stdio");
}

main().catch((error) => {
  console.error("Fatal:", error);
  process.exit(1);
});
