//! Restore pipeline — skeleton-first restore.

use crate::archive::epoch_keys::EpochKeyManager;
use crate::backup::segment_builder::open_segment;
use crate::backup::snapshot::BackupSnapshot;
use crate::crypto::Key32;
use crate::formats::manifest::BackupManifest;
use crate::local_store::LocalStoreDb;
use crate::restore::manifest_verifier::verify_chain;
use crate::restore::RestoreState;
use crate::transport::ChatStorageTransport;
use crate::{BackupSource, RestoreResult};

/// The restore pipeline executes a skeleton-first restore.
#[derive(Debug)]
pub struct RestorePipeline {
    state: RestoreState,
}

impl RestorePipeline {
    pub fn new() -> Self {
        Self {
            state: RestoreState::IdentityRestored,
        }
    }

    pub fn state(&self) -> RestoreState {
        self.state
    }

    fn transition(&mut self, to: RestoreState) -> Result<(), crate::Error> {
        self.state = RestoreState::try_transition(self.state, to)
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;
        Ok(())
    }

    /// Execute the full restore pipeline.
    ///
    /// `wrapping_key` is the KDRV1 DomainKey (or ShareGrantKey) from which
    /// the backup root key is derived to decrypt segments.
    /// `db` is the local store where restored data is inserted and search indexes
    /// are rebuilt.
    pub fn execute(
        &mut self,
        _source: &BackupSource,
        wrapping_key: &Key32,
        transport: &dyn ChatStorageTransport,
        db: &LocalStoreDb,
    ) -> Result<RestoreResult, crate::Error> {
        self.transition(RestoreState::RootKeysUnwrapped)?;

        // 1. Fetch manifests from the gateway
        let manifest_bytes = transport
            .fetch_backup_manifests(0)
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;

        if manifest_bytes.is_empty() {
            self.transition(RestoreState::ManifestVerified)?;
            self.transition(RestoreState::SkeletonRestored)?;
            self.transition(RestoreState::SearchRestored)?;
            self.transition(RestoreState::RecentMessagesRestored)?;
            self.transition(RestoreState::MediaLazyRestoreEnabled)?;
            self.transition(RestoreState::FullRestoreComplete)?;
            return Ok(RestoreResult::default());
        }

        // 2. Decode manifests
        let mut manifests: Vec<BackupManifest> = Vec::new();
        for bytes in &manifest_bytes {
            let payload: BackupManifest = crate::cbor::from_slice(bytes)
                .map_err(|e| crate::Error::Storage(e.to_string().into()))?;
            manifests.push(payload);
        }

        // 3. Verify manifest chain
        verify_chain(&manifests)?;
        self.transition(RestoreState::ManifestVerified)?;

        // 4. Set up epoch key manager for decryption
        let epoch_mgr = EpochKeyManager::new(wrapping_key);

        // 5. Download and decrypt backup segments
        let mut all_payloads: Vec<Vec<u8>> = Vec::new();

        for manifest in &manifests {
            for seg_ref in &manifest.segments {
                let seg_id_str = seg_ref.segment_id.to_string();
                let ciphertext = transport
                    .download_backup_segment(&seg_id_str)
                    .map_err(|e| crate::Error::Storage(e.to_string().into()))?;

                // Derive segment key from epoch key
                let epoch_key = epoch_mgr.current_epoch_key();
                let segment_key = crate::crypto::key_bridge::derive_archive_segment(
                    &epoch_key,
                    seg_ref.segment_id.as_bytes(),
                );

                // Decrypt backup segment (self-contained frame: nonce + ciphertext)
                match open_segment(&ciphertext, &segment_key) {
                    Ok(plaintext) => all_payloads.push(plaintext),
                    Err(e) => {
                        return Err(crate::Error::Storage(
                            format!("backup segment decryption failed for {seg_id_str}: {e}")
                                .into(),
                        ));
                    }
                }
            }
        }

        // 6. Clear existing data before restore to avoid silent merges
        db.clear_all_message_data()?;

        // 7. Deserialize backup snapshots from decrypted payloads and insert into DB
        self.transition(RestoreState::SkeletonRestored)?;

        let mut messages_restored = 0usize;
        let mut conversations_restored = 0usize;
        let mut search_indexes_rebuilt = 0usize;

        for payload_bytes in &all_payloads {
            let snapshot = BackupSnapshot::from_cbor(payload_bytes)?;

            // Insert conversations
            for conv in &snapshot.conversations {
                db.insert_conversation(conv)?;
                conversations_restored += 1;
            }

            // Insert skeletons
            for skel in &snapshot.skeletons {
                db.insert_skeleton(skel)?;
                messages_restored += 1;
            }

            // Insert bodies and rebuild search indexes
            for body in &snapshot.bodies {
                db.insert_body(body)?;
                if let Some(ref text) = body.text_content {
                    // Find the skeleton to get conversation_id, sender_id, created_at_ms
                    if let Ok(Some(skel)) = db.get_message_skeleton(&body.message_id) {
                        db.reindex_message(
                            &body.message_id,
                            &skel.conversation_id,
                            &skel.sender_id,
                            skel.created_at_ms,
                            text,
                        )?;
                        search_indexes_rebuilt += 1;
                    }
                }
            }
        }

        // 8. Advance through remaining states
        self.transition(RestoreState::SearchRestored)?;
        self.transition(RestoreState::RecentMessagesRestored)?;
        self.transition(RestoreState::MediaLazyRestoreEnabled)?;
        self.transition(RestoreState::FullRestoreComplete)?;

        Ok(RestoreResult {
            conversations_restored,
            messages_restored,
            media_restored: 0,
            search_indexes_rebuilt,
        })
    }
}

impl Default for RestorePipeline {
    fn default() -> Self {
        Self::new()
    }
}
