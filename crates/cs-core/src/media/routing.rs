//! Media blob sink routing.

use crate::config::MediaBlobSink;

/// Resolve the media blob sink for a given configuration.
pub fn resolve_sink(config: &MediaBlobSink) -> &MediaBlobSink {
    config
}
