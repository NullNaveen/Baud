use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::{ConnectInfo, Path, State},
    http::StatusCode,
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use axum::extract::Query;
use axum::response::Html;

use baud_core::crypto::{Address, Hash, KeyPair, Signature};
use baud_core::mempool::Mempool;
use baud_core::state::WorldState;
use baud_core::types::{EscrowStatus, Transaction, TxPayload};

static DASHBOARD_HTML: &str = include_str!("../dashboard.html");

// ─── Shared App State ───────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub world_state: Arc<RwLock<WorldState>>,
    pub mempool: Arc<Mempool>,
    pub chain_id: String,
    pub node_address: String,
    pub start_time: u64,
    pub tx_processed: Arc<AtomicU64>,
    pub tx_rejected: Arc<AtomicU64>,
}

// ─── Request/Response DTOs ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SubmitTxRequest {
    pub sender: String,
    pub nonce: u64,
    pub payload: TxPayloadDto,
    pub timestamp: u64,
    pub chain_id: String,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum TxPayloadDto {
    Transfer {
        to: String,
        amount: u128,
        memo: Option<String>,
    },
    EscrowCreate {
        recipient: String,
        amount: u128,
        hash_lock: String,
        deadline: u64,
    },
    EscrowRelease {
        escrow_id: String,
        preimage: String,
    },
    EscrowRefund {
        escrow_id: String,
    },
    AgentRegister {
        name: String,
        endpoint: String,
        capabilities: Vec<String>,
    },
}

#[derive(Debug, Serialize)]
pub struct SubmitTxResponse {
    pub tx_hash: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct AccountResponse {
    pub address: String,
    pub balance: String,
    pub nonce: u64,
    pub agent_meta: Option<AgentMetaDto>,
}

#[derive(Debug, Serialize)]
pub struct AgentMetaDto {
    pub name: String,
    pub endpoint: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BlockResponse {
    pub height: u64,
    pub hash: String,
    pub prev_hash: String,
    pub tx_root: String,
    pub state_root: String,
    pub timestamp: u64,
    pub proposer: String,
    pub tx_count: u32,
}

#[derive(Debug, Serialize)]
pub struct EscrowResponse {
    pub id: String,
    pub sender: String,
    pub recipient: String,
    pub amount: String,
    pub hash_lock: String,
    pub deadline: u64,
    pub status: String,
    pub created_at_height: u64,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub chain_id: String,
    pub height: u64,
    pub state_root: String,
    pub last_block_hash: String,
    pub mempool_size: usize,
    pub accounts: usize,
    pub escrows: usize,
    pub node_address: String,
    pub uptime_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct MempoolResponse {
    pub pending_count: usize,
    pub transactions: Vec<MempoolTxDto>,
}

#[derive(Debug, Serialize)]
pub struct MempoolTxDto {
    pub hash: String,
    pub sender: String,
    pub nonce: u64,
    pub timestamp: u64,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ─── Rate Limiter ───────────────────────────────────────────────────────────

/// Token-bucket rate limiter keyed by IP address.
#[derive(Clone)]
pub struct RateLimiter {
    /// Map from IP → (tokens remaining, last refill instant).
    buckets: Arc<DashMap<IpAddr, (f64, Instant)>>,
    /// Maximum tokens (burst capacity).
    pub max_tokens: f64,
    /// Tokens added per second.
    pub refill_rate: f64,
}

impl RateLimiter {
    /// Create a new rate limiter.
    /// `max_tokens` = burst size, `refill_rate` = requests/second steady state.
    pub fn new(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            max_tokens,
            refill_rate,
        }
    }

    /// Try to consume one token for the given IP.
    /// Returns `true` if allowed, `false` if rate-limited.
    fn try_acquire(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut entry = self.buckets.entry(ip).or_insert((self.max_tokens, now));

        let (tokens, last_refill) = entry.value_mut();
        let elapsed = now.duration_since(*last_refill).as_secs_f64();
        *tokens = (*tokens + elapsed * self.refill_rate).min(self.max_tokens);
        *last_refill = now;

        if *tokens >= 1.0 {
            *tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Axum middleware that rejects requests exceeding the rate limit.
/// When behind a reverse proxy, checks X-Forwarded-For to get the real
/// client IP, falling back to the direct TCP connection IP.
async fn rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    req: axum::extract::Request,
    next: Next,
) -> impl IntoResponse {
    // Prefer X-Forwarded-For (first entry = original client), fall back to TCP peer.
    let ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .and_then(|s| s.trim().parse::<IpAddr>().ok())
        .or_else(|| {
            req.extensions()
                .get::<ConnectInfo<std::net::SocketAddr>>()
                .map(|ci| ci.0.ip())
        })
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));

