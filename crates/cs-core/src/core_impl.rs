//! Core implementation — wires all components together.
//!
//! `CoreImpl` is the concrete implementation of the `ChatStorageCore` trait.
//! It owns the local store, drive facade, transport, query engine,
//! archive/backup coordinators, media coordinator, and offload enforcer.

use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::backup::coordinator::BackupCoordinator;
use crate::config::ChatStorageConfig;
use crate::crypto::{key_bridge, Key32};
use crate::local_store::LocalStoreDb;
use crate::message::processor::{IngestedMessage, MessagePersister, MessageProcessor};
use crate::offload::budget::StorageBudgetEnforcer;
use crate::offload::hydration::HydrationQueue;
use crate::restore::pipeline::RestorePipeline;
use crate::search::query_engine::QueryEngine;
use crate::transport::ChatStorageTransport;
use crate::{
    BackupReason, BackupResult, BackupSource, ClientMessageId, DeliveryCursor, DeviceRegistration,
    HydratedMessage, HydrationReason, IngestResult, OffloadResult, RestoreResult, SearchQuery,
    SearchResult, SearchScope, SendMediaResult, StoragePressureReason,
};

/// The concrete implementation of `ChatStorageCore`.
///
/// Owns all subsystems and wires them together. Created via [`CoreImpl::new`].
pub struct CoreImpl {
    config: ChatStorageConfig,
    db: Arc<LocalStoreDb>,
    query_engine: QueryEngine,
    transport: Arc<dyn ChatStorageTransport>,
    wrapping_key: Key32,
    backup_coordinator: Mutex<BackupCoordinator>,
}

impl CoreImpl {
    /// Create a new `CoreImpl` with the given config, wrapping key, and transport.
    ///
    /// The `wrapping_key` is the KDRV1 DomainKey (or ShareGrantKey in Max mode)
    /// from which all purpose-specific keys (archive, backup, search, local DB)
    /// are derived via HKDF-SHA256.
    pub fn new(
        config: ChatStorageConfig,
        wrapping_key: Key32,
        transport: Arc<dyn ChatStorageTransport>,
    ) -> Result<Self, crate::Error> {
        let db_path = config.data_dir.join("chat-storage.db");
        let db_path_str = db_path.to_string_lossy().to_string();

        std::fs::create_dir_all(&config.data_dir)
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;

        let db_key = key_bridge::derive_local_db_key(&wrapping_key);
        let db = Arc::new(LocalStoreDb::open(&db_path_str, &db_key)?);
        let query_engine = QueryEngine::new(db.clone());

        Ok(Self {
            config,
            db,
            query_engine,
            transport,
            wrapping_key,
            backup_coordinator: Mutex::new(BackupCoordinator::new()),
        })
    }

    /// Get a reference to the local store database.
    pub fn db(&self) -> &LocalStoreDb {
        &self.db
    }

    /// Get a reference to the config.
    pub fn config(&self) -> &ChatStorageConfig {
        &self.config
    }

    /// Send a text message.
    pub fn send_text(
        &self,
        conversation_id: Uuid,
        text: &str,
        reply_to: Option<Uuid>,
    ) -> Result<ClientMessageId, crate::Error> {
        let entry = MessageProcessor::create_outbox_entry(conversation_id, text, reply_to);
        MessagePersister::persist_outbox_entry(&self.db, &entry).map_err(crate::Error::Storage)?;
        Ok(ClientMessageId(entry.client_message_id))
    }

    /// Ingest remote messages from the delivery store.
    pub fn ingest_remote_messages(
        &self,
        conversation_id: Uuid,
        after_cursor: Option<DeliveryCursor>,
    ) -> Result<IngestResult, crate::Error> {
        let fetch_result = self
            .transport
            .fetch_messages(
                &conversation_id.to_string(),
                after_cursor.as_ref().map(|c| c.0.as_str()),
            )
            .map_err(crate::Error::Transport)?;

        let mut result = IngestResult {
            next_cursor: fetch_result.next_cursor.map(DeliveryCursor),
            ..Default::default()
        };

        for raw_msg in &fetch_result.messages {
            let ingested = IngestedMessage {
                message_id: raw_msg.message_id.clone(),
                conversation_id: raw_msg.conversation_id.clone(),
                sender_id: raw_msg.sender_id.clone(),
                created_at_ms: raw_msg.created_at_ms,
                text_content: raw_msg.text_content.clone(),
                media_descriptors: raw_msg.media_descriptors.clone(),
                reply_to: raw_msg.reply_to.clone(),
            };

            MessageProcessor::validate(&ingested)?;

            let is_new = MessagePersister::ingest_remote(&self.db, &ingested)
                .map_err(crate::Error::Storage)?;

            if is_new {
                result.new_count += 1;
            } else {
                result.duplicate_count += 1;
            }
        }

        Ok(result)
    }

