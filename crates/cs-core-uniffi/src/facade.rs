use std::sync::Arc;

use cs_core::transport::kdrive_bridge::KdriveTransport;
use cs_core::transport::ChatStorageTransport;
use cs_core::{BackupReason, BackupSource, HydrationReason, SearchScope};
use uuid::Uuid;

use crate::error::{invalid_input, ChatStorageError};
use crate::types::*;

/// ChatStorage — the main entry point for the chat-storage SDK.
///
/// Wraps `CoreImpl` and persists across calls within the app process.
/// Created once at app startup with a config and wrapping key.
#[derive(uniffi::Object)]
pub struct ChatStorage {
    inner: Arc<cs_core::CoreImpl>,
}

#[uniffi::export]
impl ChatStorage {
    /// Create a new ChatStorage instance.
    ///
    /// - `config`: Configuration object (dataDir, gatewayUrl, tenantId, etc.)
    /// - `wrapping_key_hex`: 32-byte hex-encoded key (64 hex chars). Used to derive
    ///   all purpose-specific keys (archive, backup, search, local DB).
    /// - `auth_token`: Bearer token for gateway authentication.
    /// - `user_id`: User ID for gateway routing.
    #[uniffi::constructor]
    pub fn new(
        config: ChatStorageConfigFfi,
        wrapping_key_hex: String,
        auth_token: String,
        user_id: String,
    ) -> Result<Self, ChatStorageError> {
        let key_bytes = hex::decode(&wrapping_key_hex)
            .map_err(|e| invalid_input(format!("invalid wrapping_key hex: {}", e)))?;
        if key_bytes.len() != 32 {
            return Err(invalid_input(format!(
                "wrapping_key must be 32 bytes (64 hex chars), got {} bytes",
                key_bytes.len()
            )));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);

        let cfg = config.into_rust();
        let tenant_id = cfg.tenant_id.clone().unwrap_or_default();
        let transport: Arc<dyn ChatStorageTransport> = Arc::new(KdriveTransport::new(
            cfg.drive_gateway_url.clone(),
            auth_token,
            tenant_id,
            user_id,
        ));

        let core = cs_core::CoreImpl::new(cfg, key, transport).map_err(ChatStorageError::from)?;

        Ok(Self {
            inner: Arc::new(core),
        })
    }

    /// Send a text message to a conversation.
    pub fn send_text(
        &self,
        conversation_id: String,
        text: String,
        reply_to: Option<String>,
    ) -> Result<SendTextResultFfi, ChatStorageError> {
        let conv_id = Uuid::parse_str(&conversation_id)
            .map_err(|e| invalid_input(format!("invalid conversation_id: {}", e)))?;
        let reply = reply_to
            .map(|r| Uuid::parse_str(&r))
            .transpose()
            .map_err(|e| invalid_input(format!("invalid reply_to: {}", e)))?;

        let msg_id = self
            .inner
            .send_text(conv_id, &text, reply)
            .map_err(ChatStorageError::from)?;

        Ok(SendTextResultFfi {
            client_message_id: msg_id.0.to_string(),
        })
    }

    /// Ingest remote messages from the delivery store via the gateway.
    pub fn ingest_remote(
        &self,
        conversation_id: String,
        after_cursor: Option<String>,
    ) -> Result<IngestResultFfi, ChatStorageError> {
        let conv_id = Uuid::parse_str(&conversation_id)
            .map_err(|e| invalid_input(format!("invalid conversation_id: {}", e)))?;

        let cursor = after_cursor.map(cs_core::DeliveryCursor);

        let result = self
            .inner
            .ingest_remote_messages(conv_id, cursor)
            .map_err(ChatStorageError::from)?;

        Ok(IngestResultFfi {
            new_count: result.new_count as u64,
            updated_count: result.updated_count as u64,
            duplicate_count: result.duplicate_count as u64,
            next_cursor: result.next_cursor.map(|c| c.0),
        })
    }

    /// Execute a search query.
    pub fn search(
        &self,
        query: SearchQueryFfi,
        scope: SearchScopeFfi,
    ) -> Result<Vec<SearchResultFfi>, ChatStorageError> {
        let q = query.into_rust()?;
        let s = match scope {
            SearchScopeFfi::LocalOnly => SearchScope::LocalOnly,
            SearchScopeFfi::IncludeCold => SearchScope::IncludeCold,
        };

        let results = self.inner.search(&q, s).map_err(ChatStorageError::from)?;

        Ok(results
            .into_iter()
            .map(|r| SearchResultFfi {
                message_id: r.message_id.to_string(),
                conversation_id: r.conversation_id.to_string(),
                sender_id: r.sender_id,
                created_at_ms: r.created_at_ms,
                snippet: r.snippet,
                score: r.score,
                from_cold: r.from_cold,
            })
            .collect())
    }

