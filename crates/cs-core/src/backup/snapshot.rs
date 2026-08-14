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
