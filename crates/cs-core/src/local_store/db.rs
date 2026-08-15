//! SQLCipher database connection and operations.

use std::collections::HashMap;
use std::sync::Mutex;

use rusqlite::Connection;

use super::state_machines::{ArchiveState, BodyState, MediaState};
use super::{
    schema::{LATEST_USER_VERSION, MIGRATIONS, SCHEMA_SQL},
    StorageError,
};
use std::str::FromStr;

/// Number of read-only connections in the pool.
const READ_POOL_SIZE: usize = 3;

/// The local encrypted store. Owns a write connection and a pool of
/// read-only connections.
#[derive(Debug)]
pub struct LocalStoreDb {
    write_conn: Mutex<Connection>,
    /// Pool of read-only connections for concurrent reads.
    /// Empty for in-memory databases (which can't share data across connections).
    read_conns: Vec<Mutex<Connection>>,
    /// Round-robin index into `read_conns`.
    read_idx: Mutex<usize>,
}

impl LocalStoreDb {
    /// Open (or create) the encrypted database at `path` with the
    /// given 32-byte key.
    pub fn open(path: &str, key: &[u8; 32]) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        Self::init_connection(&conn, key)?;

        // Create read-only connection pool for concurrent reads.
        // The write connection has already created the file and schema,
        // so read-only connections can open successfully.
        let mut read_conns = Vec::with_capacity(READ_POOL_SIZE);
        for _ in 0..READ_POOL_SIZE {
            let read_conn = Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            Self::init_read_connection(&read_conn, key)?;
            read_conns.push(Mutex::new(read_conn));
        }