    if limiter.try_acquire(ip) {
        next.run(req).await.into_response()
    } else {
        warn!(%ip, "rate limited");
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorResponse {
                error: "rate limit exceeded — try again shortly".into(),
            }),
        )
            .into_response()
    }
}

// ─── Router ─────────────────────────────────────────────────────────────────

pub fn build_router(state: AppState) -> Router {
    build_router_with_rate_limit(state, RateLimiter::new(60.0, 20.0))
}

/// Build the router with a custom rate limiter (useful for tests).
pub fn build_router_with_rate_limit(state: AppState, limiter: RateLimiter) -> Router {
    Router::new()
        .route("/", get(serve_dashboard))
        .route("/dashboard", get(serve_dashboard))
        .route("/v1/status", get(get_status))
        .route("/v1/account/:address", get(get_account))
        .route("/v1/tx", post(submit_tx))
        .route("/v1/tx/:hash", get(get_tx))
        .route("/v1/escrow/:id", get(get_escrow))
        .route("/v1/mempool", get(get_mempool))
        .route("/v1/health", get(get_health))
        .route("/v1/metrics", get(get_metrics))
        .route("/v1/mining", get(get_mining_info))
        .route("/v1/keygen", get(keygen))
        .route("/v1/sign-and-submit", post(sign_and_submit))
        .layer(RequestBodyLimitLayer::new(128 * 1024)) // 128 KiB max body
        .layer(middleware::from_fn_with_state(
            limiter,
            rate_limit_middleware,
        ))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

// ─── Handlers ───────────────────────────────────────────────────────────────

async fn get_status(State(state): State<AppState>) -> impl IntoResponse {
    let ws = state.world_state.read();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    Json(StatusResponse {
        chain_id: ws.chain_id.clone(),
        height: ws.height,
        state_root: ws.state_root().to_hex(),
        last_block_hash: ws.last_block_hash.to_hex(),
        mempool_size: state.mempool.len(),
        accounts: ws.accounts.len(),
        escrows: ws.escrows.len(),
        node_address: state.node_address.clone(),
        uptime_ms: now.saturating_sub(state.start_time),
    })
}

async fn get_account(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<AccountResponse>, (StatusCode, Json<ErrorResponse>)> {
    let addr = Address::from_hex(&address).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("invalid address: {e}"),
            }),
        )
    })?;

    let ws = state.world_state.read();
    let account = ws.get_account(&addr);

    let agent_meta = account.agent_meta.as_ref().map(|m| AgentMetaDto {
        name: String::from_utf8_lossy(&m.name).to_string(),
        endpoint: String::from_utf8_lossy(&m.endpoint).to_string(),
        capabilities: m
            .capabilities
            .iter()
            .map(|c| String::from_utf8_lossy(c).to_string())
            .collect(),
    });

    Ok(Json(AccountResponse {
        address: addr.to_hex(),
        balance: account.balance.to_string(),
        nonce: account.nonce,
        agent_meta,
    }))
}

async fn submit_tx(
    State(state): State<AppState>,
    Json(req): Json<SubmitTxRequest>,
) -> Result<(StatusCode, Json<SubmitTxResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Parse the request into a Transaction.
    let tx = parse_tx_request(req).inspect_err(|_e| {
        state.tx_rejected.fetch_add(1, Ordering::Relaxed);
    })?;

    // Validate structure.
    tx.validate_structure().map_err(|e| {
        state.tx_rejected.fetch_add(1, Ordering::Relaxed);
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("{e}"),
            }),
        )
    })?;

    // Validate against state.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    {
        let ws = state.world_state.read();
        ws.validate_transaction(&tx, now).map_err(|e| {
            state.tx_rejected.fetch_add(1, Ordering::Relaxed);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("{e}"),
                }),
            )
        })?;
    }

    // Add to mempool.
    let tx_hash = state.mempool.add(tx).map_err(|e| {
        state.tx_rejected.fetch_add(1, Ordering::Relaxed);
        (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: format!("{e}"),
            }),
        )
    })?;

    state.tx_processed.fetch_add(1, Ordering::Relaxed);
    info!(tx = %tx_hash, "transaction accepted into mempool");

    Ok((
        StatusCode::ACCEPTED,
        Json(SubmitTxResponse {
            tx_hash: tx_hash.to_hex(),
            status: "pending".into(),
        }),
    ))
}

