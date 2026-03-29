use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use baud_api::routes::{build_router, record_block_txs, AppState, LotteryState};
use baud_consensus::{ConsensusConfig, ConsensusEngine};
use baud_core::crypto::{Address, KeyPair};
use baud_core::mempool::Mempool;
use baud_core::state::WorldState;
use baud_core::types::GenesisConfig;
use baud_network::{NetworkConfig, NetworkNode};
use baud_storage::BaudStore;

/// Baud Node — Full node for the M2M Agent Ledger
#[derive(Parser)]
#[command(
    name = "baud-node",
    version,
    about = "Full validator node for the Baud M2M agent ledger"
)]
struct Cli {
    /// Path to the genesis configuration JSON file.
    #[arg(long, default_value = "genesis.json")]
    genesis: String,

    /// Hex-encoded validator secret key.
    #[arg(long)]
    secret_key: String,

    /// HTTP API listen address.
    #[arg(long, default_value = "0.0.0.0:8080")]
    api_addr: String,

    /// P2P listen address.
    #[arg(long, default_value = "0.0.0.0:9944")]
    p2p_addr: String,

    /// Comma-separated list of bootstrap peer addresses (e.g., ws://1.2.3.4:9944).
    #[arg(long, default_value = "")]
    peers: String,

    /// Block production interval in milliseconds.
    #[arg(long, default_value = "1000")]
    block_interval: u64,

    /// Maximum transactions per block.
    #[arg(long, default_value = "10000")]
    max_txs_per_block: usize,

    /// Data directory for persistent storage.
    #[arg(long, default_value = "node_data")]
    data_dir: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .init();

    let cli = Cli::parse();

    // ── Load genesis ────────────────────────────────────────────────────
    let genesis_json = std::fs::read_to_string(&cli.genesis)
        .context(format!("failed to read genesis file: {}", cli.genesis))?;
    let genesis: GenesisConfig =
        serde_json::from_str(&genesis_json).context("failed to parse genesis config")?;

    info!(chain_id = %genesis.chain_id, "loading genesis configuration");

    // ── Initialize our keypair ──────────────────────────────────────────
    let keypair =
        Arc::new(KeyPair::from_secret_hex(&cli.secret_key).context("invalid secret key")?);
    info!(address = %keypair.address(), "node identity loaded");

    // ── Initialize world state from genesis ─────────────────────────────
    let store = Arc::new(
        BaudStore::open(std::path::Path::new(&cli.data_dir)).context("failed to open storage")?,
    );

    let state = match store
        .load_state()
        .context("failed to load persisted state")?
    {
        Some(persisted) => {
            info!(height = persisted.height, "resumed from persisted state");
            persisted
        }
        None => {
            let s = WorldState::from_genesis(&genesis).context("failed to init state")?;
            info!("initialized fresh state from genesis");
            s
        }
    };

    // Load extended state (new features) separately.
    let extended = store.load_extended_state().unwrap_or_else(|e| {
        warn!("failed to load extended state: {e}, using defaults");
        baud_core::types::ExtendedState::default()
    });

    let mut state = state;
    state.extended = extended;
    let state = Arc::new(RwLock::new(state));

    // ── Initialize mempool ──────────────────────────────────────────────
    let mempool = Arc::new(Mempool::new());

    // ── Consensus configuration ─────────────────────────────────────────
    let consensus_config = ConsensusConfig {
        max_txs_per_block: cli.max_txs_per_block,
        block_interval_ms: cli.block_interval,
        ..ConsensusConfig::default()
    };

    let validator_addresses: Vec<Address> = genesis.validators.iter().map(|v| v.address).collect();

    // ── Create consensus engine ─────────────────────────────────────────
    let (consensus_engine, mut finalized_rx, _consensus_tx) = ConsensusEngine::new(
        Arc::clone(&keypair),
        validator_addresses,
        Arc::clone(&state),
        Arc::clone(&mempool),
        consensus_config,
    );
    let consensus_engine = Arc::new(consensus_engine);

    // ── Create network node ─────────────────────────────────────────────
    let bootstrap_peers: Vec<String> = cli
        .peers
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let network_config = NetworkConfig {
        listen_addr: cli.p2p_addr.clone(),
        bootstrap_peers,
        max_peers: 50,
    };

    // Create channels for consensus ↔ network communication.
    let (net_to_consensus_tx, _net_to_consensus_rx) = tokio::sync::mpsc::channel(1024);
    let (_consensus_to_net_tx, consensus_to_net_rx) = tokio::sync::mpsc::channel(1024);

