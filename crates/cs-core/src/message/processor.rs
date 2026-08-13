//! Message processor — ingest, persist, and manage outgoing messages.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::formats::media_descriptor::MediaDescriptor;
use crate::local_store::{LocalStoreDb, MessageBody, MessageKind, MessageSkeleton, StorageError};
use crate::message::MessageError;

/// An ingested message (post-MLS-decrypt, pre-persistence).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestedMessage {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub text_content: Option<String>,
    pub media_descriptors: Vec<MediaDescriptor>,
    pub reply_to: Option<String>,
}

/// An outbox entry for an outgoing message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub client_message_id: Uuid,
    pub conversation_id: Uuid,
    pub text_content: Option<String>,
    pub media_asset_id: Option<String>,
    pub created_at_ms: i64,
}

/// Message processor — creates outbox entries and validates messages.
#[derive(Debug)]
pub struct MessageProcessor;

impl MessageProcessor {
    /// Create an outbox entry for an outgoing text message.
    pub fn create_outbox_entry(
        conversation_id: Uuid,
        text: &str,
        _reply_to: Option<Uuid>,
    ) -> OutboxEntry {
        OutboxEntry {
            client_message_id: Uuid::now_v7(),
            conversation_id,
            text_content: Some(text.to_string()),
            media_asset_id: None,
            created_at_ms: now_ms(),
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
            text_content: caption.map(|c| c.to_string()),
            media_asset_id: Some(asset_id.to_string()),
            created_at_ms: now_ms(),
        }
    }

    /// Validate an ingested message before persistence.
    pub fn validate(msg: &IngestedMessage) -> Result<(), MessageError> {
        if msg.message_id.is_empty() {
            return Err(MessageError::Validation("empty message_id".into()));
        }
        if msg.conversation_id.is_empty() {
            return Err(MessageError::Validation("empty conversation_id".into()));
        }
        if msg.sender_id.is_empty() {
            return Err(MessageError::Validation("empty sender_id".into()));
        }
        if msg.text_content.is_none() && msg.media_descriptors.is_empty() {
            return Err(MessageError::Validation(
                "message must have text or media".into(),
            ));
        }
        Ok(())
    }
}

/// Message persister — stores messages in the local DB.
#[derive(Debug)]
pub struct MessagePersister;

impl MessagePersister {
    /// Persist an outbox entry to the local DB.
    pub fn persist_outbox_entry(
        db: &LocalStoreDb,
        entry: &OutboxEntry,
    ) -> Result<(), StorageError> {
        let conn = db.write()?;
        conn.execute(
            "INSERT OR REPLACE INTO outbox (client_message_id, conversation_id, text_content, media_asset_id, created_at_ms, sent, sent_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL)",
            rusqlite::params![
                entry.client_message_id.to_string(),
                entry.conversation_id.to_string(),
                entry.text_content,
                entry.media_asset_id,
                entry.created_at_ms,
            ],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    /// Ingest a remote message into the local store.
    pub fn ingest_remote(db: &LocalStoreDb, msg: &IngestedMessage) -> Result<bool, StorageError> {
        let conn = db.write()?;

        // Check for duplicate and insert atomically within the same lock scope
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM message_skeleton WHERE message_id = ?1",
                rusqlite::params![msg.message_id],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if exists {
            return Ok(false); // duplicate
        }

        let kind = if msg.media_descriptors.is_empty() {
            MessageKind::Text
        } else {
            MessageKind::Media
        };

        let skeleton = MessageSkeleton {
            message_id: msg.message_id.clone(),
            conversation_id: msg.conversation_id.clone(),
            sender_id: msg.sender_id.clone(),
            created_at_ms: msg.created_at_ms,
            received_at_ms: now_ms(),
            kind,
            body_state: "local_plain_available".to_string(),
            media_state: if msg.media_descriptors.is_empty() {
                None
            } else {
                Some("thumbnail_only".to_string())
            },
            archive_state: "not_archived".to_string(),
            backup_state: "not_backed_up".to_string(),
            reply_to: msg.reply_to.clone(),
            edited_at_ms: None,
            deleted_at_ms: None,
        };

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
                skeleton.body_state,
                skeleton.media_state,
                skeleton.archive_state,
                skeleton.backup_state,
                skeleton.reply_to,
                skeleton.edited_at_ms,
                skeleton.deleted_at_ms,
            ],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        drop(conn);

        if let Some(text) = &msg.text_content {
            let body = MessageBody {
                message_id: msg.message_id.clone(),
                text_content: Some(text.clone()),
                detected_language: None,
            };
            db.insert_body(&body)?;
            db.index_fts(
                &msg.message_id,
                &msg.conversation_id,
                &msg.sender_id,
                msg.created_at_ms,
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
                    let _ = db.index_fuzzy(gram, script.code(), &msg.message_id);
                }
            }
        }

        Ok(true)
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_store::LocalStoreDb;

    #[test]
    fn test_ingest_and_search() {
        let db = LocalStoreDb::open_in_memory(&[0x42u8; 32]).unwrap();

        let msg = IngestedMessage {
            message_id: "msg-test-1".to_string(),
            conversation_id: "conv-test".to_string(),
            sender_id: "user-a".to_string(),
            created_at_ms: 1_700_000_000_000,
            text_content: Some("Hello world from KChat".to_string()),
            media_descriptors: vec![],
            reply_to: None,
        };

        MessageProcessor::validate(&msg).unwrap();
        let is_new = MessagePersister::ingest_remote(&db, &msg).unwrap();
        assert!(is_new);

        // Duplicate should return false
        let is_dup = MessagePersister::ingest_remote(&db, &msg).unwrap();
        assert!(!is_dup);

        // FTS search should find it
        let results = db.search_fts("hello", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "msg-test-1");

        // Fuzzy search should find it
        let fuzzy_results = crate::search::fuzzy_search::fuzzy_search(&db, "hello", 10).unwrap();
        assert!(!fuzzy_results.is_empty());
    }

    #[test]
    fn test_validate_rejects_empty() {
        let msg = IngestedMessage {
            message_id: "".to_string(),
            conversation_id: "conv".to_string(),
            sender_id: "user".to_string(),
            created_at_ms: 0,
            text_content: Some("hi".to_string()),
            media_descriptors: vec![],
            reply_to: None,
        };
        assert!(MessageProcessor::validate(&msg).is_err());
    }

    #[test]
    fn test_validate_rejects_no_content() {
        let msg = IngestedMessage {
            message_id: "msg-1".to_string(),
            conversation_id: "conv".to_string(),
            sender_id: "user".to_string(),
            created_at_ms: 0,
            text_content: None,
            media_descriptors: vec![],
            reply_to: None,
        };
        assert!(MessageProcessor::validate(&msg).is_err());
    }
}