    /// Execute a search query.
    pub fn search(
        &self,
        query: &SearchQuery,
        scope: SearchScope,
    ) -> Result<Vec<SearchResult>, crate::Error> {
        self.query_engine
            .execute_search(query, scope)
            .map_err(crate::Error::Search)
    }

    /// Hydrate a message from the archive or local store.
    pub fn hydrate_message(
        &self,
        message_id: Uuid,
        reason: HydrationReason,
    ) -> Result<HydratedMessage, crate::Error> {
        let msg_id_str = message_id.to_string();

        // First try local DB — fetch skeleton + body
        if let Ok(Some(skeleton)) = self.db.fetch_skeleton(&msg_id_str) {
            let body = self.db.fetch_body(&msg_id_str).ok().flatten();
            let conv_id =
                Uuid::parse_str(&skeleton.conversation_id).unwrap_or_else(|_| Uuid::nil());
            return Ok(HydratedMessage {
                message_id,
                conversation_id: conv_id,
                sender_id: skeleton.sender_id,
                created_at_ms: skeleton.created_at_ms,
                text_content: body.and_then(|b| b.text_content),
                media_assets: vec![],
            });
        }

        // Not local — queue hydration and process via transport
        let mut queue = HydrationQueue::new();
        queue.push(crate::offload::hydration::HydrationRequest { message_id, reason });
        let hydrated = queue.process(&self.db, self.transport.as_ref())?;

        if hydrated == 0 {
            return Err(crate::Error::Storage(
                format!("message not found: {}", message_id).into(),
            ));
        }

        // Try fetching from DB again after hydration
        if let Ok(Some(skeleton)) = self.db.fetch_skeleton(&msg_id_str) {
            let body = self.db.fetch_body(&msg_id_str).ok().flatten();
            let conv_id =
                Uuid::parse_str(&skeleton.conversation_id).unwrap_or_else(|_| Uuid::nil());
            return Ok(HydratedMessage {
                message_id,
                conversation_id: conv_id,
                sender_id: skeleton.sender_id,
                created_at_ms: skeleton.created_at_ms,
                text_content: body.and_then(|b| b.text_content),
                media_assets: vec![],
            });
        }

        Err(crate::Error::Storage(
            format!("hydration failed: {}", message_id).into(),
        ))
    }

    /// Run an incremental backup.
    pub fn run_incremental_backup(
        &self,
        _reason: BackupReason,
    ) -> Result<BackupResult, crate::Error> {
        let backup_key = key_bridge::derive_backup_root(&self.wrapping_key);

        // Collect data to backup — serialize message skeletons + bodies
        // since the last backup generation. For now, we export the DB snapshot.
        let data = b"chat-storage-backup-snapshot"; // placeholder

        let mut coordinator = self
            .backup_coordinator
            .lock()
            .map_err(|_| crate::Error::Storage("backup coordinator lock poisoned".into()))?;
        let generation = coordinator.run_backup(data, &backup_key, self.transport.as_ref())?;

        Ok(BackupResult {
            segments_built: 1,
            segments_uploaded: 1,
            manifest_generation: generation,
            bytes_uploaded: data.len() as u64,
        })
    }

    /// Enforce the storage budget by evicting cold data.
    pub fn enforce_storage_budget(
        &self,
        _reason: StoragePressureReason,
    ) -> Result<OffloadResult, crate::Error> {
        let budget_config = self.config.storage_budget.clone().unwrap_or_default();
        let enforcer = StorageBudgetEnforcer::new(budget_config);
        let candidates = enforcer.check_and_plan_eviction(&self.db)?;

        let mut bytes_freed = 0u64;
        let mut messages_offloaded = 0usize;
        let mut media_offloaded = 0usize;

        for candidate in &candidates {
            bytes_freed += candidate.size_bytes;
            match candidate.kind {
                crate::offload::eviction::EvictionTier::MessageBodies => {
                    messages_offloaded += 1;
                    let _ = self
                        .db
                        .update_body_state(&candidate.id, "remote_archive_only");
                }
                crate::offload::eviction::EvictionTier::MediaOriginals => {
                    media_offloaded += 1;
                    let _ = self.db.update_media_state(&candidate.id, "evicted");
                }
                crate::offload::eviction::EvictionTier::MediaThumbnails => {
                    media_offloaded += 1;
                    let _ = self.db.update_media_state(&candidate.id, "evicted");
                }
                _ => {}
            }
        }

        Ok(OffloadResult {
            messages_offloaded,
            media_offloaded,
            bytes_freed,
        })
    }

