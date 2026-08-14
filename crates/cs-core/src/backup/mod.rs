//! Backup engine — chained manifests, segment builder, sinks, compaction.

pub mod compaction;
pub mod coordinator;
pub mod dedup;
pub mod event_journal;
pub mod manifest_builder;
pub mod segment_builder;
pub mod sinks;
pub mod snapshot;
pub mod wire;
