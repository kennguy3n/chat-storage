//! Message processor — ingest, persist, and manage outgoing messages.
//!
//! The ingest / outbox pipeline:
//!
//! * The library consumes **already-decrypted** MLS application
//!   messages (this module's `IngestedMessage`).
//! * Idempotency is keyed by `message_id` — re-ingesting the same
//!   message must be a no-op.
//! * The outbox carries client-originated text sends until MLS
//!   delivery confirms. `OutboxEntry::client_message_id` is a UUID
//!   v7 so monotonic ordering survives crashes.

use std::collections::HashSet;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::formats::media_descriptor::MediaDescriptor;
use crate::local_store::state_machines::{
    ArchiveState, BackupState, BodyState, MediaState, StateTransitionError,
};
use crate::local_store::{LocalStoreDb, MessageKind, MessageSkeleton, StorageError};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors returned by the [`MessageProcessor`] / [`MessagePersister`].
#[derive(Debug, thiserror::Error)]
pub enum ProcessorError {
    /// The ingested message failed validation.
    #[error("invalid message: {0}")]
    InvalidMessage(String),

    /// The message has already been ingested.
    #[error("duplicate message")]
    DuplicateMessage,

    /// A storage-layer call failed.
    #[error("storage: {0}")]
    StorageError(String),

    /// A `rusqlite` call failed inside [`MessagePersister`].
    #[error("db: {0}")]
    Db(#[from] StorageError),

    /// A body-state transition rejected by the state machine.
    #[error("illegal state transition: {0}")]
    IllegalTransition(#[from] StateTransitionError),
}

impl From<rusqlite::Error> for ProcessorError {
    fn from(e: rusqlite::Error) -> Self {
        ProcessorError::Db(StorageError::from(e))
    }
}

impl From<ProcessorError> for crate::message::MessageError {
    fn from(e: ProcessorError) -> Self {
        crate::message::MessageError::Custom(e.to_string())
    }
}

impl From<ProcessorError> for crate::Error {
    fn from(e: ProcessorError) -> Self {
        crate::Error::Message(e.into())
    }
}

// ---------------------------------------------------------------------------
// Ingest pipeline types
// ---------------------------------------------------------------------------

/// MLS-decrypted application message, ready to be persisted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestedMessage {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub text_content: Option<String>,
    pub media_descriptors: Vec<MediaDescriptor>,
    pub reply_to: Option<String>,
}

/// Outbox-entry lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxStatus {
    Pending,
    Sending,
    Sent,
    Failed,
}

/// Pending text/media send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub client_message_id: Uuid,
    pub conversation_id: Uuid,
    pub text_content: String,
    pub media_asset_id: Option<String>,
    pub reply_to: Option<String>,
    pub created_at_ms: i64,
    pub status: OutboxStatus,
}

/// Message processor — pure validators and outbox-entry factory.
#[derive(Debug, Default)]
pub struct MessageProcessor {
    _private: (),
}

impl MessageProcessor {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Create an outbox entry for an outgoing text message.
    pub fn create_outbox_entry(
        conversation_id: Uuid,
        text: &str,
        reply_to: Option<Uuid>,
    ) -> OutboxEntry {
        OutboxEntry {
            client_message_id: Uuid::now_v7(),
            conversation_id,
            text_content: text.to_string(),
            media_asset_id: None,
            reply_to: reply_to.map(|u| u.to_string()),
            created_at_ms: now_ms(),
            status: OutboxStatus::Pending,
        }
    }

    /// Create an outbox entry for an outgoing media message.
    pub fn create_media_outbox_entry(
        conversation_id: Uuid,
        asset_id: &str,
        caption: Option<&str>,
    ) -> OutboxEntry {
        OutboxEntry {
            client_message_id: Uuid::now_v7(),
            conversation_id,
            text_content: caption.unwrap_or("").to_string(),
            media_asset_id: Some(asset_id.to_string()),
            reply_to: None,
            created_at_ms: now_ms(),
            status: OutboxStatus::Pending,
        }
    }

    /// Validate that `msg` has the minimum fields required to be persisted.
    pub fn validate_ingest(msg: &IngestedMessage) -> Result<(), ProcessorError> {
        if msg.message_id.is_empty() {
            return Err(ProcessorError::InvalidMessage(
                "message_id must not be empty".into(),
            ));
        }
        if msg.conversation_id.is_empty() {
            return Err(ProcessorError::InvalidMessage(
                "conversation_id must not be empty".into(),
            ));
        }
        if msg.sender_id.is_empty() {
            return Err(ProcessorError::InvalidMessage(
                "sender_id must not be empty".into(),
            ));
        }
        if msg.created_at_ms <= 0 {
            return Err(ProcessorError::InvalidMessage(format!(
                "created_at_ms must be positive (got {})",
                msg.created_at_ms
            )));
        }
        if msg.text_content.is_none() && msg.media_descriptors.is_empty() {
            return Err(ProcessorError::InvalidMessage(
                "message has neither text nor media".into(),
            ));
        }
        if let Some(text) = &msg.text_content {
            if text.is_empty() {
                return Err(ProcessorError::InvalidMessage(
                    "text_content must not be empty when present".into(),
                ));
            }
        }
        Ok(())
    }

