//! Transport layer — bridges to kdrive gateway HTTP API.
//!
//! `ChatStorageTransport` is the unified trait that `CoreImpl` depends on.
//! It combines message delivery, kdrive drive operations, dedup,
//! archive/search/backup endpoints.

pub mod auto_transport;
pub mod circuit_breaker;
pub mod dedup_bridge;
pub mod delivery;
pub mod kdrive_bridge;
pub mod offline;
pub mod request_coalescer;

pub use delivery::{DeliveryClient, FetchResult, RawDeliveryMessage};

/// Transport-layer errors.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("network: {0}")]
    Network(String),

    #[error("auth: {0}")]
    Auth(String),

    #[error("server: {0}")]
    Server(String),

    #[error("kdrive: {0}")]
    Kdrive(String),

    #[error("{0}")]
    Custom(String),
}

impl From<String> for TransportError {
    fn from(s: String) -> Self {
        TransportError::Custom(s)
    }
}

impl From<kchat_drive_types::DriveError> for TransportError {
    fn from(e: kchat_drive_types::DriveError) -> Self {
        TransportError::Kdrive(e.to_string())
    }
}

/// The unified transport trait for chat-storage.
///
/// This combines:
/// - `DeliveryClient` (message fetch)
/// - Archive segment upload/download
/// - Search shard upload/download
/// - Backup manifest upload/download
///
/// kdrive drive operations (Transport, DedupTransport) are handled
/// separately by the `DriveFacade`.
pub trait ChatStorageTransport: Send + Sync {
    /// Fetch the next page of messages for a conversation.
    fn fetch_messages(
        &self,
        conversation_id: &str,
        after_cursor: Option<&str>,
    ) -> Result<FetchResult, TransportError>;

    /// Upload an archive segment.
    fn upload_archive_segment(
        &self,
        segment_id: &str,
        ciphertext: &[u8],
    ) -> Result<String, TransportError>;

    /// Download an archive segment.
    fn download_archive_segment(&self, segment_id: &str) -> Result<Vec<u8>, TransportError>;

    /// Fetch archive manifests after a generation.
    fn fetch_archive_manifests(
        &self,
        after_generation: u64,
    ) -> Result<Vec<Vec<u8>>, TransportError>;

    /// Upload an archive manifest.
    fn upload_archive_manifest(&self, manifest: &[u8]) -> Result<(), TransportError>;

    /// Upload a search index shard.
    fn upload_search_shard(&self, shard_key: &str, ciphertext: &[u8])
        -> Result<(), TransportError>;

    /// Download a search index shard.
    fn download_search_shard(&self, shard_key: &str) -> Result<Vec<u8>, TransportError>;

    /// Upload a backup manifest.
    fn upload_backup_manifest(&self, manifest: &[u8]) -> Result<(), TransportError>;

    /// Fetch backup manifests after a generation.
    fn fetch_backup_manifests(&self, after_generation: u64)
        -> Result<Vec<Vec<u8>>, TransportError>;
}
