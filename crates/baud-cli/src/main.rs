use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::json;
use tracing_subscriber::EnvFilter;

use baud_core::crypto::{Address, Hash, KeyPair, Signature};
use baud_core::types::{
    GenesisAllocation, GenesisConfig, Transaction, TxPayload, ValidatorInfo, QUANTA_PER_BAUD,
};
use baud_wallet::EncryptedWallet;

/// Baud — CLI for the M2M Agent Ledger
#[derive(Parser)]
#[command(
    name = "baud",
    version,
    about = "Headless CLI for the Baud M2M agent ledger"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a new Ed25519 keypair for an agent identity.
    Keygen,

    /// Display the address for a given secret key.
    Address {
        /// Hex-encoded 32-byte secret key (or set BAUD_SECRET_KEY env var).
        #[arg(long, env = "BAUD_SECRET_KEY")]
        secret: String,
    },

    /// Create a signed transfer transaction and print it as JSON.
    Transfer {
        /// Hex-encoded sender secret key (or set BAUD_SECRET_KEY env var).
        #[arg(long, env = "BAUD_SECRET_KEY")]
        secret: String,
        /// Hex-encoded recipient address.
        #[arg(long)]
        to: String,
        /// Amount in quanta (smallest unit).
        #[arg(long)]
        amount: u128,
        /// Sender nonce.
        #[arg(long)]
        nonce: u64,
        /// Optional text memo.
        #[arg(long)]
        memo: Option<String>,
        /// Chain identifier (must match the target chain).
        #[arg(long, default_value = "baud-mainnet")]
        chain_id: String,
    },

    /// Create a signed escrow-create transaction and print it as JSON.
    EscrowCreate {
        /// Hex-encoded sender secret key (or set BAUD_SECRET_KEY env var).
        #[arg(long, env = "BAUD_SECRET_KEY")]
        secret: String,
        /// Hex-encoded recipient address.
        #[arg(long)]
        recipient: String,
        /// Amount in quanta.
        #[arg(long)]
        amount: u128,
        /// The secret preimage (plaintext). A BLAKE3 hash-lock is derived from this.
        #[arg(long)]
        preimage: String,
        /// Deadline as Unix milliseconds.
        #[arg(long)]
        deadline: u64,
        /// Sender nonce.
        #[arg(long)]
        nonce: u64,
        /// Chain identifier (must match the target chain).
        #[arg(long, default_value = "baud-mainnet")]
        chain_id: String,
    },

    /// Create a signed escrow-release transaction and print it as JSON.
    EscrowRelease {
        /// Hex-encoded sender (recipient of escrow) secret key (or set BAUD_SECRET_KEY env var).
        #[arg(long, env = "BAUD_SECRET_KEY")]
        secret: String,
        /// Hex-encoded escrow ID.
        #[arg(long)]
        escrow_id: String,
        /// The secret preimage (plaintext).
        #[arg(long)]
        preimage: String,
        /// Sender nonce.
        #[arg(long)]
        nonce: u64,
        /// Chain identifier (must match the target chain).
        #[arg(long, default_value = "baud-mainnet")]
        chain_id: String,
    },

    /// Create a signed escrow-refund transaction and print it as JSON.
    EscrowRefund {
        /// Hex-encoded sender (original escrow creator) secret key (or set BAUD_SECRET_KEY env var).
        #[arg(long, env = "BAUD_SECRET_KEY")]
        secret: String,
        /// Hex-encoded escrow ID.
        #[arg(long)]
        escrow_id: String,
        /// Sender nonce.
        #[arg(long)]
        nonce: u64,
        /// Chain identifier (must match the target chain).
        #[arg(long, default_value = "baud-mainnet")]
        chain_id: String,
    },

    /// Create a signed agent-register transaction and print it as JSON.
    AgentRegister {
        /// Hex-encoded sender secret key (or set BAUD_SECRET_KEY env var).
        #[arg(long, env = "BAUD_SECRET_KEY")]
        secret: String,
        /// Agent name.
        #[arg(long)]
        name: String,
        /// Service endpoint URL.
        #[arg(long)]
        endpoint: String,
        /// Comma-separated capability tags.
        #[arg(long)]
        capabilities: String,
        /// Sender nonce.
        #[arg(long)]
        nonce: u64,
        /// Chain identifier (must match the target chain).
        #[arg(long, default_value = "baud-mainnet")]
        chain_id: String,
    },

    /// Generate a genesis configuration file.
    Genesis {
        /// Chain ID.
        #[arg(long, default_value = "baud-mainnet")]
        chain_id: String,
        /// Comma-separated list of secret keys for initial validators.
        #[arg(long)]
        validators: String,
        /// Initial balance per validator in BAUD (will be converted to quanta).
        #[arg(long, default_value = "1000000")]
        initial_balance: u64,
        /// Output file path.
        #[arg(long, default_value = "genesis.json")]
        output: String,
    },

    /// Compute the BLAKE3 hash of arbitrary input (useful for escrow hash-locks).
    HashData {
        /// The data to hash.
        #[arg(long)]
        data: String,
    },

    /// Submit a signed transaction JSON to a node's API.
    Submit {
        /// Node API URL (e.g., http://localhost:8080).
        #[arg(long, default_value = "http://localhost:8080")]
        node: String,
        /// Path to the signed transaction JSON file, or "-" for stdin.
        #[arg(long)]
        tx_file: String,
    },

    /// Query account balance from a node.
    Balance {
        /// Node API URL.
        #[arg(long, default_value = "http://localhost:8080")]
        node: String,
        /// Hex-encoded address.
        #[arg(long)]
        address: String,
    },

    /// Query node status.
    Status {
        /// Node API URL.
        #[arg(long, default_value = "http://localhost:8080")]
        node: String,
    },

    /// Create a new encrypted wallet file with a fresh keypair.
    WalletCreate {
        /// Path to the wallet file.
        #[arg(long, default_value = "wallet.json")]
        wallet: String,
        /// Password for encryption.
        #[arg(long)]
        password: String,
        /// Label for the initial key.
        #[arg(long, default_value = "default")]
        label: String,
    },

    /// Import an existing secret key into an encrypted wallet.
    WalletImport {
        /// Path to the wallet file.
        #[arg(long, default_value = "wallet.json")]
        wallet: String,
        /// Password for encryption.
        #[arg(long)]
        password: String,
        /// Label for the key.
        #[arg(long)]
        label: String,
        /// Hex-encoded secret key to import.
        #[arg(long)]
        secret: String,
    },

    /// List all keys in an encrypted wallet (addresses only).
    WalletList {
        /// Path to the wallet file.
        #[arg(long, default_value = "wallet.json")]
        wallet: String,
        /// Password for decryption.
        #[arg(long)]
        password: String,
    },

    /// Export the secret key for a specific label from the wallet.
    WalletExport {
        /// Path to the wallet file.
        #[arg(long, default_value = "wallet.json")]
        wallet: String,
        /// Password for decryption.
        #[arg(long)]
        password: String,
        /// Label of the key to export.
        #[arg(long)]
        label: String,
    },

    /// Open the web dashboard in your browser.
    Dashboard {
        /// Node API URL.
        #[arg(long, default_value = "http://localhost:8080")]
        node: String,
    },
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn sign_tx(kp: &KeyPair, tx: &mut Transaction) {
    let hash = tx.signable_hash();
    tx.signature = kp.sign(hash.as_bytes());
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Keygen => {
            let kp = KeyPair::generate();
            let output = json!({
                "address": kp.address().to_hex(),
                "secret_key": kp.secret_hex(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::Address { secret } => {
            let kp = KeyPair::from_secret_hex(&secret).context("invalid secret key")?;
            println!("{}", kp.address().to_hex());
        }

        Commands::Transfer {
            secret,
            to,
            amount,
            nonce,
            memo,
            chain_id,
        } => {
            let kp = KeyPair::from_secret_hex(&secret).context("invalid secret key")?;
            let to_addr = Address::from_hex(&to).context("invalid recipient address")?;
            let memo_clone = memo.clone();

            let mut tx = Transaction {
                sender: kp.address(),
                nonce,
                payload: TxPayload::Transfer {
                    to: to_addr,
                    amount,
                    memo: memo.map(|m| m.into_bytes()),
                },
                timestamp: now_ms(),
                chain_id,
                signature: Signature::zero(),
            };
            sign_tx(&kp, &mut tx);

            let output = json!({
                "sender": tx.sender.to_hex(),
                "nonce": tx.nonce,
                "payload": {
                    "type": "Transfer",
                    "to": to,
                    "amount": amount,
                    "memo": memo_clone,
                },
                "timestamp": tx.timestamp,
                "signature": tx.signature.to_hex(),
                "tx_hash": tx.hash().to_hex(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::EscrowCreate {
            secret,
            recipient,
            amount,
            preimage,
            deadline,
            nonce,
            chain_id,
        } => {
            let kp = KeyPair::from_secret_hex(&secret).context("invalid secret key")?;
            let recipient_addr =
                Address::from_hex(&recipient).context("invalid recipient address")?;
            let hash_lock = Hash::digest(preimage.as_bytes());

            let mut tx = Transaction {
                sender: kp.address(),
                nonce,
                payload: TxPayload::EscrowCreate {
                    recipient: recipient_addr,
                    amount,
                    hash_lock,
                    deadline,
                },
                timestamp: now_ms(),
                chain_id,
                signature: Signature::zero(),
            };
            sign_tx(&kp, &mut tx);

            let output = json!({
                "sender": tx.sender.to_hex(),
                "nonce": tx.nonce,
                "payload": {
                    "type": "EscrowCreate",
                    "recipient": recipient,
                    "amount": amount,
                    "hash_lock": hash_lock.to_hex(),
                    "deadline": deadline,
                },
                "timestamp": tx.timestamp,
                "signature": tx.signature.to_hex(),
                "tx_hash": tx.hash().to_hex(),
                "escrow_id": tx.hash().to_hex(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::EscrowRelease {
            secret,
            escrow_id,
            preimage,
            nonce,
            chain_id,
        } => {
            let kp = KeyPair::from_secret_hex(&secret).context("invalid secret key")?;
            let eid = Hash::from_hex(&escrow_id).context("invalid escrow ID")?;

            let mut tx = Transaction {
                sender: kp.address(),
                nonce,
                payload: TxPayload::EscrowRelease {
                    escrow_id: eid,
                    preimage: preimage.into_bytes(),
                },
                timestamp: now_ms(),
                chain_id,
                signature: Signature::zero(),
            };
            sign_tx(&kp, &mut tx);

            let output = json!({
                "sender": tx.sender.to_hex(),
                "nonce": tx.nonce,
                "payload": {
                    "type": "EscrowRelease",
                    "escrow_id": escrow_id,
                },
                "timestamp": tx.timestamp,
                "signature": tx.signature.to_hex(),
                "tx_hash": tx.hash().to_hex(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::EscrowRefund {
            secret,
            escrow_id,
            nonce,
            chain_id,
        } => {
            let kp = KeyPair::from_secret_hex(&secret).context("invalid secret key")?;
            let eid = Hash::from_hex(&escrow_id).context("invalid escrow ID")?;

            let mut tx = Transaction {
                sender: kp.address(),
                nonce,
                payload: TxPayload::EscrowRefund { escrow_id: eid },
                timestamp: now_ms(),
                chain_id,
                signature: Signature::zero(),
            };
            sign_tx(&kp, &mut tx);

            let output = json!({
                "sender": tx.sender.to_hex(),
                "nonce": tx.nonce,
                "payload": {
                    "type": "EscrowRefund",
                    "escrow_id": escrow_id,
                },
                "timestamp": tx.timestamp,
                "signature": tx.signature.to_hex(),
                "tx_hash": tx.hash().to_hex(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::AgentRegister {
            secret,
            name,
            endpoint,
            capabilities,
            nonce,
            chain_id,
        } => {
            let kp = KeyPair::from_secret_hex(&secret).context("invalid secret key")?;
            let caps: Vec<Vec<u8>> = capabilities
                .split(',')
                .map(|c| c.trim().as_bytes().to_vec())
                .collect();

            let mut tx = Transaction {
                sender: kp.address(),
                nonce,
                payload: TxPayload::AgentRegister {
                    name: name.as_bytes().to_vec(),
                    endpoint: endpoint.as_bytes().to_vec(),
                    capabilities: caps,
                },
                timestamp: now_ms(),
                chain_id,
                signature: Signature::zero(),
            };
            sign_tx(&kp, &mut tx);

            let output = json!({
                "sender": tx.sender.to_hex(),
                "nonce": tx.nonce,
                "payload": {
                    "type": "AgentRegister",
                    "name": name,
                    "endpoint": endpoint,
                    "capabilities": capabilities,
                },
                "timestamp": tx.timestamp,
                "signature": tx.signature.to_hex(),
                "tx_hash": tx.hash().to_hex(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::Genesis {
            chain_id,
            validators,
            initial_balance,
            output,
        } => {
            let secrets: Vec<&str> = validators.split(',').map(|s| s.trim()).collect();
            let mut allocs = Vec::new();
            let mut vals = Vec::new();

            for (i, secret) in secrets.iter().enumerate() {
                let kp = KeyPair::from_secret_hex(secret)
                    .context(format!("invalid secret key at index {i}"))?;
                let balance = (initial_balance as u128)
                    .checked_mul(QUANTA_PER_BAUD)
                    .context("balance overflow")?;
                allocs.push(GenesisAllocation {
                    address: kp.address(),
                    balance,
                });
                vals.push(ValidatorInfo {
                    address: kp.address(),
                    name: format!("validator-{i}"),
                });
            }

            let genesis = GenesisConfig {
                chain_id,
                allocations: allocs,
                validators: vals,
                timestamp: now_ms(),
            };

            let json_str = serde_json::to_string_pretty(&genesis)?;
            std::fs::write(&output, &json_str)
                .context(format!("failed to write genesis to {output}"))?;
            println!("Genesis config written to {output}");
            println!("{json_str}");
        }

        Commands::HashData { data } => {
            let hash = Hash::digest(data.as_bytes());
            println!("{}", hash.to_hex());
        }

        Commands::Submit { node, tx_file } => {
            let tx_json = if tx_file == "-" {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            } else {
                std::fs::read_to_string(&tx_file).context(format!("failed to read {tx_file}"))?
            };

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let client = reqwest::Client::new();
                let url = format!("{node}/v1/tx");
                let resp = client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .body(tx_json)
                    .send()
                    .await
                    .context("failed to send request")?;

                let status = resp.status();
                let body = resp.text().await.context("failed to read response")?;
                println!("HTTP {status}");
                println!("{body}");
                Ok::<(), anyhow::Error>(())
            })?;
        }

        Commands::Balance { node, address } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let url = format!("{node}/v1/account/{address}");
                let resp = reqwest::get(&url).await.context("request failed")?;
                let body = resp.text().await?;
                println!("{body}");
                Ok::<(), anyhow::Error>(())
            })?;
        }

        Commands::Status { node } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let url = format!("{node}/v1/status");
                let resp = reqwest::get(&url).await.context("request failed")?;
                let body = resp.text().await?;
                println!("{body}");
                Ok::<(), anyhow::Error>(())
            })?;
        }

        Commands::WalletCreate {
            wallet,
            password,
            label,
        } => {
            let w = EncryptedWallet::at(&wallet);
            let entry = w.create(&password, &label)?;
            let output = json!({
                "created": wallet,
                "label": entry.label,
                "address": entry.address,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::WalletImport {
            wallet,
            password,
            label,
            secret,
        } => {
            let w = EncryptedWallet::at(&wallet);
            let entry = w.import_key(&password, &label, &secret)?;
            let output = json!({
                "imported": label,
                "address": entry.address,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::WalletList { wallet, password } => {
            let w = EncryptedWallet::at(&wallet);
            let entries = w.list(&password)?;
            let output: Vec<_> = entries
                .iter()
                .map(|e| {
                    json!({
                        "label": e.label,
                        "address": e.address,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::WalletExport {
            wallet,
            password,
            label,
        } => {
            let w = EncryptedWallet::at(&wallet);
            let entry = w.export(&password, &label)?;
            let output = json!({
                "label": entry.label,
                "address": entry.address,
                "secret_key": entry.secret_key,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::Dashboard { node } => {
            println!("Opening Baud dashboard at {node}");
            #[cfg(target_os = "windows")]
            { let _ = std::process::Command::new("cmd").args(["/C", "start", &node]).spawn(); }
            #[cfg(target_os = "macos")]
            { let _ = std::process::Command::new("open").arg(&node).spawn(); }
            #[cfg(target_os = "linux")]
            { let _ = std::process::Command::new("xdg-open").arg(&node).spawn(); }
        }
    }

    Ok(())
}