async fn get_tx(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Json<MempoolTxDto>, (StatusCode, Json<ErrorResponse>)> {
    let tx_hash = Hash::from_hex(&hash).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("invalid hash: {e}"),
            }),
        )
    })?;

    match state.mempool.get(&tx_hash) {
        Some(tx) => Ok(Json(MempoolTxDto {
            hash: tx.hash().to_hex(),
            sender: tx.sender.to_hex(),
            nonce: tx.nonce,
            timestamp: tx.timestamp,
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "transaction not found in mempool".into(),
            }),
        )),
    }
}

async fn get_escrow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<EscrowResponse>, (StatusCode, Json<ErrorResponse>)> {
    let escrow_id = Hash::from_hex(&id).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("invalid escrow ID: {e}"),
            }),
        )
    })?;

    let ws = state.world_state.read();
    match ws.escrows.get(&escrow_id) {
        Some(escrow) => Ok(Json(EscrowResponse {
            id: escrow.id.to_hex(),
            sender: escrow.sender.to_hex(),
            recipient: escrow.recipient.to_hex(),
            amount: escrow.amount.to_string(),
            hash_lock: escrow.hash_lock.to_hex(),
            deadline: escrow.deadline,
            status: match escrow.status {
                EscrowStatus::Active => "active".into(),
                EscrowStatus::Released => "released".into(),
                EscrowStatus::Refunded => "refunded".into(),
            },
            created_at_height: escrow.created_at_height,
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "escrow not found".into(),
            }),
        )),
    }
}

async fn get_mempool(State(state): State<AppState>) -> impl IntoResponse {
    let pending = state.mempool.get_pending(100);
    Json(MempoolResponse {
        pending_count: state.mempool.len(),
        transactions: pending
            .iter()
            .map(|tx| MempoolTxDto {
                hash: tx.hash().to_hex(),
                sender: tx.sender.to_hex(),
                nonce: tx.nonce,
                timestamp: tx.timestamp,
            })
            .collect(),
    })
}

/// Prometheus-compatible metrics endpoint for monitoring/observability.
async fn get_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let ws = state.world_state.read();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let uptime = now.saturating_sub(state.start_time);
    let tx_ok = state.tx_processed.load(Ordering::Relaxed);
    let tx_err = state.tx_rejected.load(Ordering::Relaxed);

    let metrics = format!(
        "# HELP baud_block_height Current block height\n\
         # TYPE baud_block_height gauge\n\
         baud_block_height {}\n\
         # HELP baud_accounts_total Total number of accounts\n\
         # TYPE baud_accounts_total gauge\n\
         baud_accounts_total {}\n\
         # HELP baud_escrows_active Active escrows\n\
         # TYPE baud_escrows_active gauge\n\
         baud_escrows_active {}\n\
         # HELP baud_milestone_escrows_active Active milestone escrows\n\
         # TYPE baud_milestone_escrows_active gauge\n\
         baud_milestone_escrows_active {}\n\
         # HELP baud_mempool_size Current mempool size\n\
         # TYPE baud_mempool_size gauge\n\
         baud_mempool_size {}\n\
         # HELP baud_tx_processed_total Total transactions processed\n\
         # TYPE baud_tx_processed_total counter\n\
         baud_tx_processed_total {}\n\
         # HELP baud_tx_rejected_total Total transactions rejected\n\
         # TYPE baud_tx_rejected_total counter\n\
         baud_tx_rejected_total {}\n\
         # HELP baud_uptime_ms Node uptime in milliseconds\n\
         # TYPE baud_uptime_ms counter\n\
         baud_uptime_ms {}\n",
        ws.height,
        ws.accounts.len(),
        ws.escrows.len(),
        ws.milestone_escrows.len(),
        state.mempool.len(),
        tx_ok,
        tx_err,
        uptime,
    );

    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        metrics,
    )
}

/// Health check endpoint — returns 200 if the node is operational.
async fn get_health(State(state): State<AppState>) -> impl IntoResponse {
    let ws = state.world_state.read();
    Json(serde_json::json!({
        "status": "healthy",
        "chain_id": state.chain_id,
        "block_height": ws.height,
        "accounts": ws.accounts.len(),
    }))
}