    let network_node = Arc::new(NetworkNode::new(
        network_config,
        net_to_consensus_tx,
        consensus_to_net_rx,
    ));

    // ── Shutdown signal ─────────────────────────────────────────────────
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    // ── Start API server ────────────────────────────────────────────────
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let app_state = AppState {
        world_state: Arc::clone(&state),
        mempool: Arc::clone(&mempool),
        chain_id: genesis.chain_id.clone(),
        node_address: keypair.address().to_hex(),
        start_time: now,
        tx_processed: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        tx_rejected: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        keypair: Some(Arc::clone(&keypair)),
        faucet_claims: Arc::new(dashmap::DashMap::new()),
        tx_history: Arc::new(parking_lot::RwLock::new(Vec::new())),
        lottery: Arc::new(parking_lot::RwLock::new(LotteryState::default())),
    };

    let router = build_router(app_state.clone());
    let api_addr = cli.api_addr.clone();

    let api_handle = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(&api_addr)
            .await
            .expect("failed to bind API listener");
        info!(addr = %api_addr, "API server started");
        axum::serve(listener, router.into_make_service())
            .await
            .expect("API server error");
    });

    // ── Start consensus engine ──────────────────────────────────────────
    let consensus_shutdown = shutdown_tx.subscribe();
    let consensus_handle = tokio::spawn({
        let engine = Arc::clone(&consensus_engine);
        async move {
            engine.run(consensus_shutdown).await;
        }
    });

    // ── Start network node ──────────────────────────────────────────────
    let network_shutdown = shutdown_tx.subscribe();
    let network_handle = tokio::spawn({
        let net = Arc::clone(&network_node);
        async move {
            net.run(network_shutdown).await;
        }
    });

    // ── Log finalized blocks and persist state ─────────────────────────
    let persist_state = Arc::clone(&state);
    let persist_store = Arc::clone(&store);
    let history_state = app_state.clone();
    let log_handle = tokio::spawn(async move {
        while let Ok(finalized) = finalized_rx.recv().await {
            let height = finalized.block.header.height;
            info!(
                height = height,
                txs = finalized.block.transactions.len(),
                hash = %finalized.block.header.hash(),
                votes = finalized.votes.len(),
                "block finalized"
            );
            // Record transaction history.
            if !finalized.block.transactions.is_empty() {
                record_block_txs(
                    &history_state,
                    &finalized.block.transactions,
                    height,
                );
            }
            // Persist state every block.
            let ws = persist_state.read().clone();
            if let Err(e) = persist_store.save_state(&ws) {
                tracing::error!(error = %e, "failed to persist state");
            }
            if let Err(e) = persist_store.save_extended_state(&ws.extended) {
                tracing::error!(error = %e, "failed to persist extended state");
            }
        }
    });

    // ── Print startup banner ────────────────────────────────────────────
    info!("╔══════════════════════════════════════════════════╗");
    info!("║         BAUD — M2M Agent Ledger Node            ║");
    info!("╠══════════════════════════════════════════════════╣");
    info!("║ Chain:     {:<38}║", genesis.chain_id);
    info!("║ Address:   {}..  ║", &keypair.address().to_hex()[..38]);
    info!("║ API:       {:<38}║", cli.api_addr);
    info!("║ P2P:       {:<38}║", cli.p2p_addr);
    info!("║ Validators: {:<37}║", genesis.validators.len());
    info!("╚══════════════════════════════════════════════════╝");

    // ── Wait for Ctrl+C ─────────────────────────────────────────────────
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for Ctrl+C");

    info!("shutdown signal received, stopping node...");
    let _ = shutdown_tx.send(());

    // Persist final state before shutdown.
    {
        let ws = state.read().clone();
        if let Err(e) = store.save_state(&ws) {
            tracing::error!(error = %e, "failed to persist state on shutdown");
        } else {
            info!(height = ws.height, "state persisted on shutdown");
        }
        if let Err(e) = store.save_extended_state(&ws.extended) {
            tracing::error!(error = %e, "failed to persist extended state on shutdown");
        }
    }

    // Wait for tasks to finish (with timeout).
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        futures_util::future::join_all(vec![
            api_handle,
            consensus_handle,
            network_handle,
            log_handle,
        ]),
    )
    .await;

    info!("node stopped cleanly");
    Ok(())
}
