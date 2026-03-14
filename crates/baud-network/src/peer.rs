use std::sync::Arc;

use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::{accept_async, connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use baud_consensus::ConsensusMessage;
use baud_core::crypto::Hash;

// ─── Configuration ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Address to listen on (e.g., "0.0.0.0:9944").
    pub listen_addr: String,
    /// List of bootstrap peer addresses (e.g., ["ws://192.168.1.10:9944"]).
    pub bootstrap_peers: Vec<String>,
    /// Maximum number of peer connections.
    pub max_peers: usize,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:9944".into(),
            bootstrap_peers: Vec::new(),
            max_peers: 50,
        }
    }
}

// ─── Network Messages ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// A consensus-layer message to forward.
    Consensus(ConsensusMessage),
    /// Request a peer's current chain height.
    PingHeight,
    /// Response with current height.
    PongHeight(u64),
    /// Announce a new transaction hash (gossip protocol).
    AnnounceTx(Hash),
}

// ─── Peer connection state ──────────────────────────────────────────────────

#[derive(Debug)]
struct PeerInfo {
    _address: String,
}

// ─── Network Node ───────────────────────────────────────────────────────────

/// The P2P networking layer that connects validators and relays consensus messages.
pub struct NetworkNode {
    config: NetworkConfig,
    /// Active peer connections tracked by address.
    peers: Arc<DashMap<String, PeerInfo>>,
    /// Channel to forward incoming consensus messages to the consensus engine.
    to_consensus: mpsc::Sender<ConsensusMessage>,
    /// Channel to receive outbound messages from consensus engine.
    from_consensus: Arc<RwLock<Option<mpsc::Receiver<ConsensusMessage>>>>,
    /// Broadcast channel for outbound messages to all peers.
    outbound_tx: broadcast::Sender<String>,
    /// Set of seen message hashes for deduplication.
    seen: Arc<DashMap<Hash, ()>>,
}

impl NetworkNode {
    pub fn new(
        config: NetworkConfig,
        to_consensus: mpsc::Sender<ConsensusMessage>,
        from_consensus: mpsc::Receiver<ConsensusMessage>,
    ) -> Self {
        let (outbound_tx, _) = broadcast::channel(4096);
        Self {
            config,
            peers: Arc::new(DashMap::new()),
            to_consensus,
            from_consensus: Arc::new(RwLock::new(Some(from_consensus))),
            outbound_tx,
            seen: Arc::new(DashMap::new()),
        }
    }

