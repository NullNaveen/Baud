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
| **Mining Block** | ~218,500+ (ongoing) |
| **Mined %** | ~10.93% of 1B BAUD (109.27M) ✅ TARGET REACHED |
| **Node** | Windows baud-node.exe, API port 3030, P2P port 9944 |
| **Validator address** | `17b28d62fb2fa94474ff54823159ddd992bed3d6fadd576bfc87c425b0b8ac1b` |
| **Git branch** | main |
| **Latest commit** | 91306b1 (clippy + fmt fixes) |
| **CI Status** | CI ✅ \| Pages ✅ \| Docker ✅ \| npm ✅ \| PyPI ✅ — ALL GREEN |
| **Tests** | 49/49 passing (18 baud-core + 22 integration + 4 consensus + 5 wallet) |
| **Tx Types** | 24 (was 19, added 5: CreateSubAccount, DelegatedTransfer, SetArbitrator, ArbitrateDispute, BatchTransfer) |
| **SDK** | Python v0.3.0 on PyPI, npm baud-mcp-server published |

---

## Active Tasks

### HIGH PRIORITY
- [x] **AT-1**: Pure Python SDK (PyNaCl + blake3) — DONE (v0.3.0 on PyPI)
- [x] **AT-2**: Fix SpendingPolicy co-signer validation — DONE (CoSignedTransfer tx type)
- [x] **AT-3**: PyPI publishing — DONE (baud-sdk v0.3.0, API token auth)
- [x] **AT-4**: npm publishing — DONE (Classic Automation token, workflow green)
- [x] **AT-5**: Docker image build — DONE (rust:1.85-slim)

### MEDIUM PRIORITY
- [x] **AT-6**: Agent pricing (UpdateAgentPricing tx + /v1/pricing/:addr endpoint) — DONE
- [x] **AT-7**: Reputation system (RateAgent tx + /v1/reputation/:addr endpoint) — DONE
- [x] **AT-8**: Service agreements (Create/Accept/Complete/Dispute lifecycle) — DONE
- [x] **AT-9**: Demo agent script (examples/demo_agent.py) — DONE
- [x] **AT-10**: Easy startup — DONE (start-node.bat with env var support)
- [x] **AT-11**: SDK documentation — DONE (v0.3.0 README with all 24 tx types, examples)

### LOW PRIORITY
- [x] **AT-12**: Transaction batching/atomicity — DONE (BatchTransfer tx type)
- [x] **AT-13**: Governance/voting (CreateProposal + CastVote tx types) — DONE
- [x] **AT-14**: Dispute resolution/arbitration — DONE (SetArbitrator + ArbitrateDispute tx types)
- [x] **AT-15**: Recurring payments (CreateRecurringPayment + CancelRecurringPayment) — DONE
- [x] **AT-16**: Block explorer — DONE (dashboard with TX, Escrow, Agreement, Proposal, Sub-Account, Reputation lookups)

---

## AGI/SAI Autonomy Gaps

These are the 11 critical gaps identified that prevent fully autonomous AGI/SAI agents from using Baud without human intervention. Each gap is categorized by severity and phase.

### GAP-1: No Sub-Accounts / Multi-Sig / Budgeting ✅ DONE
**Status**: ✅ DONE (CreateSubAccount + DelegatedTransfer tx types, SubAccount struct with owner/label/budget/spent/expiry)
**Phase**: 2 (Agent Economy)
**Problem**: An AGI agent cannot create sub-wallets, delegate spending to sub-agents, or set budgets for different tasks. A superintelligent system managing hundreds of sub-processes needs hierarchical financial control.
**Solution**: Added `CreateSubAccount` and `DelegatedTransfer` tx types with budget limits, expiry, and owner-delegated spending. API endpoint: `/v1/sub-account/:id`.

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

### GAP-9: No Dispute Resolution / Arbitration ✅ DONE
**Status**: ✅ DONE (SetArbitrator + ArbitrateDispute tx types with payment splitting)
**Phase**: 3 (Programmability)
**Problem**: When an escrow goes wrong (service not delivered, quality dispute), there's only refund-after-deadline. No arbitration, no partial release, no third-party mediator.
**Solution**: Added `SetArbitrator` tx (client/provider assigns arbitrator to disputed agreement) and `ArbitrateDispute` tx (arbitrator splits payment between provider and client). Updates reputation on resolution.

### GAP-10: No Network-Level Automation (AMM, Lending, etc.) ⚠️ LOW
**Status**: Not started
**Phase**: 4 (Decentralization)
**Problem**: No automated market makers, lending pools, or DeFi primitives. Agents can't earn yield on idle BAUD or swap tokens.
**Solution**: Phase 4 — requires smart contracts (GAP-4) first. Build DEX/AMM as first smart contract.

