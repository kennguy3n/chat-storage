//! Test helpers — shared fixtures, context setup, and seed functions.

use std::sync::Arc;

use cs_core::config::{
    ArchiveBackend, ChatStorageConfig, EpochCadence, MediaBlobSinkConfig, PrivacyLevel,
    PrivacyModeSerde, StorageBudgetConfig,
};
use cs_core::crypto::Key32;
use cs_core::local_store::state_machines::{ArchiveState, BackupState, BodyState};
use cs_core::local_store::{Conversation, LocalStoreDb, MessageBody, MessageKind, MessageSkeleton};
use cs_core::message::processor::{IngestedMessage, MessagePersister, MessageProcessor};
use cs_core::transport::kdrive_bridge::KdriveTransport;
use cs_core::transport::ChatStorageTransport;
use cs_core::CoreImpl;
use uuid::Uuid;

/// Derive a deterministic wrapping key for a tenant.
pub fn tenant_wrapping_key(tenant_id: &str) -> Key32 {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"chat-storage-e2e-test-master");
    hasher.update(tenant_id.as_bytes());
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// Create a test config pointing at the given gateway URL.
pub fn make_config(gateway_url: &str, data_dir: &std::path::Path) -> ChatStorageConfig {
    ChatStorageConfig {
        data_dir: data_dir.to_path_buf(),
        drive_gateway_url: gateway_url.to_string(),
        privacy_mode: PrivacyModeSerde::default(),
        archive_backend: ArchiveBackend::Kdrive,
        media_blob_sink: MediaBlobSinkConfig::default(),
        search: Default::default(),
        storage_budget: Some(StorageBudgetConfig::default()),
        tenant_id: None,
        epoch_rotation: EpochCadence::Monthly,
        privacy_level: PrivacyLevel::Standard,
    }
}

/// Create a `CoreImpl` wired with a live `KdriveTransport`.
pub fn make_core(
    gateway_url: &str,
    tenant_id: &str,
    user_id: &str,
    data_dir: &std::path::Path,
) -> CoreImpl {
    let mut config = make_config(gateway_url, data_dir);
    config.tenant_id = Some(tenant_id.to_string());
    let wrapping_key = tenant_wrapping_key(tenant_id);
    let auth_token = format!("test-token-{}", tenant_id);
    let transport: Arc<dyn ChatStorageTransport> = Arc::new(KdriveTransport::new(
        gateway_url.to_string(),
        auth_token,
        tenant_id.to_string(),
        user_id.to_string(),
    ));
    CoreImpl::new(config, wrapping_key, transport).expect("failed to create CoreImpl")
}

/// Create an in-memory `LocalStoreDb` for tests that don't need transport.
pub fn make_in_memory_db() -> LocalStoreDb {
    LocalStoreDb::open_in_memory(&[0x42u8; 32]).expect("failed to open in-memory DB")
}

/// Create a temp directory for test data.
pub fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("failed to create temp dir")
}

/// Create and insert a conversation.
pub fn seed_conversation(db: &LocalStoreDb, conv_id: &str, scope: &str, tenant_id: Option<&str>) {
    let conv = Conversation {
        conversation_id: conv_id.to_string(),
        title_cipher: None,
        pinned: false,
        muted: false,
        last_message_id: None,
        last_activity_ms: 1_700_000_000_000,
        conversation_type: "dm".to_string(),
        scope: scope.to_string(),
        tenant_id: tenant_id.unwrap_or("").to_string(),
        community_id: String::new(),
        domain_id: String::new(),
    };
    db.insert_conversation(&conv)
        .expect("failed to insert conversation");
}

/// Ingest a single message into the DB and return its message_id.
pub fn ingest_one(
    db: &LocalStoreDb,
    conv_id: &str,
    sender_id: &str,
    text: &str,
    created_at_ms: i64,
) -> String {
    let msg = IngestedMessage {
        message_id: Uuid::now_v7().to_string(),
        conversation_id: conv_id.to_string(),
        sender_id: sender_id.to_string(),
        created_at_ms,
        text_content: Some(text.to_string()),
        media_descriptors: vec![],
        reply_to: None,
    };
    MessageProcessor::validate(&msg).expect("validation failed");
    MessagePersister::new(db)
        .persist_ingested_message(&msg)
        .expect("ingest failed");
    msg.message_id
}

