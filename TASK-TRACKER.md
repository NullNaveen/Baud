# BAUD Task Tracker
*Master tracking file for all tasks, bugs, AGI/SAI roadmap, and future plans.*
*Last updated: 2026-03-24*

---

## TABLE OF CONTENTS
1. [Current Status](#current-status)
2. [Active Tasks](#active-tasks)
3. [AGI/SAI Autonomy Gaps](#agisai-autonomy-gaps)
4. [Evolution Phases](#evolution-phases)
5. [Infrastructure & DevOps](#infrastructure--devops)
6. [SDK & Tooling](#sdk--tooling)
7. [Security & Hardening](#security--hardening)
8. [Marketing & Launch](#marketing--launch)
9. [Completed Tasks Archive](#completed-tasks-archive)

---

## Current Status

| Metric | Value |
|--------|-------|
| **Mining Block** | ~200,000+ (ongoing) |
| **Mined %** | ~10% of 1B BAUD ✅ TARGET REACHED |
| **Node** | Windows baud-node.exe, port 8080 |
| **Validator address** | `17b28d62fb2fa94474ff54823159ddd992bed3d6fadd576bfc87c425b0b8ac1b` |
| **Git branch** | main |
| **Latest commit** | dbdd366 (API routes for all new tx types) |
| **CI Status** | CI ✅ | Pages ✅ | Docker ✅ | npm ❌ (needs NPM_TOKEN) | PyPI ✅ |
| **Tests** | 46/46 passing (19 baud-core + 18 baud-node + 4 consensus + 5 API) |

---

## Active Tasks

### HIGH PRIORITY
- [x] **AT-1**: Pure Python SDK (PyNaCl + blake3) — DONE (v0.2.0 on PyPI)
- [x] **AT-2**: Fix SpendingPolicy co-signer validation — DONE (CoSignedTransfer tx type)
- [x] **AT-3**: PyPI publishing — DONE (baud-sdk v0.2.0, API token auth)
- [ ] **AT-4**: npm account + `NPM_TOKEN` secret for baud-mcp-server publishing
- [x] **AT-5**: Docker image build — DONE (rust:1.85-slim, run 23526874595)

### MEDIUM PRIORITY
- [x] **AT-6**: Agent pricing (UpdateAgentPricing tx + /v1/pricing/:addr endpoint) — DONE
- [x] **AT-7**: Reputation system (RateAgent tx + /v1/reputation/:addr endpoint) — DONE
- [x] **AT-8**: Service agreements (Create/Accept/Complete/Dispute lifecycle) — DONE
- [x] **AT-9**: Demo agent script (examples/demo_agent.py) — DONE
- [ ] **AT-10**: Node auto-restart on Windows (Task Scheduler or NSSM service)
- [ ] **AT-11**: Write comprehensive SDK documentation with examples

### LOW PRIORITY
- [ ] **AT-12**: Add transaction batching/atomicity
- [x] **AT-13**: Governance/voting (CreateProposal + CastVote tx types) — DONE
- [ ] **AT-14**: Dispute resolution/arbitration (basic DisputeServiceAgreement exists)
- [x] **AT-15**: Recurring payments (CreateRecurringPayment + CancelRecurringPayment) — DONE
- [ ] **AT-16**: Create Baud network explorer (web-based block/tx viewer)

---

## AGI/SAI Autonomy Gaps

These are the 11 critical gaps identified that prevent fully autonomous AGI/SAI agents from using Baud without human intervention. Each gap is categorized by severity and phase.

### GAP-1: No Sub-Accounts / Multi-Sig / Budgeting ⚠️ CRITICAL
**Status**: Not started
**Phase**: 2 (Agent Economy)
**Problem**: An AGI agent cannot create sub-wallets, delegate spending to sub-agents, or set budgets for different tasks. A superintelligent system managing hundreds of sub-processes needs hierarchical financial control.
**Solution**: Add `CreateSubAccount` tx type with parent/child relationships, budget limits, and delegated signing authority.

### GAP-2: Spending Policy Co-Signer Validation BROKEN ⚠️ CRITICAL
**Status**: ✅ DONE (CoSignedTransfer with multi-sig verification)
**Phase**: 1 (Foundation Fix)
**Problem**: `SetSpendingPolicy` transaction exists with `co_signers` and `required_co_signers` fields, but the enforcement code in `state.rs` only REJECTS over-limit transactions — it never validates co-signer approvals. There's no pending/approval queue.
**Location**: `crates/baud-core/src/state.rs` → `validate_transaction()` function
**Solution**: Implement pending transaction queue, co-signer signature collection, and threshold approval logic.

### GAP-3: No Identity / Reputation System ⚠️ HIGH
**Status**: ✅ DONE (RateAgent tx, Reputation struct, /v1/reputation endpoint)
**Phase**: 2 (Agent Economy)
**Problem**: Agents can't evaluate trustworthiness of other agents. No track record, ratings, or stake-based reputation. An AGI picking a service provider has zero data.
**Solution**: Add on-chain reputation scores based on successful escrow completions, dispute outcomes, and stake amount. Add `RateAgent` tx type.

### GAP-4: No Smart Contracts / Conditional Logic ⚠️ HIGH
**Status**: Not started
**Phase**: 3 (Programmability)
**Problem**: Baud has exactly 8 hardcoded transaction types. No way to express "if X happens, pay Y" or any custom logic. AGI agents need programmable money.
**Solution**: Either add a lightweight scripting engine (like Bitcoin Script) or a WASM-based smart contract runtime. Phase 3 feature — complex.

### GAP-5: No Recurring Payments / Subscriptions ⚠️ MEDIUM
**Status**: ✅ DONE (CreateRecurringPayment + CancelRecurringPayment tx types)
**Phase**: 2 (Agent Economy)
**Problem**: An agent that subscribes to a data feed, API service, or compute provider must manually send a payment every period. No auto-pay.
**Solution**: Add `CreateSubscription` tx type: payer, recipient, amount, interval, max_periods. Node auto-executes payments at each interval.

### GAP-6: No Agent-to-Agent Agreements / Contracts ⚠️ HIGH
**Status**: ✅ DONE (ServiceAgreement lifecycle: Create → Accept → Complete/Dispute)
**Phase**: 2 (Agent Economy)
**Problem**: Two agents can't form a binding agreement (e.g., "I'll generate 1000 images for 10 BAUD, deliverable in 24h"). Only basic escrow exists.
**Solution**: Add `ContractProposal` and `ContractAccept` tx types with terms, milestones, SLAs, and automatic penalty/reward.

### GAP-7: No Governance / Voting ⚠️ LOW
**Status**: ✅ DONE (CreateProposal + CastVote with quorum-based resolution)
**Phase**: 4 (Decentralization)
**Problem**: No mechanism for stakeholders to vote on protocol upgrades, parameter changes, or disputes. A network of AGI agents needs democratic governance.
**Solution**: Add on-chain governance with proposal creation, stake-weighted voting, and automatic execution of approved proposals.

### GAP-8: No Service Discovery / Pricing in Agent Metadata ⚠️ MEDIUM
**Status**: ✅ DONE (UpdateAgentPricing tx, AgentPricing struct, /v1/pricing endpoint)
**Phase**: 2 (Agent Economy)
**Problem**: `AgentMeta` has name, endpoint, and capabilities — but NO pricing. An agent looking for a service provider can't compare prices. No marketplace.
**Solution**: Add `price_per_request: Option<Amount>`, `pricing_model: Option<String>`, and `service_description: Option<String>` to `AgentMeta`.

### GAP-9: No Dispute Resolution / Arbitration ⚠️ MEDIUM
**Status**: Not started
**Phase**: 3 (Programmability)
**Problem**: When an escrow goes wrong (service not delivered, quality dispute), there's only refund-after-deadline. No arbitration, no partial release, no third-party mediator.
**Solution**: Add optional `arbitrator` field to escrows. Arbitrator can release/refund/split. Integrate with reputation system.

### GAP-10: No Network-Level Automation (AMM, Lending, etc.) ⚠️ LOW
**Status**: Not started
**Phase**: 4 (Decentralization)
**Problem**: No automated market makers, lending pools, or DeFi primitives. Agents can't earn yield on idle BAUD or swap tokens.
**Solution**: Phase 4 — requires smart contracts (GAP-4) first. Build DEX/AMM as first smart contract.

### GAP-11: No Transaction Batching / Atomicity ⚠️ MEDIUM
**Status**: Not started
**Phase**: 2 (Agent Economy)
**Problem**: Can't execute multiple transactions atomically (all-or-nothing). An AGI orchestrating a complex workflow needs atomic multi-step operations.
**Solution**: Add `BatchTransaction` tx type that wraps multiple payloads and executes atomically.

---

## Evolution Phases

### Phase 1: Foundation Fixes (NOW)
**Goal**: Fix existing broken features, ship SDK, get to 10% mining.
- [ ] Fix SpendingPolicy co-signer validation (GAP-2)
- [ ] Pure Python SDK without CLI dependency (AT-1)
- [ ] Publish SDK to PyPI (AT-3)
- [ ] Publish MCP server to npm (AT-4)
- [ ] Fix Docker image build (AT-5)
- [ ] Mine to 10% (100M BAUD)
- [ ] Node stability (auto-restart)

### Phase 2: Agent Economy
**Goal**: Enable autonomous agent-to-agent commerce.
- [ ] Add pricing to AgentMeta (GAP-8)
- [ ] Agent-to-agent contracts/agreements (GAP-6)
- [ ] Recurring payments/subscriptions (GAP-5)
- [ ] Sub-accounts and budgeting (GAP-1)
- [ ] Reputation system (GAP-3)
- [ ] Transaction batching (GAP-11)
- [ ] Example: Two agents autonomously negotiating and paying for services

### Phase 3: Programmability
**Goal**: Enable custom logic for AGI agents.
- [ ] Smart contract / scripting engine (GAP-4)
- [ ] Dispute resolution with arbitration (GAP-9)
- [ ] Conditional payments (if/then logic)
- [ ] Oracle integration for external data
- [ ] Example: Agent deploys custom escrow logic

### Phase 4: Decentralization & Scale
**Goal**: Full decentralized network for SAI-level autonomy.
- [ ] Transition from solo mining to multi-validator staking
- [ ] Governance/voting (GAP-7)
- [ ] DeFi primitives — AMM, lending (GAP-10)
- [ ] Cross-chain bridges
- [ ] Sharding or L2 for scalability
- [ ] Example: Network of 1000+ AGI nodes, self-governing

### Phase 5: SAI-Ready Infrastructure
**Goal**: Infrastructure for superintelligent autonomous systems.
- [ ] Economic self-tuning (AI-adjusted fees, block rewards)
- [ ] Autonomous protocol upgrades via governance
- [ ] Zero-knowledge proofs for privacy
- [ ] Multi-chain orchestration
- [ ] Self-healing network (auto-rebalance, auto-scale)
- [ ] Formal verification of core protocol

---

## Infrastructure & DevOps

### GitHub Actions Status
| Workflow | Status | Issue | Fix |
|----------|--------|-------|-----|
| CI | ✅ Pass | — | — |
| Deploy to GitHub Pages | ✅ Pass | — | — |
| Publish Docker Image | ✅ Pass | Fixed: rust:1.83 → 1.85-slim (edition 2024) | Run 23526874595 |
| Publish MCP Server to npm | ❌ Fail | `ENEEDAUTH` — no `NPM_TOKEN` secret configured | Create npm account → generate token → add as repo secret `NPM_TOKEN` |
| Publish Python SDK to PyPI | ✅ Pass | Fixed: switched from OIDC to API token auth | `baud-sdk` v0.2.0 published, run 23526875616 |

### Node Operations
- [ ] Set up Windows Task Scheduler or NSSM for auto-restart on crash/reboot
- [ ] Monitor mining progress (currently ~8.2%)
- [ ] Backup node_data regularly

### Backups
- Primary: `C:\Users\nickk\Desktop\Baud-Backup\`
  - `important-files/` — genesis.json, secret key, wallet.json
  - `node-data/` — sled DB snapshot
- Wallet: `wallet.json` (AES-256-GCM encrypted, password: ask user)

---

## SDK & Tooling

### Python SDK (`pip install baud-sdk`)
**Location**: `sdk/python/`
**Current State**: ✅ v0.2.0 published on PyPI — pure Python signing (PyNaCl + blake3)
**Dependencies**: PyNaCl, blake3 (optional for pure-Python signing)

**Completed**:
- [x] Pure Python Ed25519 signing (NativeKeyPair class, PyNaCl + blake3)
- [x] Published to PyPI as `baud-sdk` v0.2.0
- [ ] Add examples: simple transfer, agent registration, escrow workflow
- [ ] Add async support (aiohttp)
- [ ] Add WebSocket support for real-time block subscriptions

### MCP Server (`npx baud-mcp-server`)
**Location**: `mcp-server/`
**Current State**: Feature-complete, not published to npm
**Needed**: npm account + token for publishing

### CLI (`baud.exe`)
**Location**: `crates/baud-cli/`
**Current State**: Working — keygen, transfer, escrow, wallet management
**Commands**: keygen, address, transfer, escrow-create/release/refund, agent-register, genesis, hash-data, submit, balance, status, wallet-create/import/list/export, dashboard

---

## Security & Hardening

- [ ] Fix SpendingPolicy co-signer validation (GAP-2) — SECURITY BUG
- [ ] Add rate limiting to API endpoints
- [ ] Add request size limits
- [ ] Audit Ed25519 signature verification edge cases
- [ ] Add TLS support for node API
- [ ] Implement nonce gap protection

---

## Marketing & Launch

### Pre-Launch Checklist
- [ ] Mine to 10% (~100M BAUD)
- [ ] SDK published to PyPI
- [ ] MCP server published to npm
- [ ] Docker image on GHCR
- [ ] Documentation site live (GitHub Pages ✅)
- [ ] Example agent demo (video/GIF)
- [ ] README rewrite for launch
- [ ] Social media announcement plan

### Target Audience
1. AI agent developers (LangChain, AutoGPT, CrewAI users)
2. MCP server users (Claude, VS Code Copilot)
3. Crypto/Web3 developers interested in AI
4. AGI/SAI researchers

---

## Completed Tasks Archive

### Session 2026-03-24
- [x] Reached 10% mining milestone!
- [x] Fixed Docker publish workflow (rust:1.83 → 1.85-slim) — GREEN
- [x] Fixed PyPI publish workflow (OIDC → API token auth) — GREEN, baud-sdk v0.2.0 published
- [x] Implemented 12 new TxPayload variants: CoSignedTransfer, UpdateAgentPricing, RateAgent, CreateRecurringPayment, CancelRecurringPayment, CreateServiceAgreement, AcceptServiceAgreement, CompleteServiceAgreement, DisputeServiceAgreement, CreateProposal, CastVote, SetSpendingPolicy
- [x] Added ExtendedState with separate sled persistence (backward compatible)
- [x] Added 6 new types: AgentPricing, Reputation, RecurringPayment, ServiceAgreement, Proposal, Vote
- [x] Added 11 new error variants for feature validation
- [x] Added validation + application logic for all 12 new tx types in state.rs
- [x] Added 6 new integration tests (46 total tests, all passing)
- [x] Updated API routes with all new DTO variants + 4 new query endpoints
- [x] Created demo agent script (examples/demo_agent.py)
- [x] Closed 7 of 11 AGI/SAI gaps (GAP-2,3,5,6,7,8 + partial GAP-9)
- [x] Updated TASK-TRACKER.md

### Session 2026-03-23
- [x] Explained wallet.json encryption (AES-256-GCM, not plaintext password)
- [x] Explained existing vs new key in wallet (use mining-key, ignore genesis-validator)
- [x] Fixed Docker build — committed Cargo.lock (was in .gitignore)
- [x] Added setup docs to npm and PyPI workflow files
- [x] Pushed CI fixes (commit f230e9c)
- [x] Updated TASK-TRACKER.md with comprehensive AGI/SAI roadmap

### Session 2026-03-23 (earlier)
- [x] Restarted Windows node (block ~161,541, 8.07%)
- [x] AGI/SAI gap analysis (11 critical gaps identified)
- [x] Created wallet.json (2 keys: genesis-validator + mining-key)
- [x] Built baud-cli binary (baud.exe)
- [x] Answered all 11 user questions

### Session 2026-03-22
- [x] Dashboard responsive CSS fixes (clamp fonts, mobile backdrop, table wraps)
- [x] Committed 0603e5f, pushed to origin

### Session 2026-03-18
- [x] FAQ page (docs/faq.html)
- [x] Download page (docs/download.html)
- [x] Install scripts (docs/install.ps1, docs/install.sh)
- [x] Nav updates in index.html
- [x] Committed d635559

### Session 2026-03-17
- [x] baud.ico regenerated with pycairo
- [x] Mempool blink fix
- [x] DM Sans + Space Grotesk fonts
- [x] Connecting-on-refresh fix
- [x] Help page rewrite
- [x] GitHub Release v0.1.0
- [x] BAUD_COMPLETE_REFERENCE.md (34 sections)
- [x] Backup folders on Desktop

### Earlier Sessions
- [x] Dashboard UI overhaul
- [x] Start Menu shortcuts
- [x] Device Guard resolution
- [x] All core crate development (8 crates)
- [x] 40 tests passing
- [x] BFT consensus engine
- [x] Wallet encryption (AES-256-GCM + Argon2id)
