//! Archive coordinator — orchestrates segment build, upload, and manifest chain.

use crate::archive::epoch_keys::EpochKeyManager;
use crate::archive::event_journal::{ArchiveEvent, ArchiveEventJournal};
use crate::archive::manifest_builder::build_manifest;
use crate::archive::segment_builder::build_segment;
use crate::archive::types::ArchiveSegmentFrame;
use crate::archive::upload::{upload_manifest, upload_segment};
use crate::formats::manifest::{ManifestSegmentRef, WrappedEpochKeyRef};
use crate::formats::SegmentType;
use crate::message::processor::IngestedMessage;
use crate::transport::ChatStorageTransport;

/// The archive coordinator manages the archive lifecycle.
#[derive(Debug)]
pub struct ArchiveCoordinator {
    pub epoch_manager: EpochKeyManager,
    pub event_journal: ArchiveEventJournal,
    /// Segments built in the current epoch (not yet manifested).
    pending_segments: Vec<(String, ArchiveSegmentFrame)>,
    /// Current manifest generation.
    current_generation: u64,
    /// Hash of the previous manifest for chain integrity.
    prev_manifest_hash: [u8; 32],
}

/// M5: Maximum number of pending segments before forcing a flush.
const MAX_PENDING_SEGMENTS: usize = 100;

impl ArchiveCoordinator {
    pub fn new(wrapping_key: &[u8; 32]) -> Result<Self, crate::Error> {
        Ok(Self {
            epoch_manager: EpochKeyManager::new(wrapping_key)?,
            event_journal: ArchiveEventJournal::new(),
            pending_segments: Vec::with_capacity(MAX_PENDING_SEGMENTS),
            current_generation: 0,
            prev_manifest_hash: [0u8; 32],
        })
    }

    /// Archive a batch of messages: build segment, upload, and queue for manifest.
    pub fn archive_batch(
        &mut self,
        messages: &[IngestedMessage],
        conversation_id: &str,
        time_bucket: &str,
        transport: &dyn ChatStorageTransport,
    ) -> Result<String, crate::Error> {
        let epoch_id = self.epoch_manager.current_epoch();
        let segment_key = self.epoch_manager.current_epoch_key()?;

        // Build the encrypted segment
        let frame = build_segment(
            messages,
            conversation_id,
            time_bucket,
            epoch_id,
            &segment_key,
        )?;
        let segment_id = uuid::Uuid::now_v7().to_string();

        // Upload the segment
        let uploaded_id = upload_segment(transport, &segment_id, &frame)?;
        self.event_journal.record(ArchiveEvent::SegmentUploaded {
            segment_id: uploaded_id.clone(),
            epoch: epoch_id,
            message_count: messages.len(),
        });

        // Queue for manifest
        // M5: check pending segment cap and return error if exceeded
        if self.pending_segments.len() >= MAX_PENDING_SEGMENTS {
            return Err(crate::Error::Storage(
                format!(
                    "pending segment cap reached ({MAX_PENDING_SEGMENTS}) — call finalize_epoch first"
                )
                .into(),
            ));
        }
        self.pending_segments.push((uploaded_id.clone(), frame));

        Ok(uploaded_id)
    }

    /// Finalize the current epoch: build and upload a manifest for all pending segments.
    pub fn finalize_epoch(
        &mut self,
        transport: &dyn ChatStorageTransport,
    ) -> Result<u64, crate::Error> {
        if self.pending_segments.is_empty() {
            return Ok(self.current_generation);
        }

        let _epoch_id = self.epoch_manager.current_epoch();

        // Build SegmentRefs from pending segments
        let segments: Vec<ManifestSegmentRef> = self
            .pending_segments
            .iter()
            .map(|(id, frame)| ManifestSegmentRef {
                segment_id: uuid::Uuid::parse_str(id).unwrap_or_else(|_| uuid::Uuid::now_v7()),
                segment_type: SegmentType::MessageDelta,
                ciphertext_sha256: frame.plaintext_hash,
                size: frame.ciphertext.len() as u64,
            })
            .collect();

        // Build wrapped epoch keys
        let wrapped_keys: Vec<WrappedEpochKeyRef> = self
            .epoch_manager
            .wrapped_prior_epochs()
            .iter()
            .map(|(epoch, wrapped)| WrappedEpochKeyRef {
                epoch_id: epoch.to_string(),
                wrapped_key: wrapped.clone(),
            })
            .collect();

        let manifest = build_manifest(
            self.current_generation,
            self.prev_manifest_hash,
            segments,
            wrapped_keys,
        )?;

        // Encode and upload manifest
        let manifest_bytes = crate::cbor::to_vec(&manifest)
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;
        upload_manifest(transport, &manifest_bytes)?;

        // Update prev hash for chaining
        self.prev_manifest_hash = crate::archive::manifest_builder::manifest_hash(&manifest)?;

        self.event_journal.record(ArchiveEvent::ManifestUploaded {
            generation: self.current_generation,
            segment_count: self.pending_segments.len(),
        });

        // Clear pending and advance generation
        self.pending_segments.clear();
        self.current_generation += 1;

        Ok(self.current_generation)
    }

    /// Check if epoch rotation is needed and rotate.
    pub fn maybe_rotate(&mut self) -> Result<bool, crate::Error> {
        let current = crate::archive::epoch_keys::current_epoch_id();
        if current > self.epoch_manager.current_epoch() {
            let old_epoch = self.epoch_manager.current_epoch();
            self.epoch_manager.rotate()?;
            self.event_journal
                .record(ArchiveEvent::EpochRotated { old_epoch });
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get pending segment count.
    pub fn pending_count(&self) -> usize {
        self.pending_segments.len()
    }
}