    /// Backwards-compatible alias for `validate_ingest`.
    pub fn validate(msg: &IngestedMessage) -> Result<(), crate::message::MessageError> {
        Self::validate_ingest(msg).map_err(|e| e.into())
    }

    /// Whether `message_id` has already been ingested.
    pub fn is_duplicate(message_id: &str, existing_ids: &HashSet<String>) -> bool {
        existing_ids.contains(message_id)
    }
}

/// Wall-clock millisecond timestamp.
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

// ---------------------------------------------------------------------------
// MessagePersister — DB-backed counterpart to MessageProcessor
// ---------------------------------------------------------------------------

/// DB-backed persistence helper that wires the pure validators above
/// to a [`LocalStoreDb`] connection.
///
/// The persister is intentionally stateless apart from its borrow on
/// `LocalStoreDb`: every public method acquires a transaction
/// internally, performs all writes (skeleton + body + FTS row +
/// backup-event-journal entry), and commits as a single unit so a
/// crash mid-ingest cannot leave the FTS index out of sync with the
/// skeleton table.
#[derive(Debug)]
pub struct MessagePersister<'a> {
    db: &'a LocalStoreDb,
}

impl<'a> MessagePersister<'a> {
    /// Construct a new persister against the supplied database.
    pub fn new(db: &'a LocalStoreDb) -> Self {
        Self { db }
    }

    /// Persist an MLS-decrypted [`IngestedMessage`].
    ///
    /// Inside one transaction the persister:
    ///
    /// 1. Validates the message via [`MessageProcessor::validate_ingest`].
    /// 2. Rejects duplicates (existing `message_skeleton` row).
    /// 3. Inserts the skeleton + body + FTS row + backup event journal.
    /// 4. Bumps the conversation's `last_message_id` / `last_activity_ms`.
    pub fn persist_ingested_message(&self, msg: &IngestedMessage) -> Result<(), ProcessorError> {
        MessageProcessor::validate_ingest(msg)?;
        if self.skeleton_exists(&msg.message_id)? {
            return Err(ProcessorError::DuplicateMessage);
        }

        let conn = self.db.write()?;
        conn.execute_batch("SAVEPOINT persist_ingest;")
            .map_err(|e| ProcessorError::StorageError(e.to_string()))?;
        let result = self.persist_ingested_message_inner(msg, &conn);
        match &result {
            Ok(_) => {
                conn.execute_batch("RELEASE persist_ingest;")
                    .map_err(|e| ProcessorError::StorageError(e.to_string()))?;
            }
            Err(_) => {
                let _ = conn.execute_batch("ROLLBACK TO persist_ingest; RELEASE persist_ingest;");
            }
        }
        result
    }

