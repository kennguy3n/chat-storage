use cs_core::config::{
    ArchiveBackend, ChatStorageConfig, EpochCadence, MediaBlobSinkConfig, PrivacyLevel,
    PrivacyModeSerde, StorageBudgetConfig,
};
use cs_core::{ContentKind, SearchQuery};
use napi_derive::napi;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[napi(object)]
pub struct ChatStorageConfigJs {
    pub data_dir: String,
    pub gateway_url: String,
    pub tenant_id: Option<String>,
    pub privacy_mode: Option<i32>,
    pub max_db_bytes: Option<i64>,
    pub max_cache_bytes: Option<i64>,
}

impl ChatStorageConfigJs {
    pub fn into_rust(self) -> ChatStorageConfig {
        ChatStorageConfig {
            data_dir: std::path::PathBuf::from(&self.data_dir),
            drive_gateway_url: self.gateway_url,
            privacy_mode: PrivacyModeSerde::default(),
            archive_backend: ArchiveBackend::Kdrive,
            media_blob_sink: MediaBlobSinkConfig::default(),
            search: Default::default(),
            storage_budget: Some(StorageBudgetConfig {
                max_db_bytes: self.max_db_bytes.unwrap_or(512 * 1024 * 1024) as u64,
                max_cache_bytes: self.max_cache_bytes.unwrap_or(256 * 1024 * 1024) as u64,
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

#[napi(object)]
pub struct SearchQueryJs {
    pub query: String,
    pub sender_id: Option<String>,
    pub conversation_id: Option<String>,
    pub date_from_ms: Option<i64>,
    pub date_to_ms: Option<i64>,
    pub content_kind: Option<String>,
}

impl SearchQueryJs {
    pub fn into_rust(self) -> Result<SearchQuery, napi::Error> {
        let conv_id = self
            .conversation_id
            .map(|c| Uuid::parse_str(&c))
            .transpose()
            .map_err(|e| crate::error::invalid_input(format!("invalid conversation_id: {}", e)))?;

        let kind = self.content_kind.map(|k| match k.as_str() {
            "text" => ContentKind::Text,
            "media" => ContentKind::Media,
            "document" => ContentKind::Document,
            "link" => ContentKind::Link,
            _ => ContentKind::Text,
        });

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

#[napi(object)]
pub struct SearchResultJs {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub snippet: String,
    pub score: f64,
    pub from_cold: bool,
}

#[napi]
pub enum SearchScopeJs {
    LocalOnly,
    IncludeCold,
}

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

#[napi(object)]
pub struct SendTextResultJs {
    pub client_message_id: String,
}

#[napi(object)]
pub struct IngestResultJs {
    pub new_count: i64,
    pub updated_count: i64,
    pub duplicate_count: i64,
    pub next_cursor: Option<String>,
}

#[napi(object)]
pub struct HydratedMessageJs {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub text_content: Option<String>,
    pub media_assets: Vec<MediaAssetRefJs>,
}

#[napi(object)]
pub struct MediaAssetRefJs {
    pub asset_id: String,
    pub mime_type: String,
    pub node_id: String,
    pub version_id: String,
}

#[napi(object)]
pub struct TimelineEntryJs {
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

#[napi]
pub enum BackupReasonJs {
    Scheduled,
    UserInitiated,
    PreMigration,
}

#[napi(object)]
pub struct BackupResultJs {
    pub segments_built: i64,
    pub segments_uploaded: i64,
    pub manifest_generation: i64,
    pub bytes_uploaded: i64,
}

#[napi]
pub enum BackupSourceJs {
    KdriveGateway,
    ICloud,
    AndroidBackup,
    ZkObjectFabric {
        bucket: String,
        prefix: String,
    },
}

#[napi(object)]
pub struct RestoreResultJs {
    pub conversations_restored: i64,
    pub messages_restored: i64,
    pub media_restored: i64,
    pub search_indexes_rebuilt: i64,
}

// ---------------------------------------------------------------------------
// Storage / Offload types
// ---------------------------------------------------------------------------

#[napi]
pub enum StoragePressureReasonJs {
    BudgetThreshold,
    DiskLow,
    UserInitiated,
}

#[napi(object)]
pub struct OffloadResultJs {
    pub messages_offloaded: i64,
    pub media_offloaded: i64,
    pub bytes_freed: i64,
}

#[napi(object)]
pub struct StorageStatsJs {
    pub db_size_bytes: i64,
    pub message_count: i64,
    pub media_count: i64,
    pub conversation_count: i64,
    pub evictable_count: i64,
}

// ---------------------------------------------------------------------------
// Device registration
// ---------------------------------------------------------------------------

#[napi(object)]
pub struct DeviceRegistrationJs {
    pub device_id: String,
    pub registered: bool,
}

// ---------------------------------------------------------------------------
// Media
// ---------------------------------------------------------------------------

#[napi(object)]
pub struct SendMediaResultJs {
    pub message_id: String,
    pub asset_id: String,
    pub node_id: String,
    pub version_id: String,
}

// ---------------------------------------------------------------------------
// Hydration reason
// ---------------------------------------------------------------------------

#[napi]
pub enum HydrationReasonJs {
    UserTap,
    ScrollBack,
    SearchHit,
}

// ---------------------------------------------------------------------------
// Conversation
// ---------------------------------------------------------------------------

#[napi(object)]
pub struct ConversationJs {
    pub conversation_id: String,
    pub pinned: bool,
    pub muted: bool,
    pub last_message_id: Option<String>,
    pub last_activity_ms: i64,
    pub conversation_type: String,
    pub scope: String,
    pub tenant_id: String,
}