        Ok(Self {
            write_conn: Mutex::new(conn),
            read_conns,
            read_idx: Mutex::new(0),
        })
    }

    /// Open an in-memory encrypted database (for tests).
    ///
    /// In-memory databases cannot share data across connections, so no
    /// read pool is created — `read()` falls back to the write connection.
    pub fn open_in_memory(key: &[u8; 32]) -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        Self::init_connection(&conn, key)?;
        Ok(Self {
            write_conn: Mutex::new(conn),
            read_conns: Vec::new(),
            read_idx: Mutex::new(0),
        })
    }

    fn init_connection(conn: &Connection, key: &[u8; 32]) -> Result<(), StorageError> {
        let key_hex = hex::encode(key);
        conn.pragma_update(None, "key", &key_hex)?;
        // Production pragmas for performance and concurrency
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "cache_size", -65536)?; // 64MB cache
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "busy_timeout", 5000)?; // 5s timeout
        // C12: additional performance pragmas
        conn.pragma_update(None, "mmap_size", 268435456)?; // 256MB memory-mapped I/O
        conn.pragma_update(None, "wal_autocheckpoint", 1000)?; // checkpoint every 1000 pages

        // Forward-only migration system.
        //
        // `PRAGMA user_version` records the schema version the database is
        // currently at. On a fresh database (version 0) we apply the full
        // SCHEMA_SQL (which is migration v1) and jump straight to
        // LATEST_USER_VERSION. On an existing database we apply any
        // migrations whose target_version is greater than the current
        // user_version, updating user_version after each one.
        let current_version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap_or(0);

        if current_version == 0 {
            // Fresh database — apply the base schema (migration v1) and any
            // subsequent migrations.
            conn.execute_batch(SCHEMA_SQL)?;
            // Apply migrations beyond v1 if any exist.
            for &(target_version, sql) in MIGRATIONS.iter() {
                if target_version > 1 {
                    conn.execute_batch(sql).map_err(|e| StorageError::MigrationFailed {
                        from: (target_version - 1) as u32,
                        to: target_version as u32,
                        detail: e.to_string(),
                    })?;
                }
            }
            conn.pragma_update(None, "user_version", LATEST_USER_VERSION)?;
        } else if current_version < LATEST_USER_VERSION {
            // Existing database — apply pending migrations in order.
            for &(target_version, sql) in MIGRATIONS.iter() {
                if target_version > current_version {
                    conn.execute_batch(sql).map_err(|e| StorageError::MigrationFailed {
                        from: current_version as u32,
                        to: target_version as u32,
                        detail: e.to_string(),
                    })?;
                    conn.pragma_update(None, "user_version", target_version)?;
                }
            }
        }
        // If current_version == LATEST_USER_VERSION, no work to do.

        Ok(())
    }

    /// Initialize a read-only connection with the key and safe pragmas.
    /// Does not set journal_mode (requires write access) or create schema
    /// (already created by the write connection).
    fn init_read_connection(conn: &Connection, key: &[u8; 32]) -> Result<(), StorageError> {
        let key_hex = hex::encode(key);
        conn.pragma_update(None, "key", &key_hex)?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "cache_size", -65536)?; // 64MB cache
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "mmap_size", 268435456)?; // 256MB
        Ok(())
    }

    /// Get a lock on the write connection.
    pub fn write(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.write_conn
            .lock()
            .map_err(|_| StorageError::LockPoisoned("LocalStoreDb"))
    }

    /// Get a lock for read-only operations.
    /// Uses the read-only connection pool (round-robin) for file-based DBs.
    /// Falls back to the write connection for in-memory databases.
    pub fn read(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        if self.read_conns.is_empty() {
            return self
                .write_conn
                .lock()
                .map_err(|_| StorageError::LockPoisoned("LocalStoreDb"));
        }
        let mut idx_guard = self
            .read_idx
            .lock()
            .map_err(|_| StorageError::LockPoisoned("LocalStoreDb"))?;
        let i = *idx_guard % self.read_conns.len();
        *idx_guard = idx_guard.wrapping_add(1);
        let conn = self.read_conns[i]
            .lock()
            .map_err(|_| StorageError::LockPoisoned("LocalStoreDb"))?;
        drop(idx_guard);
        Ok(conn)
    }

    /// Run `PRAGMA wal_checkpoint(TRUNCATE)` on the write connection to
    /// truncate the WAL file back into the main database.
    pub fn checkpoint(&self) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
        Ok(())
    }

    /// Insert a conversation.
    pub fn insert_conversation(&self, conv: &super::Conversation) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "INSERT OR REPLACE INTO conversation (conversation_id, title_cipher, pinned, muted, last_message_id, last_activity_ms, conversation_type, scope, tenant_id, community_id, domain_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                conv.conversation_id,
                conv.title_cipher,
                conv.pinned as i32,
                conv.muted as i32,
                conv.last_message_id,
                conv.last_activity_ms,
                conv.conversation_type,
                conv.scope,
                conv.tenant_id,
                conv.community_id,
                conv.domain_id,
            ],
        )?;
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
                skeleton.body_state.to_string(),
                skeleton.media_state.map(|s| s.to_string()),
                skeleton.archive_state.to_string(),
                skeleton.backup_state.to_string(),
                skeleton.reply_to,
                skeleton.edited_at_ms,
                skeleton.deleted_at_ms,
            ],
        )?;
        Ok(())
    }

    /// Insert a message body.
    pub fn insert_body(&self, body: &super::MessageBody) -> Result<(), StorageError> {
        let conn = self.write()?;
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
        )?;
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
            "SELECT message_id, conversation_id, sender_id, created_at_ms, kind, body_state, reply_to, edited_at_ms, deleted_at_ms
             FROM message_skeleton
             WHERE conversation_id = ?1 AND created_at_ms < ?2 AND deleted_at_ms IS NULL
             ORDER BY created_at_ms DESC LIMIT ?3"
        } else {
            "SELECT message_id, conversation_id, sender_id, created_at_ms, kind, body_state, reply_to, edited_at_ms, deleted_at_ms
             FROM message_skeleton
             WHERE conversation_id = ?1 AND deleted_at_ms IS NULL
             ORDER BY created_at_ms DESC LIMIT ?2"
        };

        let mut stmt = conn.prepare_cached(sql)?;

        let mut result = Vec::new();
        if let Some(before) = before_ms {
            let rows = stmt.query_map(
                rusqlite::params![conversation_id, before, limit as i64],
                map_timeline_row,
            )?;
            for row in rows {
                result.push(row?);
            }
        } else {
            let rows = stmt.query_map(
                rusqlite::params![conversation_id, limit as i64],
                map_timeline_row,
            )?;
            for row in rows {
                result.push(row?);
            }
        }
        Ok(result)
    }

    /// Fetch a message body by message ID.
    pub fn fetch_body(&self, message_id: &str) -> Result<Option<super::MessageBody>, StorageError> {
        let conn = self.read()?;
        let mut stmt = conn.prepare_cached(
            "SELECT message_id, text_content, detected_language, rich_meta FROM message_body WHERE message_id = ?1",
        )?;
        let result = stmt
            .query_row(rusqlite::params![message_id], |row| {
                Ok(super::MessageBody {
                    message_id: row.get(0)?,
                    text_content: row.get(1)?,
                    detected_language: row.get(2)?,
                    rich_meta: row.get(3)?,
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
        )?;
        Ok(())
    }

    /// Batch-fetch text content for multiple message IDs in a single query.
    /// Returns a map of message_id → text_content for messages that have text.
    /// Used to avoid N+1 queries when generating search snippets.
    pub fn fetch_bodies_batch(
        &self,
        message_ids: &[String],
    ) -> Result<HashMap<String, String>, StorageError> {
        if message_ids.is_empty() {
            return Ok(HashMap::new());
        }
        // SQLite has a ~999 parameter limit. Guard against exceeding it.
        const MAX_BATCH_PARAMS: usize = 900;
        if message_ids.len() > MAX_BATCH_PARAMS {
            return Err(StorageError::Custom(format!(
                "batch size {} exceeds SQLite parameter limit",
                message_ids.len()
            )));
        }
        let conn = self.read()?;
        let placeholders = (0..message_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT message_id, text_content FROM message_body WHERE message_id IN ({})",
            placeholders
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> = message_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        let rows = stmt.query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        })?;
        let mut result = HashMap::new();
        for row in rows {
            let (id, text) = row?;
            if let Some(text) = text {
                result.insert(id, text);
            }
        }
        Ok(result)
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
        let mut stmt = conn.prepare_cached(
            "SELECT message_id, conversation_id, sender_id, created_at_ms, rank
             FROM search_fts WHERE search_fts MATCH ?1 ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![query, limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, f64>(4)?,
            ))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
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
        let mut stmt = conn.prepare_cached(
            "SELECT message_id, conversation_id, sender_id, created_at_ms, rank
             FROM search_fts WHERE search_fts MATCH ?1 AND conversation_id = ?2
             ORDER BY rank LIMIT ?3",
        )?;
        let rows = stmt.query_map(
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
        )?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Insert a media asset record.
    pub fn insert_media_asset(&self, asset: &super::MediaAsset) -> Result<(), StorageError> {
        let conn = self.write()?;
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
                "SELECT asset_id, message_id, mime_type, bytes_total, bytes_local, media_state, wrapped_k_asset, chunk_count, merkle_root, blob_id, storage_sink
                 FROM media_asset WHERE asset_id = ?1",
                rusqlite::params![asset_id],
                |row| {
                    let media_state_str: String = row.get(5)?;
                    let media_state = MediaState::from_str(&media_state_str)
                        .unwrap_or(MediaState::ThumbnailOnly);
                    Ok(super::MediaAsset {
                        asset_id: row.get(0)?,
                        message_id: row.get(1)?,
                        mime_type: row.get(2)?,
                        bytes_total: row.get(3)?,
                        bytes_local: row.get(4)?,
                        media_state,
                        wrapped_k_asset: row.get(6)?,
                        chunk_count: row.get::<_, i64>(7)? as i32,
                        merkle_root: row.get(8)?,
                        blob_id: row.get(9)?,
                        storage_sink: row.get(10)?,
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
        )?;
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
        )?;
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
                    let body_state_str: String = row.get(6)?;
                    let media_state_str: Option<String> = row.get(7)?;
                    let archive_state_str: String = row.get(8)?;
                    let backup_state_str: String = row.get(9)?;
                    Ok(super::MessageSkeleton {
                        message_id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        sender_id: row.get(2)?,
                        created_at_ms: row.get(3)?,
                        received_at_ms: row.get(4)?,
                        kind: super::MessageKind::parse(&row.get::<_, String>(5)?),
                        body_state: BodyState::from_str(&body_state_str)
                            .unwrap_or(BodyState::Unavailable),
                        media_state: media_state_str
                            .as_deref()
                            .and_then(|s| MediaState::from_str(s).ok()),
                        archive_state: ArchiveState::from_str(&archive_state_str)
                            .unwrap_or(ArchiveState::NotArchived),
                        backup_state: super::state_machines::BackupState::from_str(
                            &backup_state_str,
                        )
                        .unwrap_or(super::state_machines::BackupState::NotBackedUp),
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
        let mut stmt = conn.prepare_cached(
            "SELECT asset_id, bytes_local, bytes_total
             FROM media_asset
             WHERE media_state = 'original_local' AND bytes_local > 0
             ORDER BY bytes_local DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Fetch evictable message bodies (messages that are archived).
    pub fn fetch_evictable_bodies(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, i64, i64)>, StorageError> {
        let conn = self.read()?;
        let mut stmt = conn.prepare_cached(
            "SELECT mb.message_id, length(mb.text_content), ms.created_at_ms
             FROM message_body mb
             JOIN message_skeleton ms ON mb.message_id = ms.message_id
             WHERE ms.archive_state = 'archive_verified' AND ms.deleted_at_ms IS NULL
             ORDER BY ms.created_at_ms ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Count messages in a conversation.
    pub fn count_messages(&self, conversation_id: &str) -> Result<i64, StorageError> {
        let conn = self.read()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM message_skeleton WHERE conversation_id = ?1 AND deleted_at_ms IS NULL",
            rusqlite::params![conversation_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Insert a backup event journal entry.
    pub fn insert_backup_event(
        &self,
        entry: &super::BackupEventJournalEntry,
    ) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "INSERT INTO backup_event_journal (event_type, conversation_id, message_id, payload, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                entry.event_type,
                entry.conversation_id,
                entry.message_id,
                entry.payload,
                entry.created_at_ms,
            ],
        )?;
        Ok(())
    }

    /// Update a conversation's last_message_id and last_activity_ms.
    pub fn update_conversation_last_message(
        &self,
        conversation_id: &str,
        message_id: &str,
        activity_ms: i64,
    ) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "UPDATE conversation SET last_message_id = ?2, last_activity_ms = ?3
             WHERE conversation_id = ?1",
            rusqlite::params![conversation_id, message_id, activity_ms],
        )?;
        Ok(())
    }

    /// Alias for `fetch_skeleton` matching the search repo API name.
    pub fn get_message_skeleton(
        &self,
        message_id: &str,
    ) -> Result<Option<super::MessageSkeleton>, StorageError> {
        self.fetch_skeleton(message_id)
    }

    /// Update the text content of a message body.
    pub fn update_message_body_text(
        &self,
        message_id: &str,
        new_text: &str,
    ) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "UPDATE message_body SET text_content = ?2 WHERE message_id = ?1",
            rusqlite::params![message_id, new_text],
        )?;
        Ok(())
    }

    /// Set the `edited_at_ms` timestamp on a skeleton row.
    pub fn update_skeleton_edited(
        &self,
        message_id: &str,
        edited_at_ms: i64,
    ) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "UPDATE message_skeleton SET edited_at_ms = ?2 WHERE message_id = ?1",
            rusqlite::params![message_id, edited_at_ms],
        )?;
        Ok(())
    }

    /// Delete a row from the FTS5 index.
    pub fn delete_fts_row(&self, message_id: &str) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "DELETE FROM search_fts WHERE message_id = ?1",
            rusqlite::params![message_id],
        )?;
        Ok(())
    }

    /// Delete fuzzy index rows for a message.
    pub fn delete_fuzzy_rows(&self, message_id: &str) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "DELETE FROM search_fuzzy WHERE message_id = ?1",
            rusqlite::params![message_id],
        )?;
        Ok(())
    }

    /// Mark a skeleton as deleted (set `deleted_at_ms` and update `body_state`).
    pub fn update_skeleton_deleted(
        &self,
        message_id: &str,
        deleted_at_ms: i64,
        new_state: BodyState,
    ) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "UPDATE message_skeleton SET deleted_at_ms = ?2, body_state = ?3
             WHERE message_id = ?1",
            rusqlite::params![message_id, deleted_at_ms, new_state.to_string()],
        )?;
        Ok(())
    }

    /// Delete a message body row.
    pub fn delete_message_body(&self, message_id: &str) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "DELETE FROM message_body WHERE message_id = ?1",
            rusqlite::params![message_id],
        )?;
        Ok(())
    }

    /// List all media assets attached to a message.
    pub fn list_media_assets_by_message(
        &self,
        message_id: &str,
    ) -> Result<Vec<super::MediaAsset>, StorageError> {
        let conn = self.read()?;
        let mut stmt = conn.prepare_cached(
            "SELECT asset_id, message_id, mime_type, bytes_total, bytes_local, media_state,
                    wrapped_k_asset, chunk_count, merkle_root, blob_id, storage_sink
             FROM media_asset WHERE message_id = ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![message_id], |row| {
            let media_state_str: String = row.get(5)?;
            let media_state =
                MediaState::from_str(&media_state_str).unwrap_or(MediaState::ThumbnailOnly);
            Ok(super::MediaAsset {
                asset_id: row.get(0)?,
                message_id: row.get(1)?,
                mime_type: row.get(2)?,
                bytes_total: row.get(3)?,
                bytes_local: row.get(4)?,
                media_state,
                wrapped_k_asset: row.get(6)?,
                chunk_count: row.get::<_, i64>(7)? as i32,
                merkle_root: row.get(8)?,
                blob_id: row.get(9)?,
                storage_sink: row.get(10)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Get the current database size (approximate, from page_count * page_size).
    pub fn db_size_bytes(&self) -> Result<u64, StorageError> {
        let conn = self.read()?;
        let page_count: i64 = conn.query_row("PRAGMA page_count", [], |row| row.get(0))?;
        // SQLCipher may return text for page_size; handle both types
        let page_size: i64 =
            conn.query_row("PRAGMA page_size", [], |row| match row
                .get::<_, rusqlite::types::Value>(0)?
            {
                rusqlite::types::Value::Integer(i) => Ok(i),
                rusqlite::types::Value::Text(s) => Ok(s.parse::<i64>().unwrap_or(4096)),
                _ => Ok(4096),
            })?;
        Ok((page_count * page_size) as u64)
    }

    /// List all conversations (for backup serialization).
    pub fn list_all_conversations(&self) -> Result<Vec<super::Conversation>, StorageError> {
        let conn = self.read()?;
        let mut stmt = conn.prepare_cached(
            "SELECT conversation_id, title_cipher, pinned, muted, last_message_id,
                    last_activity_ms, conversation_type, scope, tenant_id, community_id, domain_id
             FROM conversation",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(super::Conversation {
                conversation_id: row.get(0)?,
                title_cipher: row.get(1)?,
                pinned: row.get(2)?,
                muted: row.get(3)?,
                last_message_id: row.get(4)?,
                last_activity_ms: row.get(5)?,
                conversation_type: row.get(6)?,
                scope: row.get(7)?,
                tenant_id: row.get(8)?,
                community_id: row.get(9)?,
                domain_id: row.get(10)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// List all message skeletons (for backup serialization).
    pub fn list_all_skeletons(&self) -> Result<Vec<super::MessageSkeleton>, StorageError> {
        let conn = self.read()?;
        let mut stmt = conn.prepare_cached(
            "SELECT message_id, conversation_id, sender_id, created_at_ms, received_at_ms,
                    kind, body_state, media_state, archive_state, backup_state,
                    reply_to, edited_at_ms, deleted_at_ms
             FROM message_skeleton",
        )?;
        let rows = stmt.query_map([], |row| {
            let kind_str: String = row.get(5)?;
            let body_state_str: String = row.get(6)?;
            let media_state_str: Option<String> = row.get(7)?;
            let archive_state_str: String = row.get(8)?;
            let backup_state_str: String = row.get(9)?;
            Ok(super::MessageSkeleton {
                message_id: row.get(0)?,
                conversation_id: row.get(1)?,
                sender_id: row.get(2)?,
                created_at_ms: row.get(3)?,
                received_at_ms: row.get(4)?,
                kind: super::MessageKind::parse(&kind_str),
                body_state: BodyState::from_str(&body_state_str).unwrap_or(BodyState::Unavailable),
                media_state: media_state_str
                    .as_deref()
                    .and_then(|s| MediaState::from_str(s).ok()),
                archive_state: ArchiveState::from_str(&archive_state_str)
                    .unwrap_or(ArchiveState::NotArchived),
                backup_state: super::state_machines::BackupState::from_str(&backup_state_str)
                    .unwrap_or(super::state_machines::BackupState::NotBackedUp),
                reply_to: row.get(10)?,
                edited_at_ms: row.get(11)?,
                deleted_at_ms: row.get(12)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// List only skeletons that haven't been backed up yet (incremental backup).
    pub fn list_skeletons_for_backup(&self) -> Result<Vec<super::MessageSkeleton>, StorageError> {
        let conn = self.read()?;
        let mut stmt = conn.prepare_cached(
            "SELECT message_id, conversation_id, sender_id, created_at_ms, received_at_ms,
                    kind, body_state, media_state, archive_state, backup_state,
                    reply_to, edited_at_ms, deleted_at_ms
             FROM message_skeleton
             WHERE backup_state = 'not_backed_up'",
        )?;
        let rows = stmt.query_map([], |row| {
            let kind_str: String = row.get(5)?;
            let body_state_str: String = row.get(6)?;
            let media_state_str: Option<String> = row.get(7)?;
            let archive_state_str: String = row.get(8)?;
            let backup_state_str: String = row.get(9)?;
            Ok(super::MessageSkeleton {
                message_id: row.get(0)?,
                conversation_id: row.get(1)?,
                sender_id: row.get(2)?,
                created_at_ms: row.get(3)?,
                received_at_ms: row.get(4)?,
                kind: super::MessageKind::parse(&kind_str),
                body_state: BodyState::from_str(&body_state_str).unwrap_or(BodyState::Unavailable),
                media_state: media_state_str
                    .as_deref()
                    .and_then(|s| MediaState::from_str(s).ok()),
                archive_state: ArchiveState::from_str(&archive_state_str)
                    .unwrap_or(ArchiveState::NotArchived),
                backup_state: super::state_machines::BackupState::from_str(&backup_state_str)
                    .unwrap_or(super::state_machines::BackupState::NotBackedUp),
                reply_to: row.get(10)?,
                edited_at_ms: row.get(11)?,
                deleted_at_ms: row.get(12)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// List all message bodies (for backup serialization).
    pub fn list_all_bodies(&self) -> Result<Vec<super::MessageBody>, StorageError> {
        let conn = self.read()?;
        let mut stmt = conn.prepare_cached(
            "SELECT message_id, text_content, detected_language, rich_meta FROM message_body",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(super::MessageBody {
                message_id: row.get(0)?,
                text_content: row.get(1)?,
                detected_language: row.get(2)?,
                rich_meta: row.get(3)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// List skeletons for backup in batches (C3: streaming backup support).
    /// Uses LIMIT/OFFSET to fetch only `limit` rows starting at `offset`.
    pub fn list_skeletons_for_backup_batch(
        &self,
        limit: usize,
        offset: i64,
    ) -> Result<Vec<super::MessageSkeleton>, StorageError> {
        let conn = self.read()?;
        let mut stmt = conn.prepare_cached(
            "SELECT message_id, conversation_id, sender_id, created_at_ms, received_at_ms,
                    kind, body_state, media_state, archive_state, backup_state,
                    reply_to, edited_at_ms, deleted_at_ms
             FROM message_skeleton
             WHERE backup_state = 'not_backed_up'
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit as i64, offset], |row| {
            let kind_str: String = row.get(5)?;
            let body_state_str: String = row.get(6)?;
            let media_state_str: Option<String> = row.get(7)?;
            let archive_state_str: String = row.get(8)?;
            let backup_state_str: String = row.get(9)?;
            Ok(super::MessageSkeleton {
                message_id: row.get(0)?,
                conversation_id: row.get(1)?,
                sender_id: row.get(2)?,
                created_at_ms: row.get(3)?,
                received_at_ms: row.get(4)?,
                kind: super::MessageKind::parse(&kind_str),
                body_state: BodyState::from_str(&body_state_str)
                    .unwrap_or(BodyState::Unavailable),
                media_state: media_state_str
                    .as_deref()
                    .and_then(|s| MediaState::from_str(s).ok()),
                archive_state: ArchiveState::from_str(&archive_state_str)
                    .unwrap_or(ArchiveState::NotArchived),
                backup_state: super::state_machines::BackupState::from_str(&backup_state_str)
                    .unwrap_or(super::state_machines::BackupState::NotBackedUp),
                reply_to: row.get(10)?,
                edited_at_ms: row.get(11)?,
                deleted_at_ms: row.get(12)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// List message bodies in batches (C3: streaming backup support).
    /// Uses LIMIT/OFFSET to fetch only `limit` rows starting at `offset`.
    pub fn list_all_bodies_batch(
        &self,
        limit: usize,
        offset: i64,
    ) -> Result<Vec<super::MessageBody>, StorageError> {
        let conn = self.read()?;
        let mut stmt = conn.prepare_cached(
            "SELECT message_id, text_content, detected_language, rich_meta
             FROM message_body
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit as i64, offset], |row| {
            Ok(super::MessageBody {
                message_id: row.get(0)?,
                text_content: row.get(1)?,
                detected_language: row.get(2)?,
                rich_meta: row.get(3)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Mark a skeleton as backed up.
    pub fn mark_skeleton_backed_up(&self, message_id: &str) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "UPDATE message_skeleton SET backup_state = 'backup_manifest_committed' WHERE message_id = ?1",
            rusqlite::params![message_id],
        )?;
        Ok(())
    }

    /// Batch-mark multiple skeletons as backed up in a single transaction.
    /// More efficient than calling `mark_skeleton_backed_up` N times.
    pub fn batch_mark_skeletons_backed_up(&self, message_ids: &[&str]) -> Result<(), StorageError> {
        if message_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.write()?;
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "UPDATE message_skeleton SET backup_state = 'backup_manifest_committed' WHERE message_id = ?1",
            )?;
            for id in message_ids {
                stmt.execute(rusqlite::params![id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Re-index a message into FTS5 and fuzzy search indexes.
    /// Used by the restore pipeline to rebuild search indexes.
    pub fn reindex_message(
        &self,
        message_id: &str,
        conversation_id: &str,
        sender_id: &str,
        created_at_ms: i64,
        text: &str,
    ) -> Result<(), StorageError> {
        // Insert FTS5 row
        self.index_fts(message_id, conversation_id, sender_id, created_at_ms, text)?;

        // Insert fuzzy tokens in a single transaction (M20: batch insert)
        let tokens = crate::search::tokenizer::tokenize(text);
        let mut conn = self.write()?;
        let tx = conn.transaction()?;
        {
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
                    tx.execute(
                        "INSERT OR IGNORE INTO search_fuzzy (token, script, message_id) VALUES (?1, ?2, ?3)",
                        rusqlite::params![gram, script.code(), message_id],
                    )?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Prune backup event journal entries older than the given timestamp.
    /// Returns the number of rows deleted.
    pub fn prune_backup_events(&self, before_ms: i64) -> Result<usize, StorageError> {
        let conn = self.write()?;
        let count = conn.execute(
            "DELETE FROM backup_event_journal WHERE created_at_ms < ?1",
            rusqlite::params![before_ms],
        )?;
        Ok(count)
    }

    /// Save backup coordinator state (current_generation, prev_manifest_hash)
    /// to the `backup_state` table. Uses INSERT OR REPLACE for upsert semantics.
    pub fn save_backup_state(
        &self,
        current_generation: u64,
        prev_manifest_hash: &[u8; 32],
    ) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute(
            "INSERT OR REPLACE INTO backup_state (id, current_generation, prev_manifest_hash)
             VALUES (1, ?1, ?2)",
            rusqlite::params![current_generation as i64, prev_manifest_hash.to_vec()],
        )?;
        Ok(())
    }

    /// Load backup coordinator state from the `backup_state` table.
    /// Returns `None` if no state has been persisted yet.
    pub fn load_backup_state(&self) -> Result<Option<(u64, [u8; 32])>, StorageError> {
        let conn = self.read()?;
        let result = conn
            .query_row(
                "SELECT current_generation, prev_manifest_hash FROM backup_state WHERE id = 1",
                [],
                |row| {
                    let gen: i64 = row.get(0)?;
                    let hash_vec: Vec<u8> = row.get(1)?;
                    let mut hash = [0u8; 32];
                    if hash_vec.len() == 32 {
                        hash.copy_from_slice(&hash_vec);
                    }
                    Ok((gen as u64, hash))
                },
            )
            .ok();
        Ok(result)
    }

    /// Clear all message-related data (for restore pre-clean).
    ///
    /// Deletes rows from search_fts, search_fuzzy, message_body, message_skeleton,
    /// media_asset, conversation, backup_event_journal, and outbox tables.
    /// This prevents silent merges when restoring into a non-empty database.
    pub fn clear_all_message_data(&self) -> Result<(), StorageError> {
        let conn = self.write()?;
        conn.execute_batch(
            "DELETE FROM search_fts;
             DELETE FROM search_fuzzy;
             DELETE FROM message_body;
             DELETE FROM media_asset;
             DELETE FROM message_skeleton;
             DELETE FROM conversation;
             DELETE FROM backup_event_journal;
             DELETE FROM outbox;",
        )?;
        Ok(())
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
    let kind_str: String = row.get(4)?;
    let body_state_str: String = row.get(5)?;
    Ok(super::TimelineRow {
        message_id: row.get(0)?,
        conversation_id: row.get(1)?,
        sender_id: row.get(2)?,
        created_at_ms: row.get(3)?,
        kind: super::MessageKind::parse(&kind_str),
        body_state: BodyState::from_str(&body_state_str).unwrap_or(BodyState::Unavailable),
        text_content: None,
        reply_to: row.get(6)?,
        edited_at_ms: row.get(7)?,
        deleted_at_ms: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::super::schema::*;
    use super::*;

    #[test]
    fn test_open_and_schema() {
        let db = LocalStoreDb::open_in_memory(&[0x42u8; 32]).unwrap();

        let conv = Conversation::legacy("conv-1", None, false, false, None, 1_700_000_000_000);
        db.insert_conversation(&conv).unwrap();

        let skeleton = MessageSkeleton {
            message_id: "msg-1".to_string(),
            conversation_id: "conv-1".to_string(),
            sender_id: "user-1".to_string(),
            created_at_ms: 1_700_000_000_000,
            received_at_ms: 1_700_000_001_000,
            kind: MessageKind::Text,
            body_state: BodyState::LocalPlainAvailable,
            media_state: None,
            archive_state: ArchiveState::NotArchived,
            backup_state: super::super::state_machines::BackupState::NotBackedUp,
            reply_to: None,
            edited_at_ms: None,
            deleted_at_ms: None,
        };
        db.insert_skeleton(&skeleton).unwrap();

        let body = MessageBody {
            message_id: "msg-1".to_string(),
            text_content: Some("Hello world".to_string()),
            detected_language: Some("en".to_string()),
            rich_meta: None,
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