    /// Hydrate a message from the archive or local store.
    pub fn hydrate_message(
        &self,
        message_id: String,
        reason: HydrationReasonFfi,
    ) -> Result<HydratedMessageFfi, ChatStorageError> {
        let msg_id = Uuid::parse_str(&message_id)
            .map_err(|e| invalid_input(format!("invalid message_id: {}", e)))?;
        let r = match reason {
            HydrationReasonFfi::UserTap => HydrationReason::UserTap,
            HydrationReasonFfi::ScrollBack => HydrationReason::ScrollBack,
            HydrationReasonFfi::SearchHit => HydrationReason::SearchHit,
        };

        let msg = self
            .inner
            .hydrate_message(msg_id, r)
            .map_err(ChatStorageError::from)?;

        Ok(HydratedMessageFfi {
            message_id: msg.message_id.to_string(),
            conversation_id: msg.conversation_id.to_string(),
            sender_id: msg.sender_id,
            created_at_ms: msg.created_at_ms,
            text_content: msg.text_content,
            media_assets: msg
                .media_assets
                .into_iter()
                .map(|a| MediaAssetRefFfi {
                    asset_id: a.asset_id,
                    mime_type: a.mime_type,
                    node_id: a.node_id,
                    version_id: a.version_id,
                })
                .collect(),
        })
    }

    /// Run an incremental backup.
    pub fn run_backup(&self, reason: BackupReasonFfi) -> Result<BackupResultFfi, ChatStorageError> {
        let r = match reason {
            BackupReasonFfi::Scheduled => BackupReason::Scheduled,
            BackupReasonFfi::UserInitiated => BackupReason::UserInitiated,
            BackupReasonFfi::PreMigration => BackupReason::PreMigration,
        };

        let result = self
            .inner
            .run_incremental_backup(r)
            .map_err(ChatStorageError::from)?;

        Ok(BackupResultFfi {
            segments_built: result.segments_built as u64,
            segments_uploaded: result.segments_uploaded as u64,
            manifest_generation: result.manifest_generation,
            bytes_uploaded: result.bytes_uploaded,
        })
    }

    /// Enforce the storage budget by evicting cold data.
    pub fn enforce_budget(
        &self,
        reason: StoragePressureReasonFfi,
    ) -> Result<OffloadResultFfi, ChatStorageError> {
        let r = match reason {
            StoragePressureReasonFfi::BudgetThreshold => {
                cs_core::StoragePressureReason::BudgetThreshold
            }
            StoragePressureReasonFfi::DiskLow => cs_core::StoragePressureReason::DiskLow,
            StoragePressureReasonFfi::UserInitiated => {
                cs_core::StoragePressureReason::UserInitiated
            }
        };

        let result = self
            .inner
            .enforce_storage_budget(r)
            .map_err(ChatStorageError::from)?;

        Ok(OffloadResultFfi {
            messages_offloaded: result.messages_offloaded as u64,
            media_offloaded: result.media_offloaded as u64,
            bytes_freed: result.bytes_freed,
        })
    }

    /// Restore from a backup source.
    pub fn restore_from_backup(
        &self,
        source: BackupSourceFfi,
    ) -> Result<RestoreResultFfi, ChatStorageError> {
        let src = match source {
            BackupSourceFfi::KdriveGateway => BackupSource::KdriveGateway,
            BackupSourceFfi::ICloud => BackupSource::ICloud,
            BackupSourceFfi::AndroidBackup => BackupSource::AndroidBackup,
            BackupSourceFfi::ZkObjectFabric { bucket, prefix } => {
                BackupSource::ZkObjectFabric { bucket, prefix }
            }
        };

        let result = self
            .inner
            .restore_from_backup(src)
            .map_err(ChatStorageError::from)?;

        Ok(RestoreResultFfi {
            conversations_restored: result.conversations_restored as u64,
            messages_restored: result.messages_restored as u64,
            media_restored: result.media_restored as u64,
            search_indexes_rebuilt: result.search_indexes_rebuilt as u64,
        })
    }

    /// Register a device for the current account.
    pub fn register_device(
        &self,
        account_token: String,
    ) -> Result<DeviceRegistrationFfi, ChatStorageError> {
        let result = self
            .inner
            .register_device(&account_token)
            .map_err(ChatStorageError::from)?;

        Ok(DeviceRegistrationFfi {
            device_id: result.device_id,
            registered: result.registered,
        })
    }

    /// Send a media message from in-memory bytes (no file path required).
    ///
    /// This is the iOS-friendly variant. The caller provides raw plaintext
    /// bytes and the MIME type (e.g. "image/jpeg", "video/mp4").
    pub fn send_media_bytes(
        &self,
        conversation_id: String,
        data: Vec<u8>,
        mime_type: String,
        caption: Option<String>,
    ) -> Result<SendMediaResultFfi, ChatStorageError> {
        let conv_id = Uuid::parse_str(&conversation_id)
            .map_err(|e| invalid_input(format!("invalid conversation_id: {}", e)))?;

        let result = self
            .inner
            .send_media_bytes(conv_id, data, &mime_type, caption.as_deref())
            .map_err(ChatStorageError::from)?;

        Ok(SendMediaResultFfi {
            message_id: result.message_id.to_string(),
            asset_id: result.asset_id,
            node_id: result.node_id,
            version_id: result.version_id,
        })
    }