    fn persist_ingested_message_inner(
        &self,
        msg: &IngestedMessage,
        conn: &rusqlite::Connection,
    ) -> Result<(), ProcessorError> {
        let kind = if !msg.media_descriptors.is_empty() {
            MessageKind::Media
        } else if msg.text_content.is_some() {
            MessageKind::Text
        } else {
            return Err(ProcessorError::InvalidMessage(
                "message has neither text nor media".into(),
            ));
        };

        let initial_media_state = if !msg.media_descriptors.is_empty() {
            Some(MediaState::ThumbnailOnly)
        } else {
            None
        };
        let skel = MessageSkeleton {
            message_id: msg.message_id.clone(),
            conversation_id: msg.conversation_id.clone(),
            sender_id: msg.sender_id.clone(),
            created_at_ms: msg.created_at_ms,
            received_at_ms: now_ms(),
            kind,
            body_state: BodyState::LocalPlainAvailable,
            media_state: initial_media_state,
            archive_state: ArchiveState::NotArchived,
            backup_state: BackupState::NotBackedUp,
            reply_to: msg.reply_to.clone(),
            edited_at_ms: None,
            deleted_at_ms: None,
        };
        conn.execute(
            "INSERT OR REPLACE INTO message_skeleton
             (message_id, conversation_id, sender_id, created_at_ms, received_at_ms, kind, body_state, media_state, archive_state, backup_state, reply_to, edited_at_ms, deleted_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            rusqlite::params![
                skel.message_id,
                skel.conversation_id,
                skel.sender_id,
                skel.created_at_ms,
                skel.received_at_ms,
                skel.kind.as_str(),
                skel.body_state.to_string(),
                skel.media_state.map(|s| s.to_string()),
                skel.archive_state.to_string(),
                skel.backup_state.to_string(),
                skel.reply_to,
                skel.edited_at_ms,
                skel.deleted_at_ms,
            ],
        )
        .map_err(|e| ProcessorError::StorageError(e.to_string()))?;

        // Insert media asset rows for each descriptor
        for desc in &msg.media_descriptors {
            conn.execute(
                "INSERT OR REPLACE INTO media_asset
                 (asset_id, message_id, mime_type, bytes_total, bytes_local, media_state, wrapped_k_asset, chunk_count, merkle_root, blob_id, storage_sink)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                rusqlite::params![
                    desc.asset_id.to_string(),
                    skel.message_id,
                    desc.mime_type,
                    desc.bytes_total as i64,
                    0i64,
                    MediaState::ThumbnailOnly.to_string(),
                    desc.wrapped_k_asset,
                    desc.chunk_count as i32,
                    desc.merkle_root,
                    desc.blob_id.to_string(),
                    desc.storage_sink.clone().unwrap_or_else(|| "kchat_backend".to_string()),
                ],
            )?;
        }

        if let Some(text) = &msg.text_content {
            conn.execute(
                "INSERT OR REPLACE INTO message_body (message_id, text_content, detected_language, rich_meta)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    skel.message_id,
                    text,
                    None::<String>,
                    None::<Vec<u8>>,
                ],
            )?;
            self.insert_fts_row(
                conn,
                &skel.message_id,
                &skel.conversation_id,
                &skel.sender_id,
                skel.created_at_ms,
                text,
            )?;

            // Index fuzzy tokens (trigrams/bigrams)
            let tokens = crate::search::tokenizer::tokenize(text);
            for (token, script) in &tokens {
                let grams = match script {
                    crate::search::tokenizer::Script::Hani
                    | crate::search::tokenizer::Script::Hira
                    | crate::search::tokenizer::Script::Kana
                    | crate::search::tokenizer::Script::Hang => {
                        crate::search::tokenizer::bigrams(token)
                    }
                    _ => crate::search::tokenizer::trigrams(token),
                };
                for gram in &grams {
                    let _ = conn.execute(
                        "INSERT OR IGNORE INTO search_fuzzy (token, script, message_id) VALUES (?1, ?2, ?3)",
                        rusqlite::params![gram, script.code(), skel.message_id],
                    );
                }
            }
        }

        // Bump conversation last_message_id / last_activity_ms
        let _ = conn.execute(
            "UPDATE conversation SET last_message_id = ?2, last_activity_ms = ?3
             WHERE conversation_id = ?1",
            rusqlite::params![skel.conversation_id, skel.message_id, skel.created_at_ms],
        );

        // Write backup event journal entry
        let payload = encode_event_payload(
            &skel.message_id,
            &skel.conversation_id,
            &skel.sender_id,
            skel.created_at_ms,
        );
        conn.execute(
            "INSERT INTO backup_event_journal (event_type, conversation_id, message_id, payload, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                "message_received",
                skel.conversation_id,
                skel.message_id,
                payload,
                now_ms(),
            ],
        )?;

