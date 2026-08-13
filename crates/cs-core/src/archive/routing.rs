//! Archive routing — route to kdrive gateway or ZK Object Fabric.

use crate::config::ArchiveBackend;

/// Determine the archive backend for the current configuration.
pub fn resolve_backend(config: &ArchiveBackend) -> ArchiveBackend {
    *config
}