/// Ingest N messages into a conversation with incremental timestamps.
pub fn seed_messages(db: &LocalStoreDb, conv_id: &str, count: usize) -> Vec<String> {
    let mut ids = Vec::with_capacity(count);
    for i in 0..count {
        let id = ingest_one(
            db,
            conv_id,
            "user-a",
            &format!("Message number {i} in {conv_id}"),
            1_700_000_000_000 + i as i64 * 1000,
        );
        ids.push(id);
    }
    ids
}

/// Ingest multilingual messages (English, Japanese, Arabic, Emoji).
pub fn seed_multilingual(db: &LocalStoreDb, conv_id: &str) -> Vec<String> {
    let texts = [
        "Hello world from KChat",
        "会議の議事録を共有します",
        "مرحبا بالعالم من كتشات",
        "🎉🚀 Party time! 🎊",
    ];
    texts
        .iter()
        .enumerate()
        .map(|(i, text)| {
            ingest_one(
                db,
                conv_id,
                "user-multilingual",
                text,
                1_700_000_000_000 + i as i64 * 1000,
            )
        })
        .collect()
}

/// Insert a skeleton directly (bypassing the ingest pipeline).
#[allow(dead_code)]
pub fn insert_skeleton(
    db: &LocalStoreDb,
    message_id: &str,
    conv_id: &str,
    sender_id: &str,
    created_at_ms: i64,
    kind: MessageKind,
) {
    let skeleton = MessageSkeleton {
        message_id: message_id.to_string(),
        conversation_id: conv_id.to_string(),
        sender_id: sender_id.to_string(),
        created_at_ms,
        received_at_ms: created_at_ms + 100,
        kind,
        body_state: BodyState::LocalPlainAvailable,
        media_state: None,
        archive_state: ArchiveState::NotArchived,
        backup_state: BackupState::NotBackedUp,
        reply_to: None,
        edited_at_ms: None,
        deleted_at_ms: None,
    };
    db.insert_skeleton(&skeleton)
        .expect("failed to insert skeleton");
}

/// Insert a body for a message.
#[allow(dead_code)]
pub fn insert_body(db: &LocalStoreDb, message_id: &str, text: &str) {
    let body = MessageBody {
        message_id: message_id.to_string(),
        text_content: Some(text.to_string()),
        detected_language: Some("en".to_string()),
        rich_meta: None,
    };
    db.insert_body(&body).expect("failed to insert body");
}

/// Mark a message as deleted.
pub fn mark_deleted(db: &LocalStoreDb, message_id: &str) {
    let conn = db.write().expect("failed to get write lock");
    conn.execute(
        "UPDATE message_skeleton SET deleted_at_ms = ?2 WHERE message_id = ?1",
        rusqlite::params![message_id, 1_700_000_001_000_i64],
    )
    .expect("failed to mark deleted");
}

/// Mark a message as archived (for eviction tests).
pub fn mark_archived(db: &LocalStoreDb, message_id: &str) {
    let conn = db.write().expect("failed to get write lock");
    conn.execute(
        "UPDATE message_skeleton SET archive_state = 'archive_verified' WHERE message_id = ?1",
        rusqlite::params![message_id],
    )
    .expect("failed to mark archived");
}

/// Create a test PNG file (minimal valid PNG).
#[allow(dead_code)]
pub fn make_test_png(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("test.png");
    // Minimal 1x1 white PNG
    let png_bytes: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49,
        0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01,
        0xE2, 0x21, 0xBC, 0x33, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60,
        0x82,
    ];
    std::fs::write(&path, png_bytes).expect("failed to write test PNG");
    path
}