        Ok(())
    }

    /// Persist a client-originated outbox entry.
    ///
    /// The outbox entry's `client_message_id` becomes the skeleton's
    /// `message_id`. `body_state` is set to `LocalPlainAvailable`.
    pub fn persist_outbox_entry(&self, entry: &OutboxEntry) -> Result<Uuid, ProcessorError> {
        if entry.text_content.is_empty() && entry.media_asset_id.is_none() {
            return Err(ProcessorError::InvalidMessage(
                "outbox must have text or media".into(),
            ));
        }
        let mid = entry.client_message_id.to_string();
        if self.skeleton_exists(&mid)? {
            return Err(ProcessorError::DuplicateMessage);
        }

        let conn = self.db.write()?;
        conn.execute_batch("SAVEPOINT persist_outbox;")
            .map_err(|e| ProcessorError::StorageError(e.to_string()))?;
        let result = self.persist_outbox_entry_inner(entry, &conn);
        match &result {
            Ok(_) => {
                conn.execute_batch("RELEASE persist_outbox;")
                    .map_err(|e| ProcessorError::StorageError(e.to_string()))?;
            }
            Err(_) => {
                let _ = conn.execute_batch("ROLLBACK TO persist_outbox; RELEASE persist_outbox;");
            }
        }
        result?;
        Ok(entry.client_message_id)
    }

    fn persist_outbox_entry_inner(
        &self,
        entry: &OutboxEntry,
        conn: &rusqlite::Connection,
    ) -> Result<(), ProcessorError> {
        let mid = entry.client_message_id.to_string();
        let conv = entry.conversation_id.to_string();

        // Insert outbox row
        conn.execute(
            "INSERT OR REPLACE INTO outbox (client_message_id, conversation_id, text_content, media_asset_id, created_at_ms, sent, sent_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL)",
            rusqlite::params![
                mid,
                conv,
                if entry.text_content.is_empty() { None } else { Some(&entry.text_content) },
                entry.media_asset_id,
                entry.created_at_ms,
            ],
        )
        .map_err(|e| ProcessorError::StorageError(e.to_string()))?;

        // Insert skeleton + body for text content
        if !entry.text_content.is_empty() {
            let skel = MessageSkeleton {
                message_id: mid.clone(),
                conversation_id: conv.clone(),
                sender_id: "self".into(),
                created_at_ms: entry.created_at_ms,
                received_at_ms: entry.created_at_ms,
                kind: MessageKind::Text,
                body_state: BodyState::LocalPlainAvailable,
                media_state: None,
                archive_state: ArchiveState::NotArchived,
                backup_state: BackupState::NotBackedUp,
                reply_to: entry.reply_to.clone(),
                edited_at_ms: None,
                deleted_at_ms: None,
            };
            conn.execute(
                "INSERT OR REPLACE INTO message_skeleton
                 (message_id, conversation_id, sender_id, created_at_ms, received_at_ms, kind, body_state, media_state, archive_state, backup_state, reply_to, edited_at_ms, deleted_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                rusqlite::params![
                    skel.message_id,
                    skel.conversation_id,
                    skel.sender_id,
                    skel.created_at_ms,
                    skel.received_at_ms,
                    skel.kind.as_str(),
                    skel.body_state.to_string(),
                    skel.media_state.map(|s| s.to_string()),
                    skel.archive_state.to_string(),
                    skel.backup_state.to_string(),
                    skel.reply_to,
                    skel.edited_at_ms,
                    skel.deleted_at_ms,
                ],
            )
            .map_err(|e| ProcessorError::StorageError(e.to_string()))?;

            conn.execute(
                "INSERT OR REPLACE INTO message_body (message_id, text_content, detected_language, rich_meta)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    mid,
                    entry.text_content,
                    None::<String>,
                    None::<Vec<u8>>,
                ],
            )?;
            self.insert_fts_row(
                conn,
                &mid,
                &conv,
                "self",
                entry.created_at_ms,
                &entry.text_content,
            )?;

            // Index fuzzy tokens
            let tokens = crate::search::tokenizer::tokenize(&entry.text_content);
            for (token, script) in &tokens {
                let grams = match script {
                    crate::search::tokenizer::Script::Hani
                    | crate::search::tokenizer::Script::Hira
                    | crate::search::tokenizer::Script::Kana
                    | crate::search::tokenizer::Script::Hang => {
                        crate::search::tokenizer::bigrams(token)
                    }
                    _ => crate::search::tokenizer::trigrams(token),
                };
                for gram in &grams {
                    let _ = conn.execute(
                        "INSERT OR IGNORE INTO search_fuzzy (token, script, message_id) VALUES (?1, ?2, ?3)",
                        rusqlite::params![gram, script.code(), mid],
                    );
                }
            }

            // Bump conversation
            let _ = conn.execute(
                "UPDATE conversation SET last_message_id = ?2, last_activity_ms = ?3
                 WHERE conversation_id = ?1",
                rusqlite::params![conv, mid, entry.created_at_ms],
            );

            // Backup event journal
            let payload = encode_event_payload(&mid, &conv, "self", entry.created_at_ms);
            conn.execute(
                "INSERT INTO backup_event_journal (event_type, conversation_id, message_id, payload, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    "outbox_pending",
                    conv,
                    mid,
                    payload,
                    now_ms(),
                ],
            )?;
        }

        Ok(())
    }

    /// Whether `client_message_id` exists in `message_skeleton`.
    pub fn check_duplicate(&self, client_message_id: &str) -> Result<bool, ProcessorError> {
        self.skeleton_exists(client_message_id)
    }

    /// Mark an outbox entry as sent.
    pub fn mark_sent(&self, client_message_id: &str) -> Result<(), ProcessorError> {
        let conn = self.db.write()?;
        let lookup: Option<(String, String, i64)> = conn
            .query_row(
                "SELECT conversation_id, sender_id, created_at_ms
                 FROM message_skeleton WHERE message_id = ?1",
                rusqlite::params![client_message_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|e| ProcessorError::StorageError(e.to_string()))?;
        let (conv, sender, created_at) = match lookup {
            Some(v) => v,
            None => {
                return Err(ProcessorError::InvalidMessage(format!(
                    "no outbox entry with client_message_id={client_message_id}"
                )));
            }
        };

        // Update outbox sent flag
        conn.execute(
            "UPDATE outbox SET sent = 1, sent_at_ms = ?2 WHERE client_message_id = ?1",
            rusqlite::params![client_message_id, now_ms()],
        )
        .map_err(|e| ProcessorError::StorageError(e.to_string()))?;

        let payload = encode_event_payload(client_message_id, &conv, &sender, created_at);
        conn.execute(
            "INSERT INTO backup_event_journal (event_type, conversation_id, message_id, payload, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                "outbox_sent",
                conv,
                client_message_id,
                payload,
                now_ms(),
            ],
        )?;
        Ok(())
    }

    /// Replace the text body of an existing local-plain message and
    /// keep the FTS index in sync.
    pub fn edit_message(&self, message_id: &str, new_text: &str) -> Result<(), ProcessorError> {
        if new_text.is_empty() {
            return Err(ProcessorError::InvalidMessage(
                "edit text_content must not be empty".into(),
            ));
        }
        let skel = self.db.get_message_skeleton(message_id)?.ok_or_else(|| {
            ProcessorError::InvalidMessage(format!("no message with id={message_id}"))
        })?;
        if skel.body_state != BodyState::LocalPlainAvailable {
            return Err(ProcessorError::InvalidMessage(format!(
                "edit requires body_state=local_plain_available, found {}",
                skel.body_state
            )));
        }

        let edited_at_ms = now_ms();
        let conn = self.db.write()?;
        conn.execute_batch("SAVEPOINT edit_message;")
            .map_err(|e| ProcessorError::StorageError(e.to_string()))?;
        let result = self.edit_message_inner(&skel, new_text, edited_at_ms, &conn);
        match &result {
            Ok(_) => {
                conn.execute_batch("RELEASE edit_message;")
                    .map_err(|e| ProcessorError::StorageError(e.to_string()))?;
            }
            Err(_) => {
                let _ = conn.execute_batch("ROLLBACK TO edit_message; RELEASE edit_message;");
            }
        }
        result
    }

    fn edit_message_inner(
        &self,
        skel: &MessageSkeleton,
        new_text: &str,
        edited_at_ms: i64,
        conn: &rusqlite::Connection,
    ) -> Result<(), ProcessorError> {
        conn.execute(
            "UPDATE message_body SET text_content = ?2 WHERE message_id = ?1",
            rusqlite::params![skel.message_id, new_text],
        )?;
        conn.execute(
            "UPDATE message_skeleton SET edited_at_ms = ?2 WHERE message_id = ?1",
            rusqlite::params![skel.message_id, edited_at_ms],
        )?;
        conn.execute(
            "DELETE FROM search_fts WHERE message_id = ?1",
            rusqlite::params![skel.message_id],
        )?;
        self.insert_fts_row(
            conn,
            &skel.message_id,
            &skel.conversation_id,
            &skel.sender_id,
            skel.created_at_ms,
            new_text,
        )?;
        // Re-index fuzzy
        conn.execute(
            "DELETE FROM search_fuzzy WHERE message_id = ?1",
            rusqlite::params![skel.message_id],
        )?;
        let tokens = crate::search::tokenizer::tokenize(new_text);
        for (token, script) in &tokens {
            let grams = match script {
                crate::search::tokenizer::Script::Hani
                | crate::search::tokenizer::Script::Hira
                | crate::search::tokenizer::Script::Kana
                | crate::search::tokenizer::Script::Hang => {
                    crate::search::tokenizer::bigrams(token)
                }
                _ => crate::search::tokenizer::trigrams(token),
            };
            for gram in &grams {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO search_fuzzy (token, script, message_id) VALUES (?1, ?2, ?3)",
                    rusqlite::params![gram, script.code(), skel.message_id],
                );
            }
        }

        let payload = encode_edit_event_payload(&skel.message_id, edited_at_ms);
        conn.execute(
            "INSERT INTO backup_event_journal (event_type, conversation_id, message_id, payload, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                "message_edited",
                skel.conversation_id,
                skel.message_id,
                payload,
                edited_at_ms,
            ],
        )?;
        Ok(())
    }

    /// Soft-delete a message locally (`delete-for-me`).
    pub fn delete_for_me(&self, message_id: &str) -> Result<(), ProcessorError> {
        self.delete_inner(message_id, DeleteScope::ForMe)
    }

    /// Tombstone a message for everyone (`delete-for-everyone`).
    pub fn delete_for_everyone(&self, message_id: &str) -> Result<(), ProcessorError> {
        self.delete_inner(message_id, DeleteScope::ForEveryone)
    }

    fn delete_inner(&self, message_id: &str, scope: DeleteScope) -> Result<(), ProcessorError> {
        let skel = self.db.get_message_skeleton(message_id)?.ok_or_else(|| {
            ProcessorError::InvalidMessage(format!("no message with id={message_id}"))
        })?;
        let target = match scope {
            DeleteScope::ForMe => BodyState::DeletedForMe,
            DeleteScope::ForEveryone => BodyState::DeletedForEveryone,
        };
        BodyState::try_transition(skel.body_state, target)?;

        let deleted_at_ms = now_ms();
        let conn = self.db.write()?;
        conn.execute_batch("SAVEPOINT delete_message;")
            .map_err(|e| ProcessorError::StorageError(e.to_string()))?;
        let result = self.delete_inner_tx(&skel, scope, target, deleted_at_ms, &conn);
        match &result {
            Ok(_) => {
                conn.execute_batch("RELEASE delete_message;")
                    .map_err(|e| ProcessorError::StorageError(e.to_string()))?;
            }
            Err(_) => {
                let _ = conn.execute_batch("ROLLBACK TO delete_message; RELEASE delete_message;");
            }
        }
        result
    }

    fn delete_inner_tx(
        &self,
        skel: &MessageSkeleton,
        scope: DeleteScope,
        target: BodyState,
        deleted_at_ms: i64,
        conn: &rusqlite::Connection,
    ) -> Result<(), ProcessorError> {
        conn.execute(
            "UPDATE message_skeleton SET deleted_at_ms = ?2, body_state = ?3
             WHERE message_id = ?1",
            rusqlite::params![skel.message_id, deleted_at_ms, target.to_string()],
        )?;
        conn.execute(
            "DELETE FROM search_fts WHERE message_id = ?1",
            rusqlite::params![skel.message_id],
        )?;
        conn.execute(
            "DELETE FROM search_fuzzy WHERE message_id = ?1",
            rusqlite::params![skel.message_id],
        )?;
        if matches!(scope, DeleteScope::ForEveryone) {
            conn.execute(
                "DELETE FROM message_body WHERE message_id = ?1",
                rusqlite::params![skel.message_id],
            )?;
        }
        let payload = encode_delete_event_payload(&skel.message_id, scope.label());
        conn.execute(
            "INSERT INTO backup_event_journal (event_type, conversation_id, message_id, payload, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                "message_deleted",
                skel.conversation_id,
                skel.message_id,
                payload,
                deleted_at_ms,
            ],
        )?;

        // Emit media_deleted events for attached assets — query directly on conn
        let mut stmt = conn.prepare("SELECT asset_id FROM media_asset WHERE message_id = ?1")?;
        let asset_ids: Vec<String> = stmt
            .query_map(rusqlite::params![skel.message_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        for asset_id in &asset_ids {
            let media_payload = encode_media_delete_event_payload(asset_id, &skel.message_id);
            conn.execute(
                "INSERT INTO backup_event_journal (event_type, conversation_id, message_id, payload, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    "media_deleted",
                    skel.conversation_id,
                    skel.message_id,
                    media_payload,
                    deleted_at_ms,
                ],
            )?;
        }
        Ok(())
    }

    fn skeleton_exists(&self, message_id: &str) -> Result<bool, ProcessorError> {
        let conn = self.db.read()?;
        let count: i64 = conn.query_row(
            "SELECT count(*) FROM message_skeleton WHERE message_id = ?1",
            rusqlite::params![message_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    fn insert_fts_row(
        &self,
        conn: &rusqlite::Connection,
        message_id: &str,
        conversation_id: &str,
        sender_id: &str,
        created_at_ms: i64,
        text: &str,
    ) -> Result<(), ProcessorError> {
        conn.execute(
            "INSERT INTO search_fts(
                message_id, conversation_id, sender_id,
                created_at_ms, text_content
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![message_id, conversation_id, sender_id, created_at_ms, text],
        )
        .map_err(|e| ProcessorError::StorageError(e.to_string()))?;
        Ok(())
    }
}

/// Whether a delete affects only the local user or everyone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeleteScope {
    ForMe,
    ForEveryone,
}

impl DeleteScope {
    fn label(self) -> &'static str {
        match self {
            DeleteScope::ForMe => "for_me",
            DeleteScope::ForEveryone => "for_everyone",
        }
    }
}