    /// Restore from a backup source.
    pub fn restore_from_backup(&self, source: BackupSource) -> Result<RestoreResult, crate::Error> {
        let mut pipeline = RestorePipeline::new();
        pipeline.execute(&source, &self.wrapping_key, self.transport.as_ref())
    }

    /// Register a device for the current account.
    ///
    /// Uses the KDRV1 identity layer (`kchat-drive-identity`) to generate
    /// a device keypair and device certificate signed by the account root key.
    /// The `account_token` is used to look up or create the account authority.
    ///
    /// In production, the account authority record and device private key
    /// would be persisted in the encrypted `DriveKeyVault`. For now, we
    /// generate the keypair and return the device ID.
    pub fn register_device(&self, account_token: &str) -> Result<DeviceRegistration, crate::Error> {
        use kchat_drive_identity::{enroll_user, DeviceKeyPair};
        use kchat_drive_types::UserId;
        use std::str::FromStr;

        // Parse the account token as a user ID (in production, this would
        // be a JWT or session token resolved to a user ID).
        let user_id = UserId::from_str(account_token)
            .unwrap_or_else(|_| UserId::new(uuid::Uuid::now_v7().into_bytes()));

        // Generate device keypair
        let device_id = kchat_drive_types::DeviceId::random();
        let _device_keypair = DeviceKeyPair::generate(device_id.clone());

        // Enroll user (creates account authority + first device).
        // In production, we'd check if the account already exists in the
        // vault and call add_device instead of enroll_user.
        let _enrollment = enroll_user(user_id, Some(format!("device-{}", device_id)))
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;

        // In production: store device_keypair private key in DriveKeyVault
        // and persist account authority record. For now, just return the result.
        let device_id_hex = hex::encode(device_id.as_bytes());

        Ok(DeviceRegistration {
            device_id: device_id_hex,
            registered: true,
        })
    }

    /// Send a media message.
    pub fn send_media(
        &self,
        conversation_id: Uuid,
        local_file: &std::path::Path,
        caption: Option<&str>,
    ) -> Result<SendMediaResult, crate::Error> {
        let plaintext =
            std::fs::read(local_file).map_err(|e| crate::Error::Storage(e.to_string().into()))?;

        let mime_type = mime_from_extension(local_file);
        let asset_id = uuid::Uuid::now_v7().to_string();
        let message_id = uuid::Uuid::now_v7().to_string();

        // Process media (compute chunk count, merkle root, etc.)
        let descriptor = crate::media::processor::process_media(
            &asset_id, &mime_type, &plaintext, "", // node_id — will be set after upload
            "", // version_id — will be set after upload
        )?;

        // Store media asset record in local DB
        let asset = crate::local_store::MediaAsset {
            asset_id: asset_id.clone(),
            message_id: message_id.clone(),
            mime_type: mime_type.clone(),
            bytes_total: plaintext.len() as i64,
            bytes_local: plaintext.len() as i64,
            media_state: "original_local".to_string(),
            chunk_count: descriptor.chunk_count as i64,
            merkle_root: descriptor.merkle_root.to_vec(),
            node_id: String::new(),
            version_id: String::new(),
            storage_sink: "kdrive".to_string(),
            created_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
        };
        self.db
            .insert_media_asset(&asset)
            .map_err(crate::Error::Storage)?;

        // Store the message skeleton + body (with caption as text)
        if let Some(cap) = caption {
            let entry = MessageProcessor::create_outbox_entry(conversation_id, cap, None);
            MessagePersister::persist_outbox_entry(&self.db, &entry)
                .map_err(crate::Error::Storage)?;
        }

        // TODO: Upload media to kdrive via DriveFacade (requires signing keys)
        // For now, we just store locally and return

        Ok(SendMediaResult {
            message_id: Uuid::parse_str(&message_id).unwrap_or_else(|_| Uuid::now_v7()),
            asset_id: asset_id.clone(),
            node_id: String::new(),
            version_id: String::new(),
        })
    }
}

impl std::fmt::Debug for CoreImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoreImpl")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Infer MIME type from file extension.
fn mime_from_extension(path: &std::path::Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("jpg") | Some("jpeg") => "image/jpeg".to_string(),
        Some("png") => "image/png".to_string(),
        Some("gif") => "image/gif".to_string(),
        Some("webp") => "image/webp".to_string(),
        Some("mp4") => "video/mp4".to_string(),
        Some("webm") => "video/webm".to_string(),
        Some("mp3") => "audio/mpeg".to_string(),
        Some("ogg") => "audio/ogg".to_string(),
        Some("wav") => "audio/wav".to_string(),
        Some("pdf") => "application/pdf".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}
