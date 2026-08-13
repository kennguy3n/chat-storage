//! SQLCipher database connection and operations.

use std::sync::Mutex;

use rusqlite::Connection;

use super::{schema::SCHEMA_SQL, StorageError};

/// The local encrypted store. Owns a write connection and a pool of
/// read-only connections.
#[derive(Debug)]
pub struct LocalStoreDb {
    write_conn: Mutex<Connection>,
}

impl LocalStoreDb {
    /// Open (or create) the encrypted database at `path` with the
    /// given 32-byte key.
    pub fn open(path: &str, key: &[u8; 32]) -> Result<Self, StorageError> {
        let conn = Connection::open(path).map_err(|e| StorageError::Database(e.to_string()))?;
        Self::init_connection(&conn, key)?;
        Ok(Self {
            write_conn: Mutex::new(conn),
        })
    }

    /// Open an in-memory encrypted database (for tests).
    pub fn open_in_memory(key: &[u8; 32]) -> Result<Self, StorageError> {
        let conn =
            Connection::open_in_memory().map_err(|e| StorageError::Database(e.to_string()))?;
        Self::init_connection(&conn, key)?;
        Ok(Self {
            write_conn: Mutex::new(conn),
        })
    }

    fn init_connection(conn: &Connection, key: &[u8; 32]) -> Result<(), StorageError> {
        let key_hex = hex::encode(key);
        conn.pragma_update(None, "key", &key_hex)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        // Production pragmas for performance and concurrency
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| StorageError::Database(e.to_string()))?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| StorageError::Database(e.to_string()))?;
        conn.pragma_update(None, "cache_size", -65536) // 64MB cache
            .map_err(|e| StorageError::Database(e.to_string()))?;
        conn.pragma_update(None, "temp_store", "MEMORY")
            .map_err(|e| StorageError::Database(e.to_string()))?;
        conn.pragma_update(None, "busy_timeout", 5000) // 5s timeout
            .map_err(|e| StorageError::Database(e.to_string()))?;
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| StorageError::Migration(e.to_string()))?;
        Ok(())
    }

    /// Get a lock on the write connection.
    pub fn write(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.write_conn
            .lock()
            .map_err(|_| StorageError::LockPoisoned)
    }

    /// Get a lock for read-only operations.
    /// Currently uses the same connection as write, but this is the seam
    /// where a read-only connection pool would be used in production.
    pub fn read(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.write_conn
            .lock()
            .map_err(|_| StorageError::LockPoisoned)
    }

    /// Insert a conversation.
    pub fn insert_conversation(&self, conv: &super::Conversation) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "INSERT OR REPLACE INTO conversation (id, conversation_type, scope, tenant_id, community_id, domain_id, name_encrypted, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                conv.id,
                conv.conversation_type,
                conv.scope,
                conv.tenant_id,
                conv.community_id,
                conv.domain_id,
                conv.name_encrypted,
                conv.created_at_ms,
            ],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    /// Insert a message skeleton.
    pub fn insert_skeleton(&self, skeleton: &super::MessageSkeleton) -> Result<(), StorageError> {
        let conn = self.write()?;
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
        Ok(())
    }

    /// Insert a message body.
    pub fn insert_body(&self, body: &super::MessageBody) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "INSERT OR REPLACE INTO message_body (message_id, text_content, detected_language, rich_meta)
             VALUES (?1, ?2, ?3, NULL)",
            rusqlite::params![body.message_id, body.text_content, body.detected_language],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    /// Insert into FTS5 index.
    pub fn index_fts(
        &self,
        message_id: &str,
        conversation_id: &str,
        sender_id: &str,
        created_at_ms: i64,
        text_content: &str,
    ) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "INSERT INTO search_fts (message_id, conversation_id, sender_id, created_at_ms, text_content)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![message_id, conversation_id, sender_id, created_at_ms, text_content],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    /// Fetch timeline rows for a conversation (newest first).
    pub fn fetch_timeline(
        &self,
        conversation_id: &str,
        limit: usize,
        before_ms: Option<i64>,
    ) -> Result<Vec<super::TimelineRow>, StorageError> {
        let conn = self.read()?;
        let sql = if before_ms.is_some() {
            "SELECT message_id, conversation_id, sender_id, created_at_ms, kind, body_state, media_state, reply_to
             FROM message_skeleton
             WHERE conversation_id = ?1 AND created_at_ms < ?2 AND deleted_at_ms IS NULL
             ORDER BY created_at_ms DESC LIMIT ?3"
        } else {
            "SELECT message_id, conversation_id, sender_id, created_at_ms, kind, body_state, media_state, reply_to
             FROM message_skeleton
             WHERE conversation_id = ?1 AND deleted_at_ms IS NULL
             ORDER BY created_at_ms DESC LIMIT ?2"
        };

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let mut result = Vec::new();
        if let Some(before) = before_ms {
            let rows = stmt
                .query_map(
                    rusqlite::params![conversation_id, before, limit as i64],
                    map_timeline_row,
                )
                .map_err(|e| StorageError::Database(e.to_string()))?;
            for row in rows {
                result.push(row.map_err(|e| StorageError::Database(e.to_string()))?);
            }
        } else {
            let rows = stmt
                .query_map(
                    rusqlite::params![conversation_id, limit as i64],
                    map_timeline_row,
                )
                .map_err(|e| StorageError::Database(e.to_string()))?;
            for row in rows {
                result.push(row.map_err(|e| StorageError::Database(e.to_string()))?);
            }
        }
        Ok(result)
    }

    /// Fetch a message body by message ID.
    pub fn fetch_body(&self, message_id: &str) -> Result<Option<super::MessageBody>, StorageError> {
        let conn = self.read()?;
        let mut stmt = conn
            .prepare("SELECT message_id, text_content, detected_language FROM message_body WHERE message_id = ?1")
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let result = stmt
            .query_row(rusqlite::params![message_id], |row| {
                Ok(super::MessageBody {
                    message_id: row.get(0)?,
                    text_content: row.get(1)?,
                    detected_language: row.get(2)?,
                })
            })
            .ok();
        Ok(result)
    }

    /// Update body state for a message.
    pub fn update_body_state(&self, message_id: &str, state: &str) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "UPDATE message_skeleton SET body_state = ?2 WHERE message_id = ?1",
            rusqlite::params![message_id, state],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    /// Search FTS5 for text content.
    #[allow(clippy::type_complexity)]
    pub fn search_fts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(String, String, String, i64, f64)>, StorageError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.read()?;
        let mut stmt = conn
            .prepare(
                "SELECT message_id, conversation_id, sender_id, created_at_ms, rank
                 FROM search_fts WHERE search_fts MATCH ?1 ORDER BY rank LIMIT ?2",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params![query, limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, f64>(4)?,
                ))
            })
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| StorageError::Database(e.to_string()))?);
        }
        Ok(result)
    }

    /// Search FTS5 for text content, filtered by conversation_id.
    #[allow(clippy::type_complexity)]
    pub fn search_fts_filtered(
        &self,
        query: &str,
        conversation_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, String, String, i64, f64)>, StorageError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.read()?;
        let mut stmt = conn
            .prepare(
                "SELECT message_id, conversation_id, sender_id, created_at_ms, rank
                 FROM search_fts WHERE search_fts MATCH ?1 AND conversation_id = ?2
                 ORDER BY rank LIMIT ?3",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(
                rusqlite::params![query, conversation_id, limit as i64],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, f64>(4)?,
                    ))
                },
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| StorageError::Database(e.to_string()))?);
        }
        Ok(result)
    }

    /// Insert a media asset record.
    pub fn insert_media_asset(&self, asset: &super::MediaAsset) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "INSERT OR REPLACE INTO media_asset
             (asset_id, message_id, mime_type, bytes_total, bytes_local, media_state, wrapped_k_asset, chunk_count, merkle_root, node_id, version_id, storage_sink, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            rusqlite::params![
                asset.asset_id,
                asset.message_id,
                asset.mime_type,
                asset.bytes_total,
                asset.bytes_local,
                asset.media_state,
                vec![0u8; 0], // wrapped_k_asset placeholder
                asset.chunk_count,
                asset.merkle_root,
                asset.node_id,
                asset.version_id,
                asset.storage_sink,
                asset.created_at_ms,
            ],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    /// Fetch a media asset by asset_id.
    pub fn fetch_media_asset(
        &self,
        asset_id: &str,
    ) -> Result<Option<super::MediaAsset>, StorageError> {
        let conn = self.read()?;
        let result = conn
            .query_row(
                "SELECT asset_id, message_id, mime_type, bytes_total, bytes_local, media_state, chunk_count, merkle_root, node_id, version_id, storage_sink, created_at_ms
                 FROM media_asset WHERE asset_id = ?1",
                rusqlite::params![asset_id],
                |row| {
                    Ok(super::MediaAsset {
                        asset_id: row.get(0)?,
                        message_id: row.get(1)?,
                        mime_type: row.get(2)?,
                        bytes_total: row.get(3)?,
                        bytes_local: row.get(4)?,
                        media_state: row.get(5)?,
                        chunk_count: row.get(6)?,
                        merkle_root: row.get(7)?,
                        node_id: row.get(8)?,
                        version_id: row.get(9)?,
                        storage_sink: row.get(10)?,
                        created_at_ms: row.get(11)?,
                    })
                },
            )
            .ok();
        Ok(result)
    }

    /// Update media state for an asset.
    pub fn update_media_state(&self, asset_id: &str, state: &str) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "UPDATE media_asset SET media_state = ?2 WHERE asset_id = ?1",
            rusqlite::params![asset_id, state],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    /// Insert into the fuzzy search index.
    pub fn index_fuzzy(
        &self,
        token: &str,
        script: &str,
        message_id: &str,
    ) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "INSERT OR IGNORE INTO search_fuzzy (token, script, message_id) VALUES (?1, ?2, ?3)",
            rusqlite::params![token, script, message_id],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    /// Fetch a message skeleton by message ID.
    pub fn fetch_skeleton(
        &self,
        message_id: &str,
    ) -> Result<Option<super::MessageSkeleton>, StorageError> {
        let conn = self.read()?;
        let result = conn
            .query_row(
                "SELECT message_id, conversation_id, sender_id, created_at_ms, received_at_ms,
                        kind, body_state, media_state, archive_state, backup_state, reply_to,
                        edited_at_ms, deleted_at_ms
                 FROM message_skeleton WHERE message_id = ?1",
                rusqlite::params![message_id],
                |row| {
                    Ok(super::MessageSkeleton {
                        message_id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        sender_id: row.get(2)?,
                        created_at_ms: row.get(3)?,
                        received_at_ms: row.get(4)?,
                        kind: super::MessageKind::parse(&row.get::<_, String>(5)?),
                        body_state: row.get(6)?,
                        media_state: row.get(7)?,
                        archive_state: row.get(8)?,
                        backup_state: row.get(9)?,
                        reply_to: row.get(10)?,
                        edited_at_ms: row.get(11)?,
                        deleted_at_ms: row.get(12)?,
                    })
                },
            )
            .ok();
        Ok(result)
    }

    /// Fetch evictable media assets (originals stored locally).
    pub fn fetch_evictable_media(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, i64, i64)>, StorageError> {
        let conn = self.read()?;
        let mut stmt = conn
            .prepare(
                "SELECT asset_id, bytes_local, created_at_ms
                 FROM media_asset
                 WHERE media_state = 'original_local' AND bytes_local > 0
                 ORDER BY bytes_local DESC LIMIT ?1",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| StorageError::Database(e.to_string()))?);
        }
        Ok(result)
    }

    /// Fetch evictable message bodies (messages that are archived).
    pub fn fetch_evictable_bodies(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, i64, i64)>, StorageError> {
        let conn = self.read()?;
        let mut stmt = conn
            .prepare(
                "SELECT mb.message_id, length(mb.text_content), ms.created_at_ms
                 FROM message_body mb
                 JOIN message_skeleton ms ON mb.message_id = ms.message_id
                 WHERE ms.archive_state = 'archived' AND ms.deleted_at_ms IS NULL
                 ORDER BY ms.created_at_ms ASC LIMIT ?1",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| StorageError::Database(e.to_string()))?);
        }
        Ok(result)
    }

    /// Count messages in a conversation.
    pub fn count_messages(&self, conversation_id: &str) -> Result<i64, StorageError> {
        let conn = self.read()?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM message_skeleton WHERE conversation_id = ?1 AND deleted_at_ms IS NULL",
                rusqlite::params![conversation_id],
                |row| row.get(0),
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(count)
    }

    /// Get the current database size (approximate, from page_count * page_size).
    pub fn db_size_bytes(&self) -> Result<u64, StorageError> {
        let conn = self.read()?;
        let page_count: i64 = conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .map_err(|e| StorageError::Database(e.to_string()))?;
        // SQLCipher may return text for page_size; handle both types
        let page_size: i64 = conn
            .query_row("PRAGMA page_size", [], |row| match row
                .get::<_, rusqlite::types::Value>(0)?
            {
                rusqlite::types::Value::Integer(i) => Ok(i),
                rusqlite::types::Value::Text(s) => Ok(s.parse::<i64>().unwrap_or(4096)),
                _ => Ok(4096),
            })
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok((page_count * page_size) as u64)
    }
}