// ---------------------------------------------------------------------------
// CBOR payload encoders for backup event journal
// ---------------------------------------------------------------------------

/// Encode a `[message_id, conversation_id, sender_id, created_at_ms]`
/// CBOR array.
fn encode_event_payload(
    message_id: &str,
    conversation_id: &str,
    sender_id: &str,
    created_at_ms: i64,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.push(0x84); // array of 4
    push_cbor_text(&mut out, message_id);
    push_cbor_text(&mut out, conversation_id);
    push_cbor_text(&mut out, sender_id);
    push_cbor_int(&mut out, created_at_ms);
    out
}

/// `{ "message_id": <str>, "edited_at_ms": <int> }`
fn encode_edit_event_payload(message_id: &str, edited_at_ms: i64) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.push(0xa2); // map of 2
    push_cbor_text(&mut out, "message_id");
    push_cbor_text(&mut out, message_id);
    push_cbor_text(&mut out, "edited_at_ms");
    push_cbor_int(&mut out, edited_at_ms);
    out
}

/// `{ "message_id": <str>, "scope": "for_me" | "for_everyone" }`
fn encode_delete_event_payload(message_id: &str, scope: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.push(0xa2);
    push_cbor_text(&mut out, "message_id");
    push_cbor_text(&mut out, message_id);
    push_cbor_text(&mut out, "scope");
    push_cbor_text(&mut out, scope);
    out
}

