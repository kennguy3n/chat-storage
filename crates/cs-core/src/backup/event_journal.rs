//! Backup event journal.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupEvent {
    SegmentBuilt {
        segment_id: String,
        size: u64,
    },
    SegmentUploaded {
        segment_id: String,
        size: usize,
    },
    ManifestPublished {
        generation: u64,
    },
    SegmentCompacted {
        old_ids: Vec<String>,
        new_id: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupEventJournal {
    pub events: Vec<BackupEvent>,
}

impl BackupEventJournal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, event: BackupEvent) {
        self.events.push(event);
    }
}