### GAP-11: No Transaction Batching / Atomicity ✅ DONE
**Status**: ✅ DONE (BatchTransfer tx type with atomic multi-recipient transfers)
**Phase**: 2 (Agent Economy)
**Problem**: Can't execute multiple transactions atomically (all-or-nothing). An AGI orchestrating a complex workflow needs atomic multi-step operations.
**Solution**: Added `BatchTransfer` tx type with `Vec<BatchEntry>` (recipient + amount pairs, max 32). Validates total vs balance atomically, all-or-nothing execution.

---

## Evolution Phases

### Phase 1: Foundation Fixes ✅ COMPLETE
**Goal**: Fix existing broken features, ship SDK, get to 10% mining.
- [x] Fix SpendingPolicy co-signer validation (GAP-2)
- [x] Pure Python SDK without CLI dependency (AT-1) — v0.3.0
- [x] Publish SDK to PyPI (AT-3) — baud-sdk v0.3.0
- [x] Publish MCP server to npm (AT-4) — Classic Automation token
- [x] Fix Docker image build (AT-5) — rust:1.85-slim
- [x] Mine to 10% (100M BAUD) — 109.27M (10.93%)
- [x] Node stability (start-node.bat)

### Phase 2: Agent Economy ✅ COMPLETE
**Goal**: Enable autonomous agent-to-agent commerce.
- [x] Add pricing to AgentMeta (GAP-8)
- [x] Agent-to-agent contracts/agreements (GAP-6)
- [x] Recurring payments/subscriptions (GAP-5)
- [x] Sub-accounts and budgeting (GAP-1)
- [x] Reputation system (GAP-3)
- [x] Transaction batching (GAP-11)
- [x] Dispute resolution with arbitration (GAP-9)
- [ ] Example: Two agents autonomously negotiating and paying for services

### Phase 3: Programmability 🔲 FUTURE
**Goal**: Enable custom logic for AGI agents.
- [ ] Smart contract / scripting engine (GAP-4)
- [ ] Conditional payments (if/then logic)
- [ ] Oracle integration for external data
- [ ] Example: Agent deploys custom escrow logic

### Phase 4: Decentralization & Scale 🔲 FUTURE
**Goal**: Full decentralized network for SAI-level autonomy.
- [ ] Transition from solo mining to multi-validator staking
- [x] Governance/voting (GAP-7)
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
| Publish Docker Image | ✅ Pass | Fixed: rust:1.83 → 1.85-slim (edition 2024) | — |
| Publish MCP Server to npm | ✅ Pass | Fixed: Classic Automation token (bypasses 2FA) | NPM_TOKEN secret configured |
| Publish Python SDK to PyPI | ✅ Pass | Fixed: switched from OIDC to API token auth | `baud-sdk` v0.3.0 published |

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

- [x] Fix SpendingPolicy co-signer validation (GAP-2) — DONE (CoSignedTransfer)
- [ ] Add rate limiting to API endpoints
- [ ] Add request size limits
- [ ] Audit Ed25519 signature verification edge cases
- [ ] Add TLS support for node API
- [ ] Implement nonce gap protection

---

## Marketing & Launch

### Pre-Launch Checklist
- [x] Mine to 10% (~100M BAUD) — 109.27M (10.93%)
- [x] SDK published to PyPI — baud-sdk v0.3.0
- [x] MCP server published to npm — baud-mcp-server
- [x] Docker image on GHCR
- [x] Documentation site live (GitHub Pages)
- [x] genesis.json committed to repo (network joinability)
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

### Session 2026-03-27
- [x] Implemented GAP-1: Sub-accounts (CreateSubAccount + DelegatedTransfer tx types)
- [x] Implemented GAP-9: Dispute arbitration (SetArbitrator + ArbitrateDispute tx types)
- [x] Implemented GAP-11: Transaction batching (BatchTransfer tx type with BatchEntry)
- [x] Added 5 new TxPayload variants (24 total), 7 new error types, SubAccount + BatchEntry structs
- [x] Added 3 new integration tests (49 total, all passing)
- [x] Updated API routes: 5 new DTOs, sub-account query endpoint
- [x] Python SDK v0.3.0: batch transfer, sub-account, delegated transfer signing
- [x] Created start-node.bat for easy Windows startup (AT-10)
- [x] Enhanced Explorer UI: Agreement, Proposal, Sub-Account, Reputation lookups (AT-16)
- [x] Fixed npm publish: Classic Automation token (AT-4 — all CI workflows GREEN)
- [x] Fixed CI: cargo fmt + clippy (or_default, unused imports)
- [x] Pushed 4 commits: 390ee30, 2e72658, 3b5325e, 91306b1

### Session 2026-03-29
- [x] Verified npm publish workflow GREEN (Classic Automation token working)
- [x] Found critical issue: genesis.json was gitignored — not on GitHub
- [x] Fixed: removed genesis.json from .gitignore so others can join the network
- [x] Updated TASK-TRACKER.md with accurate status (Phase 1 & 2 complete)
- [x] Closed 9 of 11 AGI/SAI gaps (GAP-4 and GAP-10 are Phase 3/4 future work)

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
