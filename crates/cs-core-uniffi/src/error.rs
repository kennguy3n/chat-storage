/// UniFFI-compatible error type for chat-storage operations.
///
/// Maps `cs_core::Error` variants into a flat enum that UniFFI can
/// expose to Swift/Kotlin. Each variant carries a human-readable
/// message string.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ChatStorageError {
    #[error("crypto error: {msg}")]
    Crypto { msg: String },

    #[error("storage error: {msg}")]
    Storage { msg: String },

    #[error("search error: {msg}")]
    Search { msg: String },

    #[error("message error: {msg}")]
    Message { msg: String },

    #[error("transport error: {msg}")]
    Transport { msg: String },

    #[error("model error: {msg}")]
    Model { msg: String },

    #[error("tenant error: {msg}")]
    Tenant { msg: String },

    #[error("quota exceeded: {resource} (limit {limit}, current {current})")]
    QuotaExceeded {
        resource: String,
        limit: u64,
        current: u64,
    },

    #[error("invalid input: {msg}")]
    InvalidInput { msg: String },
}

impl From<cs_core::Error> for ChatStorageError {
    fn from(e: cs_core::Error) -> Self {
        match e {
            cs_core::Error::Crypto(c) => ChatStorageError::Crypto { msg: c.to_string() },
            cs_core::Error::Storage(s) => ChatStorageError::Storage { msg: s.to_string() },
            cs_core::Error::Search(s) => ChatStorageError::Search { msg: s.to_string() },
            cs_core::Error::Message(m) => ChatStorageError::Message { msg: m.to_string() },
            cs_core::Error::Transport(t) => ChatStorageError::Transport { msg: t.to_string() },
            cs_core::Error::Model(m) => ChatStorageError::Model { msg: m.to_string() },
            cs_core::Error::Tenant(t) => ChatStorageError::Tenant { msg: t.to_string() },
            cs_core::Error::QuotaExceeded {
                resource,
                limit,
                current,
            } => ChatStorageError::QuotaExceeded {
                resource: resource.to_string(),
                limit,
                current,
            },
            cs_core::Error::NotImplemented(s) => {
                ChatStorageError::InvalidInput { msg: s.to_string() }
            }
        }
    }
}

/// Helper to create an `InvalidInput` error from a string.
pub fn invalid_input(msg: impl Into<String>) -> ChatStorageError {
    ChatStorageError::InvalidInput { msg: msg.into() }
}