    /// Run the networking layer: listen for connections and connect to bootstrap peers.
    pub async fn run(self: Arc<Self>, mut shutdown: broadcast::Receiver<()>) {
        let listener = match TcpListener::bind(&self.config.listen_addr).await {
            Ok(l) => {
                info!(addr = %self.config.listen_addr, "P2P listener started");
                l
            }
            Err(e) => {
                error!(err = %e, addr = %self.config.listen_addr, "failed to bind P2P listener");
                return;
            }
        };

        // Connect to bootstrap peers.
        for addr in &self.config.bootstrap_peers {
            let self_clone = Arc::clone(&self);
            let addr = addr.clone();
            tokio::spawn(async move {
                self_clone.connect_to_peer(&addr).await;
            });
        }

        // Start forwarding outbound consensus messages.
        {
            let self_clone = Arc::clone(&self);
            tokio::spawn(async move {
                self_clone.forward_outbound().await;
            });
        }

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            if self.peers.len() >= self.config.max_peers {
                                debug!("max peers reached, rejecting {}", addr);
                                continue;
                            }
                            let self_clone = Arc::clone(&self);
                            let addr_str = addr.to_string();
                            tokio::spawn(async move {
                                self_clone.handle_inbound(stream, addr_str).await;
                            });
                        }
                        Err(e) => {
                            warn!(err = %e, "failed to accept connection");
                        }
                    }
                }
                _ = shutdown.recv() => {
                    info!("network layer shutting down");
                    break;
                }
            }
        }
    }

    /// Handle an inbound peer connection.
    async fn handle_inbound(self: Arc<Self>, stream: TcpStream, addr: String) {
        let ws = match accept_async(stream).await {
            Ok(ws) => ws,
            Err(e) => {
                warn!(err = %e, peer = %addr, "WebSocket handshake failed");
                return;
            }
        };

        info!(peer = %addr, "inbound peer connected");
        self.peers.insert(addr.clone(), PeerInfo { _address: addr.clone() });

        let (mut write, mut read) = ws.split();

        // Subscribe to outbound broadcasts.
        let mut outbound_rx = self.outbound_tx.subscribe();

        // Bidirectional relay.
        loop {
            tokio::select! {
                // Inbound: peer → us
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            self.handle_network_message(&text).await;
                        }
                        Some(Ok(Message::Binary(data))) => {
                            if let Ok(text) = String::from_utf8(data.to_vec()) {
                                self.handle_network_message(&text).await;
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            info!(peer = %addr, "peer disconnected");
                            break;
                        }
                        Some(Err(e)) => {
                            warn!(peer = %addr, err = %e, "peer error");
                            break;
                        }
                        _ => {}
                    }
                }
                // Outbound: us → peer
                Ok(text) = outbound_rx.recv() => {
                    if write.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
            }
        }

        self.peers.remove(&addr);
    }

    /// Connect to a peer address.
    async fn connect_to_peer(self: Arc<Self>, addr: &str) {
        let ws_url = if addr.starts_with("ws://") || addr.starts_with("wss://") {
            addr.to_string()
        } else {
            format!("ws://{}", addr)
        };

        match connect_async(&ws_url).await {
            Ok((ws, _)) => {
                info!(peer = %addr, "connected to peer");
                self.peers.insert(addr.to_string(), PeerInfo { _address: addr.to_string() });

                let (mut write, mut read) = ws.split();
                let mut outbound_rx = self.outbound_tx.subscribe();

                loop {
                    tokio::select! {
                        msg = read.next() => {
                            match msg {
                                Some(Ok(Message::Text(text))) => {
                                    self.handle_network_message(&text).await;
                                }
                                Some(Ok(Message::Binary(data))) => {
                                    if let Ok(text) = String::from_utf8(data.to_vec()) {
                                        self.handle_network_message(&text).await;
                                    }
                                }
                                Some(Ok(Message::Close(_))) | None => {
                                    info!(peer = %addr, "peer disconnected");
                                    break;
                                }
                                Some(Err(e)) => {
                                    warn!(peer = %addr, err = %e, "peer error");
                                    break;
                                }
                                _ => {}
                            }
                        }
                        Ok(text) = outbound_rx.recv() => {
                            if write.send(Message::Text(text.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                }

                self.peers.remove(addr);
            }
            Err(e) => {
                warn!(peer = %addr, err = %e, "failed to connect to peer");
            }
        }
    }

    /// Process a received network message.
    async fn handle_network_message(&self, raw: &str) {
        let msg: NetworkMessage = match serde_json::from_str(raw) {
            Ok(m) => m,
            Err(e) => {
                debug!(err = %e, "received unparseable message");
                return;
            }
        };

        // Dedup by message hash.
        let msg_hash = Hash::digest(raw.as_bytes());
        if self.seen.contains_key(&msg_hash) {
            return;
        }
        self.seen.insert(msg_hash, ());

        // Periodically trim the seen set to prevent unbounded growth.
        if self.seen.len() > 100_000 {
            let to_remove: Vec<Hash> = self.seen.iter().take(50_000).map(|r| *r.key()).collect();
            for h in to_remove {
                self.seen.remove(&h);
            }
        }

        match msg {
            NetworkMessage::Consensus(consensus_msg) => {
                if self.to_consensus.send(consensus_msg).await.is_err() {
                    warn!("consensus channel closed");
                }
            }
            NetworkMessage::PingHeight => {
                // Respond with our height (the caller handles sending).
                debug!("received height ping");
            }
            NetworkMessage::PongHeight(h) => {
                debug!(height = h, "received height pong");
            }
            NetworkMessage::AnnounceTx(tx_hash) => {
                debug!(tx = %tx_hash, "received tx announcement");
            }
        }

        // Re-broadcast to other peers (gossip).
        let _ = self.outbound_tx.send(raw.to_string());
    }

    /// Forward outbound consensus messages to all peers.
    async fn forward_outbound(self: Arc<Self>) {
        let mut rx = self.from_consensus.write().take()
            .expect("from_consensus already taken");

        while let Some(msg) = rx.recv().await {
            let network_msg = NetworkMessage::Consensus(msg);
            match serde_json::to_string(&network_msg) {
                Ok(text) => {
                    let _ = self.outbound_tx.send(text);
                }
                Err(e) => {
                    error!(err = %e, "failed to serialize outbound message");
                }
            }
        }
    }

    /// Get the number of connected peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Get list of connected peer addresses.
    pub fn peer_addresses(&self) -> Vec<String> {
        self.peers.iter().map(|r| r.key().clone()).collect()
    }
}
