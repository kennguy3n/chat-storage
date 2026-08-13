//! cs-core — platform-agnostic core for KChat chat-storage.
//!
//! Built on top of `kdrive-rust-sdk` for KDRV1 crypto, transport, and
//! key management. Provides E2EE local storage, multilingual search,
//! personal archive, backup, storage offload, rehydration, and
//! knowledge/threat detection.
//!
//! ## Architecture
//!
//! The crypto foundation is KDRV1 from `kchat-drive-crypto`. Purpose-
//! specific keys (archive, backup, search) are derived from the Drive's
//! `DomainKey` (or `ShareGrantKey` in Max mode) via HKDF-SHA256.
//!
//! The backend is the `kdrive` Go gateway, extended with archive,
//! search shard, backup, and message delivery endpoints.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod archive;
pub mod backup;
pub mod config;
pub mod core_impl;
pub mod crypto;
pub mod formats;
pub mod knowledge;
pub mod local_store;
pub mod media;
pub mod message;
pub mod models;
pub mod offload;
pub mod perf;
pub mod restore;
pub mod scheduler;
pub mod search;
pub mod security;
pub mod tenant;
pub mod transport;
pub mod util;

pub use config::ChatStorageConfig;
pub use core_impl::CoreImpl;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Top-level error type for the core library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("crypto: {0}")]
    Crypto(#[from] crypto::CryptoError),

    #[error("storage: {0}")]
    Storage(#[from] local_store::StorageError),

    #[error("search: {0}")]
    Search(#[from] search::SearchError),

    #[error("message: {0}")]
    Message(#[from] message::MessageError),

    #[error("transport: {0}")]
    Transport(#[from] transport::TransportError),

    #[error("model: {0}")]
    Model(#[from] models::ModelError),

    #[error("tenant: {0}")]
    Tenant(#[from] tenant::TenantError),

    #[error("quota exceeded: {resource} (limit {limit}, current {current})")]
    QuotaExceeded {
        resource: &'static str,
        limit: u64,
        current: u64,
    },

    #[error("not yet implemented: {0}")]
    NotImplemented(&'static str),
}

/// Crate-wide [`Result`] alias.
pub type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Public API types (used by the ChatStorageCore trait in core_impl)
// ---------------------------------------------------------------------------

/// Unique client-generated message identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClientMessageId(pub Uuid);

/// Opaque delivery cursor for cursor-paginated message fetch.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeliveryCursor(pub String);

/// Result of ingesting remote messages.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IngestResult {
    pub new_count: usize,
    pub updated_count: usize,
    pub duplicate_count: usize,
    pub next_cursor: Option<DeliveryCursor>,
}

/// Device registration result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRegistration {
    pub device_id: String,
    pub registered: bool,
}

/// Search query with optional filters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub sender_id: Option<String>,
    pub conversation_id: Option<Uuid>,
    pub date_from_ms: Option<i64>,
    pub date_to_ms: Option<i64>,
    pub content_kind: Option<ContentKind>,
}

/// Content kind filter for search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentKind {
    Text,
    Media,
    Document,
    Link,
}

/// Search scope controls whether to include cold (offloaded) buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchScope {
    LocalOnly,
    IncludeCold,
}

/// A single search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub message_id: Uuid,
    pub conversation_id: Uuid,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub snippet: String,
    pub score: f64,
    pub from_cold: bool,
}

/// Reason for hydrating a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HydrationReason {
    UserTap,
    ScrollBack,
    SearchHit,
}

/// A hydrated message with full body and media refs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydratedMessage {
    pub message_id: Uuid,
    pub conversation_id: Uuid,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub text_content: Option<String>,
    pub media_assets: Vec<MediaAssetRef>,
}

/// Reference to a media asset in a hydrated message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAssetRef {
    pub asset_id: String,
    pub mime_type: String,
    pub node_id: String,
    pub version_id: String,
}

/// Reason for running an incremental backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackupReason {
    Scheduled,
    UserInitiated,
    PreMigration,
}

/// Result of a backup run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupResult {
    pub segments_built: usize,
    pub segments_uploaded: usize,
    pub manifest_generation: u64,
    pub bytes_uploaded: u64,
}

/// Backup source for restore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupSource {
    KdriveGateway,
    ICloud,
    AndroidBackup,
    ZkObjectFabric { bucket: String, prefix: String },
}

/// Result of a restore operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RestoreResult {
    pub conversations_restored: usize,
    pub messages_restored: usize,
    pub media_restored: usize,
    pub search_indexes_rebuilt: usize,
}

/// Reason for storage pressure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoragePressureReason {
    BudgetThreshold,
    DiskLow,
    UserInitiated,
}

/// Result of an offload operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OffloadResult {
    pub messages_offloaded: usize,
    pub media_offloaded: usize,
    pub bytes_freed: u64,
}

/// Result of sending media.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMediaResult {
    pub message_id: Uuid,
    pub asset_id: String,
    pub node_id: String,
    pub version_id: String,
}
