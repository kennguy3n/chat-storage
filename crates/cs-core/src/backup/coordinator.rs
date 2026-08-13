//! Backup coordinator — orchestrates segment build, sink upload, manifest publish.

use crate::backup::event_journal::{BackupEvent, BackupEventJournal};
use crate::backup::manifest_builder::build_manifest as build_backup_manifest;
use crate::backup::segment_builder::build_segment as build_backup_segment;
use crate::backup::sinks::kdrive_sink;
use crate::crypto::Key32;
use crate::formats::backup_manifest::SegmentRef;
use crate::transport::ChatStorageTransport;

/// The backup coordinator manages the backup lifecycle.
#[derive(Debug)]
pub struct BackupCoordinator {
    pub event_journal: BackupEventJournal,
    pub current_generation: u64,
    prev_manifest_hash: [u8; 32],
}

impl BackupCoordinator {
    pub fn new() -> Self {
        Self {
            event_journal: BackupEventJournal::new(),
            current_generation: 0,
            prev_manifest_hash: [0u8; 32],
        }
    }

    /// Run a full backup cycle: build segment from data, upload, build & upload manifest.
    pub fn run_backup(
        &mut self,
        data: &[u8],
        backup_key: &Key32,
        transport: &dyn ChatStorageTransport,
    ) -> Result<u64, crate::Error> {
        if data.is_empty() {
            return Ok(self.current_generation);
        }

        // Build encrypted+compressed backup segment
        let segment_id = uuid::Uuid::now_v7().to_string();
        let (ciphertext, _nonce, plaintext_hash) = build_backup_segment(data, backup_key)?;

        // Upload segment
        let uploaded_id = kdrive_sink::upload_segment(transport, &segment_id, &ciphertext)?;
        self.event_journal.record(BackupEvent::SegmentUploaded {
            segment_id: uploaded_id.clone(),
            size: ciphertext.len(),
        });

        // Build manifest
        let segment_ref = SegmentRef {
            segment_id: uploaded_id.clone(),
            storage_key: uploaded_id,
            size: ciphertext.len() as u64,
            merkle_root: plaintext_hash,
        };

        let manifest = build_backup_manifest(
            self.current_generation,
            self.prev_manifest_hash,
            vec![segment_ref],
            vec![], // no wrapped epoch keys for backup
        )?;

        // Encode and upload manifest
        let manifest_bytes = serde_json::to_vec(&manifest)
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;
        kdrive_sink::upload_manifest(transport, &manifest_bytes)?;

        // Update prev hash for chaining
        self.prev_manifest_hash = crate::backup::manifest_builder::manifest_hash(&manifest)?;

        self.event_journal.record(BackupEvent::ManifestPublished {
            generation: self.current_generation,
        });

        self.current_generation += 1;
        Ok(self.current_generation)
    }

    pub fn next_generation(&mut self) -> u64 {
        self.current_generation += 1;
        self.current_generation
    }
}

impl Default for BackupCoordinator {
    fn default() -> Self {
        Self::new()
    }
}
