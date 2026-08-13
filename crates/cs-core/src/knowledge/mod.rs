//! Knowledge extraction & threat detection — on-device pattern matching,
//! link safety, PII redaction, and content classification.

pub mod content_classifier;
pub mod link_safety;
pub mod pii_redaction;
pub mod threat_detection;

/// Knowledge errors.
#[derive(Debug, thiserror::Error)]
pub enum KnowledgeError {
    #[error("model not available: {0}")]
    NotAvailable(String),

    #[error("inference: {0}")]
    Inference(String),

    #[error("{0}")]
    Custom(String),
}

impl From<String> for KnowledgeError {
    fn from(s: String) -> Self {
        KnowledgeError::Custom(s)
    }
}
