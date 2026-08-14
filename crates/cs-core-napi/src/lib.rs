#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_arguments)]

mod error;
mod types;

use std::sync::Arc;

use cs_core::transport::kdrive_bridge::KdriveTransport;
use cs_core::transport::ChatStorageTransport;
use cs_core::{BackupReason, BackupSource, HydrationReason, SearchScope};
use napi_derive::napi;
use uuid::Uuid;

use error::to_napi_error;
use types::*;

/// ChatStorage — the main entry point for the chat-storage SDK.
///
/// Wraps `CoreImpl` and persists across calls within the Electron process.
/// Created once at app startup with a config and wrapping key.
#[napi]
pub struct ChatStorage {
    inner: Arc<cs_core::CoreImpl>,
}

#[napi]
impl ChatStorage {
    /// Create a new ChatStorage instance.
    ///
    /// @param config - Configuration object (dataDir, gatewayUrl, tenantId, etc.)
    /// @param wrappingKeyHex - 32-byte hex-encoded key (64 hex chars). Used to derive
    ///   all purpose-specific keys (archive, backup, search, local DB).
    /// @param authToken - Bearer token for gateway authentication.
    /// @param userId - User ID for gateway routing.
    #[napi(constructor)]
    pub fn new(
        config: ChatStorageConfigJs,
        wrapping_key_hex: String,
        auth_token: String,
        user_id: String,
    ) -> Result<Self, napi::Error> {
        let key_bytes = hex::decode(&wrapping_key_hex)
            .map_err(|e| error::invalid_input(format!("invalid wrapping_key hex: {}", e)))?;
        if key_bytes.len() != 32 {
            return Err(error::invalid_input(format!(
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

        let core = cs_core::CoreImpl::new(cfg, key, transport).map_err(to_napi_error)?;

        Ok(Self {
            inner: Arc::new(core),
        })
    }

    /// Send a text message to a conversation.
    #[napi(js_name = "sendText")]
    pub fn send_text(
        &self,
        conversation_id: String,
        text: String,
        reply_to: Option<String>,
    ) -> Result<SendTextResultJs, napi::Error> {
        let conv_id = Uuid::parse_str(&conversation_id)
            .map_err(|e| error::invalid_input(format!("invalid conversation_id: {}", e)))?;
        let reply = reply_to
            .map(|r| Uuid::parse_str(&r))
            .transpose()
            .map_err(|e| error::invalid_input(format!("invalid reply_to: {}", e)))?;

        let msg_id = self
            .inner
            .send_text(conv_id, &text, reply)
            .map_err(to_napi_error)?;

        Ok(SendTextResultJs {
            client_message_id: msg_id.0.to_string(),
        })
    }

    /// Ingest remote messages from the delivery store via the gateway.
    #[napi(js_name = "ingestRemote")]
    pub fn ingest_remote(
        &self,
        conversation_id: String,
        after_cursor: Option<String>,
    ) -> Result<IngestResultJs, napi::Error> {
        let conv_id = Uuid::parse_str(&conversation_id)
            .map_err(|e| error::invalid_input(format!("invalid conversation_id: {}", e)))?;

        let cursor = after_cursor.map(cs_core::DeliveryCursor);

        let result = self
            .inner
            .ingest_remote_messages(conv_id, cursor)
            .map_err(to_napi_error)?;

        Ok(IngestResultJs {
            new_count: result.new_count as i64,
            updated_count: result.updated_count as i64,
            duplicate_count: result.duplicate_count as i64,
            next_cursor: result.next_cursor.map(|c| c.0),
        })
    }

    /// Execute a search query.
    #[napi]
    pub fn search(
        &self,
        query: SearchQueryJs,
        scope: SearchScopeJs,
    ) -> Result<Vec<SearchResultJs>, napi::Error> {
        let q = query.into_rust()?;
        let s = match scope {
            SearchScopeJs::LocalOnly => SearchScope::LocalOnly,
            SearchScopeJs::IncludeCold => SearchScope::IncludeCold,
        };

        let results = self.inner.search(&q, s).map_err(to_napi_error)?;

        Ok(results
            .into_iter()
            .map(|r| SearchResultJs {
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
    #[napi(js_name = "hydrateMessage")]
    pub fn hydrate_message(
        &self,
        message_id: String,
        reason: HydrationReasonJs,
    ) -> Result<HydratedMessageJs, napi::Error> {
        let msg_id = Uuid::parse_str(&message_id)
            .map_err(|e| error::invalid_input(format!("invalid message_id: {}", e)))?;
        let r = match reason {
            HydrationReasonJs::UserTap => HydrationReason::UserTap,
            HydrationReasonJs::ScrollBack => HydrationReason::ScrollBack,
            HydrationReasonJs::SearchHit => HydrationReason::SearchHit,
        };

        let msg = self
            .inner
            .hydrate_message(msg_id, r)
            .map_err(to_napi_error)?;

        Ok(HydratedMessageJs {
            message_id: msg.message_id.to_string(),
            conversation_id: msg.conversation_id.to_string(),
            sender_id: msg.sender_id,
            created_at_ms: msg.created_at_ms,
            text_content: msg.text_content,
            media_assets: msg
                .media_assets
                .into_iter()
                .map(|a| MediaAssetRefJs {
                    asset_id: a.asset_id,
                    mime_type: a.mime_type,
                    node_id: a.node_id,
                    version_id: a.version_id,
                })
                .collect(),
        })
    }

    /// Run an incremental backup.
    #[napi(js_name = "runBackup")]
    pub fn run_backup(&self, reason: BackupReasonJs) -> Result<BackupResultJs, napi::Error> {
        let r = match reason {
            BackupReasonJs::Scheduled => BackupReason::Scheduled,
            BackupReasonJs::UserInitiated => BackupReason::UserInitiated,
            BackupReasonJs::PreMigration => BackupReason::PreMigration,
        };

        let result = self.inner.run_incremental_backup(r).map_err(to_napi_error)?;

        Ok(BackupResultJs {
            segments_built: result.segments_built as i64,
            segments_uploaded: result.segments_uploaded as i64,
            manifest_generation: result.manifest_generation as i64,
            bytes_uploaded: result.bytes_uploaded as i64,
        })
    }

    /// Enforce the storage budget by evicting cold data.
    #[napi(js_name = "enforceBudget")]
    pub fn enforce_budget(
        &self,
        reason: StoragePressureReasonJs,
    ) -> Result<OffloadResultJs, napi::Error> {
        let r = match reason {
            StoragePressureReasonJs::BudgetThreshold => {
                cs_core::StoragePressureReason::BudgetThreshold
            }
            StoragePressureReasonJs::DiskLow => cs_core::StoragePressureReason::DiskLow,
            StoragePressureReasonJs::UserInitiated => {
                cs_core::StoragePressureReason::UserInitiated
            }
        };

        let result = self
            .inner
            .enforce_storage_budget(r)
            .map_err(to_napi_error)?;

        Ok(OffloadResultJs {
            messages_offloaded: result.messages_offloaded as i64,
            media_offloaded: result.media_offloaded as i64,
            bytes_freed: result.bytes_freed as i64,
        })
    }

    /// Restore from a backup source.
    #[napi(js_name = "restoreFromBackup")]
    pub fn restore_from_backup(&self, source: BackupSourceJs) -> Result<RestoreResultJs, napi::Error> {
        let src = match source {
            BackupSourceJs::KdriveGateway => BackupSource::KdriveGateway,
            BackupSourceJs::ICloud => BackupSource::ICloud,
            BackupSourceJs::AndroidBackup => BackupSource::AndroidBackup,
            BackupSourceJs::ZkObjectFabric { bucket, prefix } => {
                BackupSource::ZkObjectFabric { bucket, prefix }
            }
        };

        let result = self
            .inner
            .restore_from_backup(src)
            .map_err(to_napi_error)?;

        Ok(RestoreResultJs {
            conversations_restored: result.conversations_restored as i64,
            messages_restored: result.messages_restored as i64,
            media_restored: result.media_restored as i64,
            search_indexes_rebuilt: result.search_indexes_rebuilt as i64,
        })
    }

    /// Register a device for the current account.
    #[napi(js_name = "registerDevice")]
    pub fn register_device(&self, account_token: String) -> Result<DeviceRegistrationJs, napi::Error> {
        let result = self
            .inner
            .register_device(&account_token)
            .map_err(to_napi_error)?;

        Ok(DeviceRegistrationJs {
            device_id: result.device_id,
            registered: result.registered,
        })
    }

    /// Send a media message (encrypts + uploads the file).
    #[napi(js_name = "sendMedia")]
    pub fn send_media(
        &self,
        conversation_id: String,
        file_path: String,
        caption: Option<String>,
    ) -> Result<SendMediaResultJs, napi::Error> {
        let conv_id = Uuid::parse_str(&conversation_id)
            .map_err(|e| error::invalid_input(format!("invalid conversation_id: {}", e)))?;
        let path = std::path::Path::new(&file_path);

        let result = self
            .inner
            .send_media(conv_id, path, caption.as_deref())
            .map_err(to_napi_error)?;

        Ok(SendMediaResultJs {
            message_id: result.message_id.to_string(),
            asset_id: result.asset_id,
            node_id: result.node_id,
            version_id: result.version_id,
        })
    }

    /// Create and insert a conversation into the local store.
    #[napi(js_name = "seedConversation")]
    pub fn seed_conversation(
        &self,
        conversation_id: String,
        title: Option<String>,
        scope: Option<String>,
    ) -> Result<(), napi::Error> {
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
            .map_err(|e| to_napi_error(cs_core::Error::Storage(e)))?;

        Ok(())
    }

    /// Get the message timeline for a conversation (newest first).
    #[napi(js_name = "getTimeline")]
    pub fn get_timeline(
        &self,
        conversation_id: String,
        limit: Option<i64>,
        before_ms: Option<i64>,
    ) -> Result<Vec<TimelineEntryJs>, napi::Error> {
        let lim = limit.unwrap_or(50) as usize;
        let timeline = self
            .inner
            .db()
            .fetch_timeline(&conversation_id, lim, before_ms)
            .map_err(|e| to_napi_error(cs_core::Error::Storage(e)))?;

        let mut entries = Vec::with_capacity(timeline.len());
        for row in timeline {
            let body = self.inner.db().fetch_body(&row.message_id).ok().flatten();
            entries.push(TimelineEntryJs {
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
    #[napi(js_name = "getStorageStats")]
    pub fn get_storage_stats(&self) -> Result<StorageStatsJs, napi::Error> {
        let db = self.inner.db();

        let db_size = db
            .db_size_bytes()
            .map_err(|e| to_napi_error(cs_core::Error::Storage(e)))?;

        let conn = db
            .read()
            .map_err(|e| to_napi_error(cs_core::Error::Storage(e)))?;

        let message_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM message_skeleton WHERE deleted_at_ms IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(|e| to_napi_error(cs_core::Error::Storage(e.into())))?;

        let media_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM media_asset", [], |row| row.get(0))
            .map_err(|e| to_napi_error(cs_core::Error::Storage(e.into())))?;

        let conversation_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM conversation", [], |row| row.get(0))
            .map_err(|e| to_napi_error(cs_core::Error::Storage(e.into())))?;

        let evictable_bodies = db
            .fetch_evictable_bodies(1000)
            .map_err(|e| to_napi_error(cs_core::Error::Storage(e)))?;
        let evictable_media = db
            .fetch_evictable_media(1000)
            .map_err(|e| to_napi_error(cs_core::Error::Storage(e)))?;

        Ok(StorageStatsJs {
            db_size_bytes: db_size as i64,
            message_count,
            media_count,
            conversation_count,
            evictable_count: (evictable_bodies.len() + evictable_media.len()) as i64,
        })
    }

    /// List all conversations.
    #[napi(js_name = "listConversations")]
    pub fn list_conversations(&self) -> Result<Vec<ConversationJs>, napi::Error> {
        let convs = self
            .inner
            .db()
            .list_all_conversations()
            .map_err(|e| to_napi_error(cs_core::Error::Storage(e)))?;

        Ok(convs
            .into_iter()
            .map(|c| ConversationJs {
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
    #[napi(js_name = "countMessages")]
    pub fn count_messages(&self, conversation_id: String) -> Result<i64, napi::Error> {
        self.inner
            .db()
            .count_messages(&conversation_id)
            .map_err(|e| to_napi_error(cs_core::Error::Storage(e)))
    }

    /// Clear all message data (for testing / restore pre-clean).
    #[napi(js_name = "clearAllData")]
    pub fn clear_all_data(&self) -> Result<(), napi::Error> {
        self.inner
            .db()
            .clear_all_message_data()
            .map_err(|e| to_napi_error(cs_core::Error::Storage(e)))
    }
}
