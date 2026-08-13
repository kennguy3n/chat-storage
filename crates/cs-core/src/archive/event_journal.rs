//! Archive event journal.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArchiveEvent {
    SegmentUploaded {
        segment_id: String,
        epoch: u64,
        message_count: usize,
    },
    SegmentDownloaded {
        segment_id: String,
    },
    SegmentCompacted {
        old_ids: Vec<String>,
        new_id: String,
    },
    ManifestUploaded {
        generation: u64,
        segment_count: usize,
    },
    EpochRotated {
        old_epoch: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEventJournal {
    pub events: Vec<ArchiveEvent>,
}

impl ArchiveEventJournal {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn record(&mut self, event: ArchiveEvent) {
        self.events.push(event);
    }
}

impl Default for ArchiveEventJournal {
    fn default() -> Self {
        Self::new()
    }
}
