use cs_core::config::{
    ArchiveBackend, ChatStorageConfig, EpochCadence, MediaBlobSinkConfig, PrivacyLevel,
    PrivacyModeSerde, SearchConfig, StorageBudgetConfig,
};
use cs_core::{ContentKind, SearchQuery};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for ChatStorage (passed from Swift/Kotlin).
#[derive(Debug, Clone, uniffi::Record)]
pub struct ChatStorageConfigFfi {
    pub data_dir: String,
    pub gateway_url: String,
    pub tenant_id: Option<String>,
    pub max_db_bytes: Option<u64>,
    pub max_cache_bytes: Option<u64>,
}

impl ChatStorageConfigFfi {
    pub fn into_rust(self) -> ChatStorageConfig {
        ChatStorageConfig {
            data_dir: std::path::PathBuf::from(&self.data_dir),
            drive_gateway_url: self.gateway_url,
            privacy_mode: PrivacyModeSerde::default(),
            archive_backend: ArchiveBackend::Kdrive,
            media_blob_sink: MediaBlobSinkConfig::default(),
            search: SearchConfig::default(),
            storage_budget: Some(StorageBudgetConfig {
                max_db_bytes: self.max_db_bytes.unwrap_or(512 * 1024 * 1024),
                max_cache_bytes: self.max_cache_bytes.unwrap_or(256 * 1024 * 1024),
            }),
            tenant_id: self.tenant_id,
            epoch_rotation: EpochCadence::Monthly,
            privacy_level: PrivacyLevel::Standard,
        }
    }
}

// ---------------------------------------------------------------------------
// Search types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, uniffi::Record)]
pub struct SearchQueryFfi {
    pub query: String,
    pub sender_id: Option<String>,
    pub conversation_id: Option<String>,
    pub date_from_ms: Option<i64>,
    pub date_to_ms: Option<i64>,
    pub content_kind: Option<String>,
}

impl SearchQueryFfi {
    pub fn into_rust(self) -> Result<SearchQuery, crate::error::ChatStorageError> {
        let conv_id = self
            .conversation_id
            .map(|c| Uuid::parse_str(&c))
            .transpose()
            .map_err(|e| crate::error::invalid_input(format!("invalid conversation_id: {}", e)))?;

        let kind = self
            .content_kind
            .map(|k| match k.as_str() {
                "text" => Ok(ContentKind::Text),
                "media" => Ok(ContentKind::Media),
                "document" => Ok(ContentKind::Document),
                "link" => Ok(ContentKind::Link),
                other => Err(crate::error::invalid_input(format!(
                    "unknown content_kind: \"{}\" (expected text/media/document/link)",
                    other
                ))),
            })
            .transpose()?;

        Ok(SearchQuery {
            query: self.query,
            sender_id: self.sender_id,
            conversation_id: conv_id,
            date_from_ms: self.date_from_ms,
            date_to_ms: self.date_to_ms,
            content_kind: kind,
        })
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SearchResultFfi {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub snippet: String,
    pub score: f64,
    pub from_cold: bool,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum SearchScopeFfi {
    LocalOnly,
    IncludeCold,
}

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, uniffi::Record)]
pub struct SendTextResultFfi {
    pub client_message_id: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct IngestResultFfi {
    pub new_count: u64,
    pub updated_count: u64,
    pub duplicate_count: u64,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct HydratedMessageFfi {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub text_content: Option<String>,
    pub media_assets: Vec<MediaAssetRefFfi>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MediaAssetRefFfi {
    pub asset_id: String,
    pub mime_type: String,
    pub node_id: String,
    pub version_id: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct TimelineEntryFfi {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub kind: String,
    pub text_content: Option<String>,
    pub reply_to: Option<String>,
    pub edited_at_ms: Option<i64>,
    pub deleted_at_ms: Option<i64>,
}

// ---------------------------------------------------------------------------
// Backup / Restore types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BackupReasonFfi {
    Scheduled,
    UserInitiated,
    PreMigration,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BackupResultFfi {
    pub segments_built: u64,
    pub segments_uploaded: u64,
    pub manifest_generation: u64,
    pub bytes_uploaded: u64,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BackupSourceFfi {
    KdriveGateway,
    ICloud,
    AndroidBackup,
    ZkObjectFabric { bucket: String, prefix: String },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct RestoreResultFfi {
    pub conversations_restored: u64,
    pub messages_restored: u64,
    pub media_restored: u64,
    pub search_indexes_rebuilt: u64,
}

// ---------------------------------------------------------------------------
// Storage / Offload types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, uniffi::Enum)]
pub enum StoragePressureReasonFfi {
    BudgetThreshold,
    DiskLow,
    UserInitiated,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct OffloadResultFfi {
    pub messages_offloaded: u64,
    pub media_offloaded: u64,
    pub bytes_freed: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct StorageStatsFfi {
    pub db_size_bytes: u64,
    pub message_count: i64,
    pub media_count: i64,
    pub conversation_count: i64,
    pub evictable_count: i64,
}

// ---------------------------------------------------------------------------
// Device registration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, uniffi::Record)]
pub struct DeviceRegistrationFfi {
    pub device_id: String,
    pub registered: bool,
}

// ---------------------------------------------------------------------------
// Media
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, uniffi::Record)]
pub struct SendMediaResultFfi {
    pub message_id: String,
    pub asset_id: String,
    pub node_id: String,
    pub version_id: String,
}

// ---------------------------------------------------------------------------
// Hydration reason
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, uniffi::Enum)]
pub enum HydrationReasonFfi {
    UserTap,
    ScrollBack,
    SearchHit,
}

// ---------------------------------------------------------------------------
// Conversation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, uniffi::Record)]
pub struct ConversationFfi {
    pub conversation_id: String,
    pub pinned: bool,
    pub muted: bool,
    pub last_message_id: Option<String>,
    pub last_activity_ms: i64,
    pub conversation_type: String,
    pub scope: String,
    pub tenant_id: String,
}
