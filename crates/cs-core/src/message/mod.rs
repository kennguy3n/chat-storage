//! Message ingest and persistence pipeline.

pub mod processor;

pub use processor::{IngestedMessage, MessagePersister, MessageProcessor, OutboxEntry};

/// Message pipeline errors.
#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error("validation: {0}")]
    Validation(String),

    #[error("idempotency: {0}")]
    Idempotency(String),

    #[error("image codec: {0}")]
    ImageCodec(String),

    #[error("{0}")]
    Custom(String),
}

impl From<String> for MessageError {
    fn from(s: String) -> Self {
        MessageError::Custom(s)
    }
}
