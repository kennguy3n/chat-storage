//! `local_store` module — encrypted on-device storage surface.
//!
//! Submodules:
//!
//! * [`schema`] — the SQLCipher CREATE TABLE statements
//!   (`SCHEMA_SQL`) plus the typed Rust row structs that mirror them
//!   1:1 (`Conversation`, `MessageSkeleton`, `MessageBody`,
//!   `MediaAsset`, `BackupEventJournalEntry`, `ArchiveSegmentMapEntry`,
//!   `RestoreStateEntry`).
//! * [`state_machines`] — the `body_state` / `media_state` /
//!   `archive_state` / `backup_state` / `restore_state` enums with
//!   `try_transition`, `Display` / `FromStr`, and serde support.
//! * [`db`] — the `rusqlite::Connection` bindings, prepared-statement
//!   cache, migrations, and SQLCipher key plumbing.

pub mod db;
pub mod schema;
pub mod state_machines;

pub use db::LocalStoreDb;
pub use schema::*;
pub use state_machines::*;

/// Storage-layer error type wrapped by [`crate::Error::Storage`].
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// A SQLCipher / rusqlite call failed (driver error, statement
    /// prep error, prepared-statement type mismatch, …).
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// The SQLCipher driver returned `SQLITE_BUSY` / `SQLITE_LOCKED`.
    #[error("database is locked")]
    DatabaseLocked,

    /// A schema migration failed mid-flight.
    #[error("migration failed from v{from} to v{to}: {detail}")]
    MigrationFailed { from: u32, to: u32, detail: String },

    /// The SQLCipher driver reported the underlying volume is full.
    #[error("disk full")]
    DiskFull,

    /// A row decoded from a table failed an invariant check.
    #[error("corrupt row in `{table}`: {detail}")]
    CorruptRow { table: &'static str, detail: String },

    /// A non-SQL I/O failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// A CBOR encode of a persisted / wire-format payload failed.
    #[error("cbor encode ({context}): {source}")]
    CborEncode {
        context: &'static str,
        source: ciborium::ser::Error<std::io::Error>,
    },

    /// A CBOR decode of a persisted / received wire-format payload
    /// failed.
    #[error("cbor decode ({context}): {source}")]
    CborDecode {
        context: &'static str,
        source: ciborium::de::Error<std::io::Error>,
    },

    /// A `zstd` compress / decompress call failed.
    #[error("zstd ({context}): {source}")]
    Zstd {
        context: &'static str,
        source: std::io::Error,
    },

    /// A string-formatted UUID could not be parsed back into a
    /// [`uuid::Uuid`].
    #[error("invalid {kind}: {source}")]
    InvalidId {
        kind: &'static str,
        source: uuid::Error,
    },

    /// A subsystem that callers expected to be installed at boot
    /// was not.
    #[error("subsystem `{0}` not installed")]
    SubsystemNotInstalled(&'static str),

    /// A subsystem that is installed exactly once at boot was
    /// installed twice.
    #[error("subsystem `{0}` already installed (install is write-once)")]
    SubsystemAlreadyInstalled(&'static str),

    /// A `Mutex` / `RwLock` was poisoned by a panicking thread.
    #[error("`{0}` lock poisoned")]
    LockPoisoned(&'static str),

    /// The device is currently offline.
    #[error("offline")]
    Offline,

    /// Free-form fallback for sites where the underlying error type
    /// does not (yet) merit a dedicated variant.
    #[error("{0}")]
    Custom(String),
}

impl StorageError {
    /// Construct a [`StorageError::Custom`] from anything that can be
    /// converted into a [`String`].
    pub fn msg(msg: impl Into<String>) -> Self {
        StorageError::Custom(msg.into())
    }
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

/// Promote a [`rusqlite::Error`] into a typed [`StorageError`] by
/// inspecting the extended error code.
pub fn classify_rusqlite(error: rusqlite::Error) -> StorageError {
    use rusqlite::ErrorCode;
    if let rusqlite::Error::SqliteFailure(code, _) = &error {
        match code.code {
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => {
                return StorageError::DatabaseLocked
            }
            ErrorCode::DiskFull => return StorageError::DiskFull,
            _ => {}
        }
    }
    StorageError::Sqlite(error)
}
