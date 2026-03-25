use std::path::Path;

use tracing::{debug, info};

use baud_core::state::WorldState;
use baud_core::types::ExtendedState;

/// Key for persisted world state.
const STATE_KEY: &[u8] = b"world_state";
/// Key for block height metadata.
const META_KEY_HEIGHT: &[u8] = b"meta:height";
/// Key for extended state (new features).
const EXTENDED_STATE_KEY: &[u8] = b"extended_state";

/// Persistent storage for Baud world state using sled (pure-Rust embedded DB).
pub struct BaudStore {
    db: sled::Db,
}

impl BaudStore {
    /// Open or create a sled database at the given path.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let db = sled::open(path).map_err(|e| StoreError::Storage(e.to_string()))?;
        info!(path = %path.display(), "storage opened");
        Ok(Self { db })
    }

    /// Save the full world state to disk.
    pub fn save_state(&self, state: &WorldState) -> Result<(), StoreError> {
        let encoded =
            bincode::serialize(state).map_err(|e| StoreError::Serialization(e.to_string()))?;

        self.db
            .insert(STATE_KEY, encoded.as_slice())
            .map_err(|e| StoreError::Storage(e.to_string()))?;

        self.db
            .insert(META_KEY_HEIGHT, &state.height.to_le_bytes())
            .map_err(|e| StoreError::Storage(e.to_string()))?;

        self.db
            .flush()
            .map_err(|e| StoreError::Storage(e.to_string()))?;

        debug!(
            height = state.height,
            bytes = encoded.len(),
            "state persisted"
        );
        Ok(())
    }

    /// Load the world state from disk. Returns None if no state has been saved.
    pub fn load_state(&self) -> Result<Option<WorldState>, StoreError> {
        match self
            .db
            .get(STATE_KEY)
            .map_err(|e| StoreError::Storage(e.to_string()))?
        {
            Some(bytes) => {
                let state: WorldState = bincode::deserialize(&bytes)
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                info!(
                    height = state.height,
                    accounts = state.accounts.len(),
                    "state loaded from disk"
                );
                Ok(Some(state))
            }
            None => Ok(None),
        }
    }

    /// Get the persisted block height without deserializing the full state.
    pub fn persisted_height(&self) -> Result<Option<u64>, StoreError> {
        match self
            .db
            .get(META_KEY_HEIGHT)
            .map_err(|e| StoreError::Storage(e.to_string()))?
        {
            Some(bytes) => {
                if bytes.len() == 8 {
                    let arr: [u8; 8] = bytes.as_ref().try_into().unwrap();
                    Ok(Some(u64::from_le_bytes(arr)))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Flush all pending writes to disk.
    pub fn flush(&self) -> Result<(), StoreError> {
        self.db
            .flush()
            .map_err(|e| StoreError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Save the extended state to disk.
    pub fn save_extended_state(&self, state: &ExtendedState) -> Result<(), StoreError> {
        let encoded =
            bincode::serialize(state).map_err(|e| StoreError::Serialization(e.to_string()))?;
        self.db
            .insert(EXTENDED_STATE_KEY, encoded.as_slice())
            .map_err(|e| StoreError::Storage(e.to_string()))?;
        self.db
            .flush()
            .map_err(|e| StoreError::Storage(e.to_string()))?;
        debug!(bytes = encoded.len(), "extended state persisted");
        Ok(())
    }

    /// Load the extended state from disk. Returns default if not found.
    pub fn load_extended_state(&self) -> Result<ExtendedState, StoreError> {
        match self
            .db
            .get(EXTENDED_STATE_KEY)
            .map_err(|e| StoreError::Storage(e.to_string()))?
        {
            Some(bytes) => {
                let state: ExtendedState = bincode::deserialize(&bytes)
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                info!("extended state loaded from disk");
                Ok(state)
            }
            None => {
                info!("no extended state found, using defaults");
                Ok(ExtendedState::default())
            }
        }
    }
}

/// Storage-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("storage error: {0}")]
    Storage(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}