/// `{ "asset_id": <str>, "message_id": <str> }`
fn encode_media_delete_event_payload(asset_id: &str, message_id: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.push(0xa2);
    push_cbor_text(&mut out, "asset_id");
    push_cbor_text(&mut out, asset_id);
    push_cbor_text(&mut out, "message_id");
    push_cbor_text(&mut out, message_id);
    out
}

fn push_cbor_text(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if len < 24 {
        out.push(0x60 | len as u8);
    } else if len <= u8::MAX as usize {
        out.push(0x78);
        out.push(len as u8);
    } else if len <= u16::MAX as usize {
        out.push(0x79);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(0x7a);
        out.extend_from_slice(&(len as u32).to_be_bytes());
    }
    out.extend_from_slice(bytes);
}

fn push_cbor_int(out: &mut Vec<u8>, v: i64) {
    if v >= 0 {
        push_cbor_uint(out, v as u64);
    } else {
        let n = (-(v + 1)) as u64;
        push_cbor_uint_with_major(out, 1, n);
    }
}

fn push_cbor_uint(out: &mut Vec<u8>, v: u64) {
    push_cbor_uint_with_major(out, 0, v);
}

fn push_cbor_uint_with_major(out: &mut Vec<u8>, major: u8, v: u64) {
    let m = major << 5;
    if v < 24 {
        out.push(m | v as u8);
    } else if v <= u8::MAX as u64 {
        out.push(m | 24);
        out.push(v as u8);
    } else if v <= u16::MAX as u64 {
        out.push(m | 25);
        out.extend_from_slice(&(v as u16).to_be_bytes());
    } else if v <= u32::MAX as u64 {
        out.push(m | 26);
        out.extend_from_slice(&(v as u32).to_be_bytes());
    } else {
        out.push(m | 27);
        out.extend_from_slice(&v.to_be_bytes());
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_store::{Conversation, LocalStoreDb};

    fn sample_message() -> IngestedMessage {
        IngestedMessage {
            message_id: "msg-test-1".to_string(),
            conversation_id: "conv-test".to_string(),
            sender_id: "user-a".to_string(),
            created_at_ms: 1_700_000_000_000,
            text_content: Some("Hello world from KChat".to_string()),
            media_descriptors: vec![],
            reply_to: None,
        }
    }

    fn seed_db() -> LocalStoreDb {
        let db = LocalStoreDb::open_in_memory(&[0x42u8; 32]).unwrap();
        let conv = Conversation::legacy("conv-test", None, false, false, None, 1_700_000_000_000);
        db.insert_conversation(&conv).unwrap();
        db
    }

    #[test]
    fn validate_ingest_accepts_minimal_text_message() {
        let msg = sample_message();
        MessageProcessor::validate_ingest(&msg).expect("valid");
    }

    #[test]
    fn validate_ingest_rejects_empty_message_id() {
        let mut msg = sample_message();
        msg.message_id = String::new();
        assert!(matches!(
            MessageProcessor::validate_ingest(&msg),
            Err(ProcessorError::InvalidMessage(_))
        ));
    }

    #[test]
    fn validate_ingest_rejects_empty_sender_id() {
        let mut msg = sample_message();
        msg.sender_id = String::new();
        assert!(matches!(
            MessageProcessor::validate_ingest(&msg),
            Err(ProcessorError::InvalidMessage(_))
        ));
    }

    #[test]
    fn validate_ingest_rejects_non_positive_timestamp() {
        let mut msg = sample_message();
        msg.created_at_ms = 0;
        assert!(matches!(
            MessageProcessor::validate_ingest(&msg),
            Err(ProcessorError::InvalidMessage(_))
        ));
    }

    #[test]
    fn validate_ingest_rejects_empty_text_when_no_media() {
        let mut msg = sample_message();
        msg.text_content = Some(String::new());
        assert!(matches!(
            MessageProcessor::validate_ingest(&msg),
            Err(ProcessorError::InvalidMessage(_))
        ));
    }

    #[test]
    fn validate_ingest_rejects_empty_payload() {
        let mut msg = sample_message();
        msg.text_content = None;
        msg.media_descriptors = vec![];
        assert!(matches!(
            MessageProcessor::validate_ingest(&msg),
            Err(ProcessorError::InvalidMessage(_))
        ));
    }

    #[test]
    fn is_duplicate_detects_existing() {
        let id = "msg-1".to_string();
        let mut set = HashSet::new();
        set.insert(id.clone());
        assert!(MessageProcessor::is_duplicate(&id, &set));
        assert!(!MessageProcessor::is_duplicate("msg-2", &set));
    }

    #[test]
    fn create_outbox_entry_basics() {
        let conv = Uuid::now_v7();
        let entry = MessageProcessor::create_outbox_entry(conv, "hello", None);
        assert_eq!(entry.conversation_id, conv);
        assert_eq!(entry.text_content, "hello");
        assert_eq!(entry.status, OutboxStatus::Pending);
    }

    #[test]
    fn persist_and_search() {
        let db = seed_db();
        let msg = sample_message();

        let persister = MessagePersister::new(&db);
        persister.persist_ingested_message(&msg).expect("persist");

        // Duplicate should fail
        let dup = persister.persist_ingested_message(&msg);
        assert!(matches!(dup, Err(ProcessorError::DuplicateMessage)));

        // FTS search should find it
        let results = db.search_fts("hello", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "msg-test-1");

        // Fuzzy search should find it
        let fuzzy_results = crate::search::fuzzy_search::fuzzy_search(&db, "hello", 10).unwrap();
        assert!(!fuzzy_results.is_empty());
    }

    #[test]
    fn persist_outbox_and_edit() {
        let db = seed_db();
        let conv = Uuid::now_v7();

        // Insert conversation for the outbox message
        let conv_row = Conversation::legacy(
            conv.to_string(),
            None,
            false,
            false,
            None,
            1_700_000_000_000,
        );
        db.insert_conversation(&conv_row).unwrap();

        let entry = MessageProcessor::create_outbox_entry(conv, "original text", None);
        let persister = MessagePersister::new(&db);
        let mid = persister
            .persist_outbox_entry(&entry)
            .expect("persist outbox");
        let mid_str = mid.to_string();

        // Edit the message
        persister
            .edit_message(&mid_str, "edited text")
            .expect("edit");

        // FTS should reflect the edit
        let results = db.search_fts("edited", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, mid_str);

        // Old text should no longer be in FTS
        let old_results = db.search_fts("original", 10).unwrap();
        assert!(old_results.is_empty());
    }

    #[test]
    fn delete_for_me_removes_from_search() {
        let db = seed_db();
        let msg = sample_message();
        let persister = MessagePersister::new(&db);
        persister.persist_ingested_message(&msg).expect("persist");

        // Should be in search
        assert!(!db.search_fts("hello", 10).unwrap().is_empty());

        persister.delete_for_me("msg-test-1").expect("delete");

        // Should no longer be in search
        assert!(db.search_fts("hello", 10).unwrap().is_empty());

        // Skeleton should still exist with DeletedForMe state
        let skel = db.fetch_skeleton("msg-test-1").unwrap().unwrap();
        assert_eq!(skel.body_state, BodyState::DeletedForMe);
        assert!(skel.deleted_at_ms.is_some());
    }

    #[test]
    fn delete_for_everyone_removes_body() {
        let db = seed_db();
        let msg = sample_message();
        let persister = MessagePersister::new(&db);
        persister.persist_ingested_message(&msg).expect("persist");

        // Body should exist
        assert!(db.fetch_body("msg-test-1").unwrap().is_some());

        persister.delete_for_everyone("msg-test-1").expect("delete");

        // Body should be gone
        assert!(db.fetch_body("msg-test-1").unwrap().is_none());

        // Skeleton should have DeletedForEveryone state
        let skel = db.fetch_skeleton("msg-test-1").unwrap().unwrap();
        assert_eq!(skel.body_state, BodyState::DeletedForEveryone);
    }

    #[test]
    fn mark_sent_updates_outbox() {
        let db = seed_db();
        let conv = Uuid::now_v7();
        let conv_row = Conversation::legacy(
            conv.to_string(),
            None,
            false,
            false,
            None,
            1_700_000_000_000,
        );
        db.insert_conversation(&conv_row).unwrap();

        let entry = MessageProcessor::create_outbox_entry(conv, "sending test", None);
        let persister = MessagePersister::new(&db);
        let mid = persister.persist_outbox_entry(&entry).expect("persist");
        let mid_str = mid.to_string();

        persister.mark_sent(&mid_str).expect("mark sent");

        // Verify outbox row has sent=1
        let conn = db.read().unwrap();
        let sent: i64 = conn
            .query_row(
                "SELECT sent FROM outbox WHERE client_message_id = ?1",
                rusqlite::params![mid_str],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(sent, 1);
    }
}
