//! Backup snapshot — serializable representation of local store data.
//!
//! This struct is serialized to CBOR and then encrypted + compressed by the
//! backup segment builder. It contains conversations, message skeletons
//! (only those not yet backed up), and message bodies that need to be backed up.

use serde::{Deserialize, Serialize};

use crate::local_store::{Conversation, MessageBody, MessageSkeleton};

/// A point-in-time snapshot of the local store for backup purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSnapshot {
    pub conversations: Vec<Conversation>,
    pub skeletons: Vec<MessageSkeleton>,
    pub bodies: Vec<MessageBody>,
}

impl BackupSnapshot {
    /// Build an incremental snapshot from the local store database.
    ///
    /// Only skeletons with `backup_state = 'not_backed_up'` are included.
    /// Conversations and bodies are always included (they're small and
    /// use INSERT OR REPLACE on restore).
    ///
    /// # Deprecated
    ///
    /// This method loads ALL conversations, skeletons, and bodies into memory
    /// at once. For large stores, prefer [`BackupSnapshot::stream_backup_batches`]
    /// which processes data in fixed-size batches to bound memory usage.
    #[deprecated(note = "loads all data into memory; use stream_backup_batches for large stores")]
    pub fn from_db(db: &crate::local_store::LocalStoreDb) -> Result<Self, crate::Error> {
        let conversations = db.list_all_conversations()?;
        let skeletons = db.list_skeletons_for_backup()?;
        let bodies = db.list_all_bodies()?;
        Ok(Self {
            conversations,
            skeletons,
            bodies,
        })
    }

    /// Stream backup data in batches of 200 rows, serializing each batch
    /// to CBOR and passing it to the provided callback.
    ///
    /// This avoids loading all conversations, skeletons, and bodies into
    /// memory at once. Each batch is a self-contained `BackupSnapshot` with
    /// at most 200 items per category, serialized to CBOR.
    ///
    /// The callback receives each batch's CBOR bytes and can write them to
    /// a file, upload them, etc. Only one batch is held in memory at a time.
    pub fn stream_backup_batches(
        db: &crate::local_store::LocalStoreDb,
        mut callback: impl FnMut(&[u8]) -> Result<(), crate::Error>,
    ) -> Result<usize, crate::Error> {
        const BATCH_SIZE: usize = 200;

        // Conversations are typically small; include all in the first batch.
        let conversations = db.list_all_conversations()?;
        let mut batch = BackupSnapshot {
            conversations,
            skeletons: Vec::new(),
            bodies: Vec::new(),
        };

        // Stream skeletons and bodies in batches using LIMIT/OFFSET.
        // Use separate offsets for skeletons and bodies since they are
        // independent tables with potentially different row counts.
        let mut skeleton_offset = 0i64;
        let mut body_offset = 0i64;
        let mut total_batches = 0usize;

        loop {
            let skeletons = db.list_skeletons_for_backup_batch(BATCH_SIZE, skeleton_offset)?;
            let bodies = db.list_all_bodies_batch(BATCH_SIZE, body_offset)?;

            if skeletons.is_empty() && bodies.is_empty() {
                break;
            }

            // Capture lengths before moving into batch
            let skel_count = skeletons.len();
            let body_count = bodies.len();

            batch.skeletons = skeletons;
            batch.bodies = bodies;

            let cbor = batch.to_cbor()?;
            callback(&cbor)?;
            total_batches += 1;

            // Clear batch data to free memory before next iteration
            batch.skeletons.clear();
            batch.bodies.clear();

            // Advance each offset independently
            if skel_count > 0 {
                skeleton_offset += BATCH_SIZE as i64;
            }
            if body_count > 0 {
                body_offset += BATCH_SIZE as i64;
            }

            // Terminate when both tables returned fewer than BATCH_SIZE rows
            if skel_count < BATCH_SIZE && body_count < BATCH_SIZE {
                break;
            }
        }

        // If no batches were produced (empty store), emit one empty batch
        if total_batches == 0 {
            batch.skeletons.clear();
            batch.bodies.clear();
            let cbor = batch.to_cbor()?;
            callback(&cbor)?;
            total_batches = 1;
        }

        Ok(total_batches)
    }

    /// Serialize to CBOR.
    pub fn to_cbor(&self) -> Result<Vec<u8>, crate::Error> {
        crate::cbor::to_vec(self).map_err(|e| crate::Error::Storage(e.to_string().into()))
    }

    /// Deserialize from CBOR.
    pub fn from_cbor(data: &[u8]) -> Result<Self, crate::Error> {
        crate::cbor::from_slice(data).map_err(|e| crate::Error::Storage(e.to_string().into()))
    }

    /// Mark all skeletons in this snapshot as backed up (batch in a single transaction).
    pub fn mark_backed_up(
        &self,
        db: &crate::local_store::LocalStoreDb,
    ) -> Result<(), crate::Error> {
        let ids: Vec<&str> = self
            .skeletons
            .iter()
            .map(|s| s.message_id.as_str())
            .collect();
        db.batch_mark_skeletons_backed_up(&ids)?;
        Ok(())
    }
}