/// A pool of read-only connections for parallel reads.
#[derive(Debug)]
#[allow(dead_code)]
pub struct LocalStoreReaderPool {
    readers: Vec<Mutex<Connection>>,
    next: Mutex<usize>,
}

impl LocalStoreReaderPool {
    pub fn new(_db_path: &str, _key: &[u8; 32], _size: usize) -> Result<Self, StorageError> {
        // For now, we use a single connection pool. In production,
        // this opens multiple read-only connections.
        // TODO: implement actual multi-connection pool
        Ok(Self {
            readers: Vec::new(),
            next: Mutex::new(0),
        })
    }
}

/// Helper function to map a rusqlite Row to a TimelineRow.
fn map_timeline_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<super::TimelineRow> {
    Ok(super::TimelineRow {
        message_id: row.get(0)?,
        conversation_id: row.get(1)?,
        sender_id: row.get(2)?,
        created_at_ms: row.get(3)?,
        kind: row.get(4)?,
        body_state: row.get(5)?,
        media_state: row.get(6)?,
        reply_to: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::super::schema::*;
    use super::*;

    #[test]
    fn test_open_and_schema() {
        let db = LocalStoreDb::open_in_memory(&[0x42u8; 32]).unwrap();

        let conv = Conversation {
            id: "conv-1".to_string(),
            conversation_type: "direct".to_string(),
            scope: "b2c".to_string(),
            tenant_id: None,
            community_id: None,
            domain_id: None,
            name_encrypted: None,
            created_at_ms: 1_700_000_000_000,
        };
        db.insert_conversation(&conv).unwrap();

        let skeleton = MessageSkeleton {
            message_id: "msg-1".to_string(),
            conversation_id: "conv-1".to_string(),
            sender_id: "user-1".to_string(),
            created_at_ms: 1_700_000_000_000,
            received_at_ms: 1_700_000_001_000,
            kind: MessageKind::Text,
            body_state: "local_plain_available".to_string(),
            media_state: None,
            archive_state: "not_archived".to_string(),
            backup_state: "not_backed_up".to_string(),
            reply_to: None,
            edited_at_ms: None,
            deleted_at_ms: None,
        };
        db.insert_skeleton(&skeleton).unwrap();

        let body = MessageBody {
            message_id: "msg-1".to_string(),
            text_content: Some("Hello world".to_string()),
            detected_language: Some("en".to_string()),
        };
        db.insert_body(&body).unwrap();
        db.index_fts(
            "msg-1",
            "conv-1",
            "user-1",
            1_700_000_000_000,
            "Hello world",
        )
        .unwrap();

        let timeline = db.fetch_timeline("conv-1", 10, None).unwrap();
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].message_id, "msg-1");

        let fetched_body = db.fetch_body("msg-1").unwrap();
        assert!(fetched_body.is_some());
        assert_eq!(
            fetched_body.unwrap().text_content,
            Some("Hello world".to_string())
        );

        let results = db.search_fts("hello", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "msg-1");
    }
}
