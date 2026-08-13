//! Restore pipeline — skeleton-first restore.

use crate::archive::epoch_keys::EpochKeyManager;
use crate::archive::segment_builder::open_segment;
use crate::crypto::Key32;
use crate::formats::archive_segment::ArchiveSegmentFrame;
use crate::formats::backup_manifest::{decode_payload, BackupManifestPayload};
use crate::restore::key_recovery::recover_epoch_key;
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
            state: RestoreState::NotStarted,
        }
    }

    pub fn state(&self) -> RestoreState {
        self.state
    }

    /// Execute the full restore pipeline.
    ///
    /// `wrapping_key` is the KDRV1 DomainKey (or ShareGrantKey) from which
    /// the archive root key is derived to unwrap epoch keys and decrypt segments.
    pub fn execute(
        &mut self,
        _source: &BackupSource,
        wrapping_key: &Key32,
        transport: &dyn ChatStorageTransport,
    ) -> Result<RestoreResult, crate::Error> {
        self.state = RestoreState::FetchingManifests;

        // 1. Fetch manifests from the gateway
        let manifest_bytes = transport
            .fetch_backup_manifests(0)
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;

        if manifest_bytes.is_empty() {
            return Ok(RestoreResult::default());
        }

        // 2. Decode manifests
        let mut manifests: Vec<BackupManifestPayload> = Vec::new();
        for bytes in &manifest_bytes {
            let payload = decode_payload(bytes)?;
            manifests.push(payload);
        }

        self.state = RestoreState::FetchingSkeletons;

        // 3. Verify manifest chain
        verify_chain(&manifests)?;

        self.state = RestoreState::FetchingBodies;

        // 4. Set up epoch key manager for decryption
        let epoch_mgr = EpochKeyManager::new(wrapping_key);

        // 5. Download and decrypt segments
        let mut messages_restored = 0;
        for manifest in &manifests {
            for seg_ref in &manifest.segments {
                let ciphertext = transport
                    .download_archive_segment(&seg_ref.segment_id)
                    .map_err(|e| crate::Error::Storage(e.to_string().into()))?;

                // Derive segment key: epoch key → segment key
                // We need to know which epoch this segment belongs to.
                // The manifest's wrapped_epoch_keys tell us which epochs are available.
                // Try each wrapped epoch key until decryption succeeds.
                let mut decrypted = false;
                for wrapped in &manifest.wrapped_epoch_keys {
                    if let Ok(epoch_key) = recover_epoch_key(
                        &crate::crypto::key_bridge::derive_archive_root(wrapping_key),
                        &wrapped.wrapped_key,
                    ) {
                        let segment_key = crate::crypto::key_bridge::derive_archive_segment(
                            &epoch_key,
                            seg_ref.segment_id.as_bytes(),
                        );

                        // Parse the ciphertext as an ArchiveSegmentFrame
                        // The frame is CBOR/JSON-encoded (nonce + ciphertext + hash + size)
                        if let Ok(frame) =
                            serde_json::from_slice::<ArchiveSegmentFrame>(&ciphertext)
                        {
                            if let Ok(payload) = open_segment(&frame, &segment_key) {
                                messages_restored += payload.entries.len();
                                decrypted = true;
                                break;
                            }
                        }
                    }
                }

                if !decrypted {
                    // Try current epoch key directly
                    let epoch_key = epoch_mgr.current_epoch_key();
                    let segment_key = crate::crypto::key_bridge::derive_archive_segment(
                        &epoch_key,
                        seg_ref.segment_id.as_bytes(),
                    );
                    if let Ok(frame) = serde_json::from_slice::<ArchiveSegmentFrame>(&ciphertext) {
                        if let Ok(payload) = open_segment(&frame, &segment_key) {
                            messages_restored += payload.entries.len();
                        }
                    }
                }
            }
        }

        self.state = RestoreState::FetchingMedia;

        self.state = RestoreState::BuildingIndexes;

        // 6. Rebuild search indexes (stub — would re-index FTS5 + fuzzy)
        self.state = RestoreState::Complete;

        Ok(RestoreResult {
            conversations_restored: 0,
            messages_restored,
            media_restored: 0,
            search_indexes_rebuilt: 0,
        })
    }
}

impl Default for RestorePipeline {
    fn default() -> Self {
        Self::new()
    }
}
