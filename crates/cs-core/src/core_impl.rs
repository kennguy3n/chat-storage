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
        let persister = MessagePersister::new(&self.db);
        persister
            .persist_outbox_entry(&entry)
            .map_err(crate::Error::from)?;
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

            let persister = MessagePersister::new(&self.db);
            match persister.persist_ingested_message(&ingested) {
                Ok(()) => result.new_count += 1,
                Err(crate::message::ProcessorError::DuplicateMessage) => {
                    result.duplicate_count += 1;
                }
                Err(e) => return Err(crate::Error::from(e)),
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

        // Collect actual data from the local store — serialize conversations,
        // message skeletons, and bodies into a CBOR backup snapshot.
        let snapshot = crate::backup::snapshot::BackupSnapshot::from_db(&self.db)?;
        let data = snapshot.to_cbor()?;

        if data.is_empty() {
            return Ok(BackupResult::default());
        }

        let mut coordinator = self
            .backup_coordinator
            .lock()
            .map_err(|_| crate::Error::Storage("backup coordinator lock poisoned".into()))?;
        let generation = coordinator.run_backup(&data, &backup_key, self.transport.as_ref())?;

        // Mark all backed-up skeletons as backed_up
        snapshot.mark_backed_up(&self.db)?;

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
                    self.db
                        .update_body_state(&candidate.id, "remote_archive_only")?;
                }
                crate::offload::eviction::EvictionTier::MediaOriginals => {
                    media_offloaded += 1;
                    self.db.update_media_state(&candidate.id, "evicted")?;
                }
                crate::offload::eviction::EvictionTier::MediaThumbnails => {
                    media_offloaded += 1;
                    self.db.update_media_state(&candidate.id, "evicted")?;
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
        pipeline.execute(
            &source,
            &self.wrapping_key,
            self.transport.as_ref(),
            &self.db,
        )
    }

    /// Register a device for the current account.
    ///
    /// Uses the KDRV1 identity layer (`kchat-drive-identity`) to generate
    /// a device keypair and device certificate signed by the account root key.
    /// The `account_token` is used to look up or create the account authority.
    ///
    /// The device private key is persisted in an encrypted `DriveKeyVault`.
    /// The vault's master key is derived from the wrapping key via HKDF and
    /// is **never** written to disk. Only the encrypted vault entries are
    /// persisted; the master key is re-derived on each session.
    pub fn register_device(&self, account_token: &str) -> Result<DeviceRegistration, crate::Error> {
        use kchat_drive_identity::{enroll_user, DeviceKeyPair, DriveKeyVault};
        use kchat_drive_types::UserId;
        use std::str::FromStr;

        // Parse the account token as a user ID (in production, this would
        // be a JWT or session token resolved to a user ID).
        let user_id = UserId::from_str(account_token)
            .unwrap_or_else(|_| UserId::new(uuid::Uuid::now_v7().into_bytes()));

        // Generate device keypair
        let device_id = kchat_drive_types::DeviceId::random();
        let device_keypair = DeviceKeyPair::generate(device_id.clone());

        // Enroll user (creates account authority + first device).
        let _enrollment = enroll_user(user_id, Some(format!("device-{}", device_id)))
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;

        // Derive vault master key from wrapping key (never persisted to disk)
        let vault_master_key = key_bridge::derive_local_db_key(&self.wrapping_key);

        // Load existing vault entries from disk (if any), then add new key
        let vault_path = self.config.data_dir.join("device_vault.bin");
        let mut vault = DriveKeyVault::from_master_key(vault_master_key);

        // Import existing entries if vault file exists
        if vault_path.exists() {
            if let Ok(vault_bytes) = std::fs::read(&vault_path) {
                if let Ok(entries) = serde_json::from_slice::<
                    Vec<kchat_drive_identity::VaultEntryExport>,
                >(&vault_bytes)
                {
                    vault.import_data(&entries);
                }
            }
        }

        // Store the device private key in the vault
        let device_key_id = format!("device:{}", hex::encode(device_id.as_bytes()));
        vault
            .store(&device_key_id, &device_keypair.private_key_bytes())
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;

        // Persist only the encrypted vault entries (master key is never on disk)
        let vault_bytes = serde_json::to_vec(&vault.export_data())
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;
        std::fs::write(&vault_path, &vault_bytes)
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;

        let device_id_hex = hex::encode(device_id.as_bytes());

        Ok(DeviceRegistration {
            device_id: device_id_hex,
            registered: true,
        })
    }

    /// Send a media message.
    ///
    /// Encrypts the media with a per-asset key derived from the media key
    /// hierarchy (separate from archive keys), uploads the encrypted blob,
    /// and persists the skeleton + body + media asset in the local store.
    pub fn send_media(
        &self,
        conversation_id: Uuid,
        local_file: &std::path::Path,
        caption: Option<&str>,
    ) -> Result<SendMediaResult, crate::Error> {
        use crate::local_store::state_machines::{
            ArchiveState, BackupState, BodyState, MediaState,
        };
        use crate::local_store::{MessageKind, MessageSkeleton};

        let mut plaintext =
            std::fs::read(local_file).map_err(|e| crate::Error::Storage(e.to_string().into()))?;

        let mime_type = mime_from_extension(local_file);
        let asset_id = uuid::Uuid::now_v7().to_string();
        let message_id = uuid::Uuid::now_v7().to_string();
        let blob_id = uuid::Uuid::now_v7().to_string();
        let now = now_ms();

        // Process media (compute chunk count, merkle root, etc.)
        let descriptor = crate::media::processor::process_media(
            &asset_id, &mime_type, &plaintext, &blob_id, &blob_id, &blob_id,
        )?;

        // Encrypt the media plaintext using a per-asset media key
        // derived from K_media_root (separate from archive key hierarchy)
        let media_root = key_bridge::derive_media_root(&self.wrapping_key);
        let media_key = key_bridge::derive_media_blob(&media_root, asset_id.as_bytes());
        let nonce = crate::crypto::aead::random_nonce_24();
        let ciphertext = crate::crypto::seal(
            &media_key,
            &nonce,
            &plaintext,
            b"chat-storage/media-blob/v1",
        )?;

        // Zeroize plaintext after encryption
        use zeroize::Zeroize;
        plaintext.zeroize();

        // Insert skeleton + body + media asset into DB *before* uploading.
        // All three inserts are wrapped in a SAVEPOINT so that a failure in
        // any one rolls back the others — no partial records left behind.
        let skeleton = MessageSkeleton {
            message_id: message_id.clone(),
            conversation_id: conversation_id.to_string(),
            sender_id: "local".to_string(),
            created_at_ms: now,
            received_at_ms: now,
            kind: MessageKind::Media,
            body_state: BodyState::LocalPlainAvailable,
            media_state: Some(MediaState::RemoteOriginal),
            archive_state: ArchiveState::NotArchived,
            backup_state: BackupState::NotBackedUp,
            reply_to: None,
            edited_at_ms: None,
            deleted_at_ms: None,
        };

        // Insert caption as body text (or empty body for media-only)
        let body = crate::local_store::MessageBody {
            message_id: message_id.clone(),
            text_content: caption.map(|c| c.to_string()),
            detected_language: None,
            rich_meta: None,
        };

        // Insert media asset record
        let asset = crate::local_store::MediaAsset {
            asset_id: asset_id.clone(),
            message_id: message_id.clone(),
            mime_type: mime_type.clone(),
            bytes_total: descriptor.bytes_total as i64,
            bytes_local: 0,
            media_state: MediaState::RemoteOriginal,
            wrapped_k_asset: vec![0u8; 40],
            chunk_count: descriptor.chunk_count as i32,
            merkle_root: descriptor.merkle_root.to_vec(),
            blob_id: blob_id.clone(),
            storage_sink: "kdrive".to_string(),
        };

        {
            let conn = self.db.write()?;
            conn.execute_batch("SAVEPOINT send_media;")
                .map_err(|e| crate::Error::Storage(e.to_string().into()))?;

            let insert_result = (|| {
                conn.execute(
                    "INSERT OR REPLACE INTO message_skeleton
                     (message_id, conversation_id, sender_id, created_at_ms, received_at_ms, kind, body_state, media_state, archive_state, backup_state, reply_to, edited_at_ms, deleted_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    rusqlite::params![
                        skeleton.message_id,
                        skeleton.conversation_id,
                        skeleton.sender_id,
                        skeleton.created_at_ms,
                        skeleton.received_at_ms,
                        skeleton.kind.as_str(),
                        skeleton.body_state.to_string(),
                        skeleton.media_state.map(|s| s.to_string()),
                        skeleton.archive_state.to_string(),
                        skeleton.backup_state.to_string(),
                        skeleton.reply_to,
                        skeleton.edited_at_ms,
                        skeleton.deleted_at_ms,
                    ],
                )?;
                conn.execute(
                    "INSERT OR REPLACE INTO message_body (message_id, text_content, detected_language, rich_meta)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![
                        body.message_id,
                        body.text_content,
                        body.detected_language,
                        body.rich_meta,
                    ],
                )?;
                conn.execute(
                    "INSERT OR REPLACE INTO media_asset
                     (asset_id, message_id, mime_type, bytes_total, bytes_local, media_state, wrapped_k_asset, chunk_count, merkle_root, blob_id, storage_sink)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    rusqlite::params![
                        asset.asset_id,
                        asset.message_id,
                        asset.mime_type,
                        asset.bytes_total,
                        asset.bytes_local,
                        asset.media_state.to_string(),
                        asset.wrapped_k_asset,
                        asset.chunk_count,
                        asset.merkle_root,
                        asset.blob_id,
                        asset.storage_sink,
                    ],
                )?;
                Ok::<(), rusqlite::Error>(())
            })();

            match insert_result {
                Ok(()) => {
                    conn.execute_batch("RELEASE send_media;")
                        .map_err(|e| crate::Error::Storage(e.to_string().into()))?;
                }
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK TO send_media; RELEASE send_media;");
                    return Err(crate::Error::Storage(e.to_string().into()));
                }
            }
        }

        // Upload encrypted media blob to the gateway via transport
        let uploaded_blob_id = self
            .transport
            .upload_media_blob(&blob_id, &ciphertext)
            .map_err(crate::Error::Transport)?;

        Ok(SendMediaResult {
            message_id: Uuid::parse_str(&message_id).unwrap_or_else(|_| Uuid::now_v7()),
            asset_id,
            node_id: uploaded_blob_id.clone(),
            version_id: uploaded_blob_id,
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

/// Current wall-clock time in milliseconds since Unix epoch.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
