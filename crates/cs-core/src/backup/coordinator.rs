//! Backup coordinator — orchestrates segment build, sink upload, manifest publish.

use crate::backup::event_journal::{BackupEvent, BackupEventJournal};
use crate::backup::manifest_builder::build_manifest as build_backup_manifest;
use crate::backup::segment_builder::build_segment as build_backup_segment;
use crate::backup::sinks::kdrive_sink;
use crate::crypto::Key32;
use crate::formats::manifest::ManifestSegmentRef;
use crate::formats::SegmentType;
use crate::transport::ChatStorageTransport;

/// The backup coordinator manages the backup lifecycle.
#[derive(Debug)]
pub struct BackupCoordinator {
    pub event_journal: BackupEventJournal,
    pub current_generation: u64,
    prev_manifest_hash: [u8; 32],
}

/// Payloads prepared under the coordinator lock, to be uploaded outside it.
#[derive(Debug)]
pub struct BackupPayload {
    pub segment_id: String,
    pub ciphertext: Vec<u8>,
    pub manifest_bytes: Vec<u8>,
    pub plaintext_hash: [u8; 32],
    pub manifest_hash: [u8; 32],
    pub generation: u64,
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
    ///
    /// The lock on `self` is held only for the CPU-bound prepare/finalize
    /// phases; the network uploads happen between `prepare_backup` and
    /// `finalize_backup` so a slow transport does not block other callers.
    pub fn run_backup(
        &mut self,
        data: &[u8],
        backup_key: &Key32,
        transport: &dyn ChatStorageTransport,
    ) -> Result<u64, crate::Error> {
        if data.is_empty() {
            return Ok(self.current_generation);
        }

        // Phase 1 (under lock): build segment + manifest
        let payload = self.prepare_backup(data, backup_key)?;

        // Phase 2 (no lock): upload segment + manifest over the network
        let uploaded_id =
            kdrive_sink::upload_segment(transport, &payload.segment_id, &payload.ciphertext)?;
        kdrive_sink::upload_manifest(transport, &payload.manifest_bytes)?;

        // Phase 3 (under lock): record events + advance state
        Ok(self.finalize_backup(uploaded_id, payload))
    }

    /// Prepare the backup payloads (CPU-bound). Called under the coordinator lock.
    pub(crate) fn prepare_backup(
        &mut self,
        data: &[u8],
        backup_key: &Key32,
    ) -> Result<BackupPayload, crate::Error> {
        let segment_id = uuid::Uuid::now_v7().to_string();
        let (ciphertext, _nonce, plaintext_hash) = build_backup_segment(data, backup_key)?;

        let segment_ref = ManifestSegmentRef {
            segment_id: uuid::Uuid::parse_str(&segment_id)
                .unwrap_or_else(|_| uuid::Uuid::now_v7()),
            segment_type: SegmentType::Events,
            ciphertext_sha256: plaintext_hash,
            size: ciphertext.len() as u64,
        };

        let manifest = build_backup_manifest(
            self.current_generation,
            self.prev_manifest_hash,
            vec![segment_ref],
            vec![],
        )?;

        let manifest_bytes = crate::cbor::to_vec(&manifest)
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;
        let manifest_hash = crate::backup::manifest_builder::manifest_hash(&manifest)?;

        Ok(BackupPayload {
            segment_id,
            ciphertext,
            manifest_bytes,
            plaintext_hash,
            manifest_hash,
            generation: self.current_generation,
        })
    }

    /// Finalize the backup after uploads succeed. Called under the coordinator lock.
    pub(crate) fn finalize_backup(&mut self, uploaded_id: String, payload: BackupPayload) -> u64 {
        self.event_journal.record(BackupEvent::SegmentUploaded {
            segment_id: uploaded_id,
            size: payload.ciphertext.len(),
        });
        self.prev_manifest_hash = payload.manifest_hash;
        self.event_journal.record(BackupEvent::ManifestPublished {
            generation: payload.generation,
        });
        self.current_generation += 1;
        self.current_generation
    }

    pub fn next_generation(&mut self) -> u64 {
        self.current_generation += 1;
        self.current_generation
    }

    /// Persist the coordinator state (current_generation, prev_manifest_hash)
    /// to the local store's `backup_state` table.
    pub fn save_state(&self, db: &crate::local_store::LocalStoreDb) -> Result<(), crate::Error> {
        db.save_backup_state(self.current_generation, &self.prev_manifest_hash)?;
        Ok(())
    }

    /// Load coordinator state from the local store's `backup_state` table.
    /// Returns a new coordinator with the persisted generation and hash,
    /// or a fresh coordinator if no state exists yet.
    pub fn load_state(db: &crate::local_store::LocalStoreDb) -> Result<Self, crate::Error> {
        match db.load_backup_state()? {
            Some((generation, hash)) => Ok(Self {
                event_journal: BackupEventJournal::new(),
                current_generation: generation,
                prev_manifest_hash: hash,
            }),
            None => Ok(Self::new()),
        }
    }
}

impl Default for BackupCoordinator {
    fn default() -> Self {
        Self::new()
    }
}
