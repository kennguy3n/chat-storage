//! Configuration for the chat-storage core.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Wrapper for PrivacyMode that implements serde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrivacyModeSerde(pub kchat_drive_types::PrivacyMode);

impl Serialize for PrivacyModeSerde {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(self.0.as_u8())
    }
}

impl<'de> Deserialize<'de> for PrivacyModeSerde {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let val = u8::deserialize(deserializer)?;
        kchat_drive_types::PrivacyMode::from_u8(val)
            .map(PrivacyModeSerde)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid PrivacyMode value: {}", val)))
    }
}

impl From<kchat_drive_types::PrivacyMode> for PrivacyModeSerde {
    fn from(m: kchat_drive_types::PrivacyMode) -> Self {
        Self(m)
    }
}

impl Default for PrivacyModeSerde {
    fn default() -> Self {
        Self(kchat_drive_types::PrivacyMode::Secured)
    }
}

/// Main configuration for `ChatStorageCore`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatStorageConfig {
    /// Directory for the local SQLCipher database and file cache.
    pub data_dir: PathBuf,
    /// Base URL of the kdrive Go gateway.
    pub drive_gateway_url: String,
    /// KDRV1 privacy mode (Secured / Advanced / Max).
    pub privacy_mode: PrivacyModeSerde,
    /// Archive backend selection.
    pub archive_backend: ArchiveBackend,
    /// Media blob sink configuration.
    pub media_blob_sink: MediaBlobSinkConfig,
    /// Search engine configuration.
    pub search: SearchConfig,
    /// Storage budget (optional).
    pub storage_budget: Option<StorageBudgetConfig>,
    /// Tenant ID (for multi-tenant deployments).
    pub tenant_id: Option<String>,
    /// Epoch rotation cadence (default: monthly).
    pub epoch_rotation: EpochCadence,
    /// Privacy level for archive access patterns.
    pub privacy_level: PrivacyLevel,
}

impl Default for ChatStorageConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./data"),
            drive_gateway_url: "http://localhost:8080".to_string(),
            privacy_mode: PrivacyModeSerde::default(),
            archive_backend: ArchiveBackend::Kdrive,
            media_blob_sink: MediaBlobSinkConfig::default(),
            search: SearchConfig::default(),
            storage_budget: None,
            tenant_id: None,
            epoch_rotation: EpochCadence::Monthly,
            privacy_level: PrivacyLevel::Standard,
        }
    }
}

/// Archive backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchiveBackend {
    /// Use the kdrive Go gateway for archive segment storage.
    Kdrive,
    /// Use ZK Object Fabric (S3 API) for archive segment storage.
    Zkof,
}

/// Media blob sink configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaBlobSinkConfig {
    pub sink: MediaBlobSink,
}

impl Default for MediaBlobSinkConfig {
    fn default() -> Self {
        Self {
            sink: MediaBlobSink::KchatBackend,
        }
    }
}

/// Media blob sink variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaBlobSink {
    /// Default: route through kdrive gateway.
    KchatBackend,
    /// Route to iCloud.
    ICloud { container: String },
    /// Route to Google Drive.
    GoogleDrive { folder: String },
    /// Route to ZK Object Fabric (S3 API).
    ZkObjectFabric { bucket: String, prefix: String },
}

/// Search engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Enable semantic search (requires ONNX models).
    pub semantic_enabled: bool,
    /// Path to the XLM-R model directory (optional, lazy-loaded if None).
    pub xlmr_model_path: Option<PathBuf>,
    /// Use INT4 quantization (default on mobile).
    pub int4_quantization: bool,
    /// Maximum number of cold shards to cache locally.
    pub max_cached_shards: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            semantic_enabled: false,
            xlmr_model_path: None,
            int4_quantization: true,
            max_cached_shards: 64,
        }
    }
}

/// Storage budget configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageBudgetConfig {
    /// Maximum local database size in bytes.
    pub max_db_bytes: u64,
    /// Maximum media cache size in bytes.
    pub max_cache_bytes: u64,
}

impl Default for StorageBudgetConfig {
    fn default() -> Self {
        Self {
            max_db_bytes: 512 * 1024 * 1024,    // 512 MB
            max_cache_bytes: 256 * 1024 * 1024, // 256 MB
        }
    }
}

/// Epoch rotation cadence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpochCadence {
    Monthly,
    Quarterly,
    Yearly,
}

/// Privacy level for archive access patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyLevel {
    /// Standard: batch-by-bucket prefetch.
    Standard,
    /// High: batch-by-bucket prefetch + dummy request padding.
    High,
}
