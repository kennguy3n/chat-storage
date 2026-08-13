//! Local store — SQLCipher encrypted on-device database.

pub mod db;
pub mod schema;
pub mod state_machines;

pub use db::LocalStoreDb;
pub use schema::*;
pub use state_machines::*;

/// Storage-layer errors.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database: {0}")]
    Database(String),

    #[error("database locked")]
    DatabaseLocked,

    #[error("schema migration: {0}")]
    Migration(String),

    #[error("CBOR encode/decode: {0}")]
    Cbor(String),

    #[error("lock poisoned")]
    LockPoisoned,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid state transition: {0}")]
    InvalidStateTransition(String),

    #[error("install error: {0}")]
    Install(String),

    #[error("{0}")]
    Custom(String),
}

impl From<String> for StorageError {
    fn from(s: String) -> Self {
        StorageError::Custom(s)
    }
}

impl From<&str> for StorageError {
    fn from(s: &str) -> Self {
        StorageError::Custom(s.to_string())
    }
}