    /// Create and insert a conversation into the local store.
    pub fn seed_conversation(
        &self,
        conversation_id: String,
        title: Option<String>,
        scope: Option<String>,
    ) -> Result<(), ChatStorageError> {
        use cs_core::local_store::Conversation;

        let conv = Conversation::legacy(
            conversation_id,
            title.map(|t| t.into_bytes()),
            false,
            false,
            None,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
        );
        let mut conv = conv;
        if let Some(s) = scope {
            conv.scope = s;
        }

        self.inner
            .db()
            .insert_conversation(&conv)
            .map_err(|e| ChatStorageError::from(cs_core::Error::Storage(e)))?;

        Ok(())
    }

    /// Get the message timeline for a conversation (newest first).
    pub fn get_timeline(
        &self,
        conversation_id: String,
        limit: Option<u64>,
        before_ms: Option<i64>,
    ) -> Result<Vec<TimelineEntryFfi>, ChatStorageError> {
        let lim = limit.unwrap_or(50) as usize;
        let timeline = self
            .inner
            .db()
            .fetch_timeline(&conversation_id, lim, before_ms)
            .map_err(|e| ChatStorageError::from(cs_core::Error::Storage(e)))?;

        let mut entries = Vec::with_capacity(timeline.len());
        for row in timeline {
            let body = self.inner.db().fetch_body(&row.message_id).ok().flatten();
            entries.push(TimelineEntryFfi {
                message_id: row.message_id,
                conversation_id: row.conversation_id,
                sender_id: row.sender_id,
                created_at_ms: row.created_at_ms,
                kind: match row.kind {
                    cs_core::local_store::MessageKind::Text => "text".to_string(),
                    cs_core::local_store::MessageKind::Media => "media".to_string(),
                    cs_core::local_store::MessageKind::System => "system".to_string(),
                },
                text_content: body.and_then(|b| b.text_content),
                reply_to: row.reply_to,
                edited_at_ms: row.edited_at_ms,
                deleted_at_ms: row.deleted_at_ms,
            });
        }
        Ok(entries)
    }

    /// Get storage statistics.
    pub fn get_storage_stats(&self) -> Result<StorageStatsFfi, ChatStorageError> {
        let db = self.inner.db();

        let db_size = db
            .db_size_bytes()
            .map_err(|e| ChatStorageError::from(cs_core::Error::Storage(e)))?;

        let conn = db
            .read()
            .map_err(|e| ChatStorageError::from(cs_core::Error::Storage(e)))?;

        let message_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM message_skeleton WHERE deleted_at_ms IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(|e| ChatStorageError::from(cs_core::Error::Storage(e.into())))?;

        let media_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM media_asset", [], |row| row.get(0))
            .map_err(|e| ChatStorageError::from(cs_core::Error::Storage(e.into())))?;

        let conversation_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM conversation", [], |row| row.get(0))
            .map_err(|e| ChatStorageError::from(cs_core::Error::Storage(e.into())))?;

        let evictable_bodies = db
            .fetch_evictable_bodies(1000)
            .map_err(|e| ChatStorageError::from(cs_core::Error::Storage(e)))?;
        let evictable_media = db
            .fetch_evictable_media(1000)
            .map_err(|e| ChatStorageError::from(cs_core::Error::Storage(e)))?;

        Ok(StorageStatsFfi {
            db_size_bytes: db_size,
            message_count,
            media_count,
            conversation_count,
            evictable_count: (evictable_bodies.len() + evictable_media.len()) as i64,
        })
    }

    /// List all conversations.
    pub fn list_conversations(&self) -> Result<Vec<ConversationFfi>, ChatStorageError> {
        let convs = self
            .inner
            .db()
            .list_all_conversations()
            .map_err(|e| ChatStorageError::from(cs_core::Error::Storage(e)))?;

        Ok(convs
            .into_iter()
            .map(|c| ConversationFfi {
                conversation_id: c.conversation_id,
                pinned: c.pinned,
                muted: c.muted,
                last_message_id: c.last_message_id,
                last_activity_ms: c.last_activity_ms,
                conversation_type: c.conversation_type,
                scope: c.scope,
                tenant_id: c.tenant_id,
            })
            .collect())
    }

    /// Count messages in a conversation.
    pub fn count_messages(&self, conversation_id: String) -> Result<i64, ChatStorageError> {
        self.inner
            .db()
            .count_messages(&conversation_id)
            .map_err(|e| ChatStorageError::from(cs_core::Error::Storage(e)))
    }

    /// Clear all message data (for testing / restore pre-clean).
    pub fn clear_all_data(&self) -> Result<(), ChatStorageError> {
        self.inner
            .db()
            .clear_all_message_data()
            .map_err(|e| ChatStorageError::from(cs_core::Error::Storage(e)))
    }
}