/// Mining info endpoint — shows current block reward, halving schedule, supply stats.
async fn get_mining_info(State(state): State<AppState>) -> impl IntoResponse {
    use baud_core::types::{
        block_reward_at, total_mined_at, HALVING_INTERVAL, INITIAL_BLOCK_REWARD, QUANTA_PER_BAUD,
        TOTAL_SUPPLY_QUANTA,
    };

    let ws = state.world_state.read();
    let height = ws.height;
    let current_reward = block_reward_at(height.saturating_add(1));
    let total_mined = total_mined_at(height);
    let era = if height == 0 {
        0
    } else {
        height / HALVING_INTERVAL
    };
    let next_halving = (era + 1) * HALVING_INTERVAL;
    let blocks_until_halving = next_halving.saturating_sub(height);
    let percent_mined = if TOTAL_SUPPLY_QUANTA > 0 {
        (total_mined as f64 / TOTAL_SUPPLY_QUANTA as f64) * 100.0
    } else {
        0.0
    };

    Json(serde_json::json!({
        "block_height": height,
        "current_block_reward_baud": current_reward / QUANTA_PER_BAUD,
        "current_block_reward_quanta": current_reward.to_string(),
        "initial_block_reward_baud": INITIAL_BLOCK_REWARD / QUANTA_PER_BAUD,
        "halving_interval": HALVING_INTERVAL,
        "current_era": era,
        "next_halving_block": next_halving,
        "blocks_until_halving": blocks_until_halving,
        "total_mined_baud": total_mined / QUANTA_PER_BAUD,
        "total_mined_quanta": total_mined.to_string(),
        "total_supply_baud": TOTAL_SUPPLY_QUANTA / QUANTA_PER_BAUD,
        "percent_mined": format!("{:.4}", percent_mined),
    }))
}

// ─── Dashboard ──────────────────────────────────────────────────────────────

async fn serve_dashboard() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

// ─── Keygen ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct KeygenQuery {
    derive: Option<String>,
}

async fn keygen(
    Query(q): Query<KeygenQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    match q.derive {
        Some(secret) => {
            let kp = KeyPair::from_secret_hex(&secret).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("invalid secret key: {e}"),
                    }),
                )
            })?;
            Ok(Json(
                serde_json::json!({ "address": kp.address().to_hex() }),
            ))
        }
        None => {
            let kp = KeyPair::generate();
            Ok(Json(serde_json::json!({
                "address": kp.address().to_hex(),
                "secret_key": kp.secret_hex(),
            })))
        }
    }
}

// ─── Sign & Submit (dashboard convenience) ──────────────────────────────────

#[derive(Debug, Deserialize)]
struct SignAndSubmitRequest {
    #[serde(rename = "type")]
    tx_type: String,
    secret: String,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    recipient: Option<String>,
    #[serde(default)]
    amount: Option<u128>,
    nonce: u64,
    #[serde(default)]
    memo: Option<String>,
    #[serde(default)]
    preimage: Option<String>,
    #[serde(default)]
    deadline: Option<u64>,
    #[serde(default)]
    chain_id: Option<String>,
    // Agent registration fields
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    endpoint: Option<String>,
    #[serde(default)]
    capabilities: Option<Vec<String>>,
    // Escrow release/refund fields
    #[serde(default)]
    escrow_id: Option<String>,
}

async fn sign_and_submit(
    State(state): State<AppState>,
    Json(req): Json<SignAndSubmitRequest>,
) -> Result<(StatusCode, Json<SubmitTxResponse>), (StatusCode, Json<ErrorResponse>)> {
    let kp = KeyPair::from_secret_hex(&req.secret).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("invalid secret key: {e}"),
            }),
        )
    })?;

    let chain_id = req.chain_id.unwrap_or_else(|| state.chain_id.clone());

    let payload = match req.tx_type.as_str() {
        "Transfer" => {
            let to_str = req.to.ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "missing 'to' field".into(),
                    }),
                )
            })?;
            let to_addr = Address::from_hex(&to_str).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("invalid address: {e}"),
                    }),
                )
            })?;
            let amount = req.amount.ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "missing 'amount' field".into(),
                    }),
                )
            })?;
            TxPayload::Transfer {
                to: to_addr,
                amount,
                memo: req.memo.map(|m| m.into_bytes()),
            }
        }
        "EscrowCreate" => {
            let recip_str = req.recipient.ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "missing 'recipient' field".into(),
                    }),
                )
            })?;
            let recip_addr = Address::from_hex(&recip_str).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("invalid address: {e}"),
                    }),
                )
            })?;
            let amount = req.amount.ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "missing 'amount' field".into(),
                    }),
                )
            })?;
            let preimage_str = req.preimage.ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "missing 'preimage' field".into(),
                    }),
                )
            })?;
            let deadline = req.deadline.ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "missing 'deadline' field".into(),
                    }),
                )
            })?;
            let hash_lock = Hash::digest(preimage_str.as_bytes());
            TxPayload::EscrowCreate {
                recipient: recip_addr,
                amount,
                hash_lock,
                deadline,
            }
        }
        "EscrowRelease" => {
            let eid_str = req.escrow_id.ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "missing 'escrow_id' field".into(),
                    }),
                )
            })?;
            let escrow_id = Hash::from_hex(&eid_str).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("invalid escrow_id: {e}"),
                    }),
                )
            })?;
            let preimage_str = req.preimage.ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "missing 'preimage' field".into(),
                    }),
                )
            })?;
            TxPayload::EscrowRelease {
                escrow_id,
                preimage: preimage_str.into_bytes(),
            }
        }
        "EscrowRefund" => {
            let eid_str = req.escrow_id.ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "missing 'escrow_id' field".into(),
                    }),
                )
            })?;
            let escrow_id = Hash::from_hex(&eid_str).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("invalid escrow_id: {e}"),
                    }),
                )
            })?;
            TxPayload::EscrowRefund { escrow_id }
        }
        "AgentRegister" => {
            let name = req.name.ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "missing 'name' field".into(),
                    }),
                )
            })?;
            let endpoint = req.endpoint.unwrap_or_default();
            let capabilities = req.capabilities.unwrap_or_default();
            TxPayload::AgentRegister {
                name: name.into_bytes(),
                endpoint: endpoint.into_bytes(),
                capabilities: capabilities.into_iter().map(|c| c.into_bytes()).collect(),
            }
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("unsupported tx type: {}", req.tx_type),
                }),
            ))
        }
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let mut tx = Transaction {
        sender: kp.address(),
        nonce: req.nonce,
        payload,
        timestamp: now,
        chain_id,
        signature: Signature::zero(),
    };

    let hash = tx.signable_hash();
    tx.signature = kp.sign(hash.as_bytes());

    // Validate
    tx.validate_structure().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("{e}"),
            }),
        )
    })?;
    {
        let ws = state.world_state.read();
        ws.validate_transaction(&tx, now).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("{e}"),
                }),
            )
        })?;
    }

    let tx_hash = state.mempool.add(tx).map_err(|e| {
        (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: format!("{e}"),
            }),
        )
    })?;
    state.tx_processed.fetch_add(1, Ordering::Relaxed);

    Ok((
        StatusCode::ACCEPTED,
        Json(SubmitTxResponse {
            tx_hash: tx_hash.to_hex(),
            status: "pending".into(),
        }),
    ))
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn parse_tx_request(
    req: SubmitTxRequest,
) -> Result<Transaction, (StatusCode, Json<ErrorResponse>)> {
    let sender = Address::from_hex(&req.sender).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("invalid sender address: {e}"),
            }),
        )
    })?;

    let signature = Signature::from_hex(&req.signature).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("invalid signature: {e}"),
            }),
        )
    })?;

    let payload = match req.payload {
        TxPayloadDto::Transfer { to, amount, memo } => {
            let to_addr = Address::from_hex(&to).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("invalid recipient address: {e}"),
                    }),
                )
            })?;
            TxPayload::Transfer {
                to: to_addr,
                amount,
                memo: memo.map(|m| m.into_bytes()),
            }
        }
        TxPayloadDto::EscrowCreate {
            recipient,
            amount,
            hash_lock,
            deadline,
        } => {
            let to_addr = Address::from_hex(&recipient).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("invalid recipient address: {e}"),
                    }),
                )
            })?;
            let hl = Hash::from_hex(&hash_lock).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("invalid hash_lock: {e}"),
                    }),
                )
            })?;
            TxPayload::EscrowCreate {
                recipient: to_addr,
                amount,
                hash_lock: hl,
                deadline,
            }
        }
        TxPayloadDto::EscrowRelease {
            escrow_id,
            preimage,
        } => {
            let eid = Hash::from_hex(&escrow_id).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("invalid escrow_id: {e}"),
                    }),
                )
            })?;
            TxPayload::EscrowRelease {
                escrow_id: eid,
                preimage: hex::decode(&preimage).map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: format!("invalid preimage hex: {e}"),
                        }),
                    )
                })?,
            }
        }
        TxPayloadDto::EscrowRefund { escrow_id } => {
            let eid = Hash::from_hex(&escrow_id).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("invalid escrow_id: {e}"),
                    }),
                )
            })?;
            TxPayload::EscrowRefund { escrow_id: eid }
        }
        TxPayloadDto::AgentRegister {
            name,
            endpoint,
            capabilities,
        } => TxPayload::AgentRegister {
            name: name.into_bytes(),
            endpoint: endpoint.into_bytes(),
            capabilities: capabilities.into_iter().map(|c| c.into_bytes()).collect(),
        },
    };

    Ok(Transaction {
        sender,
        nonce: req.nonce,
        payload,
        timestamp: req.timestamp,
        chain_id: req.chain_id,
        signature,
    })
}
