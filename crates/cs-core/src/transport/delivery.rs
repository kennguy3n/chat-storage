//! Message delivery client — cursor-paginated fetch from the gateway.

use crate::formats::media_descriptor::MediaDescriptor;
use crate::transport::TransportError;
use serde::{Deserialize, Serialize};

/// One MLS-decrypted message from the delivery store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDeliveryMessage {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub text_content: Option<String>,
    pub media_descriptors: Vec<MediaDescriptor>,
    pub reply_to: Option<String>,
}

/// One page of messages plus the next cursor.
#[derive(Debug, Clone, Default)]
pub struct FetchResult {
    pub messages: Vec<RawDeliveryMessage>,
    pub next_cursor: Option<String>,
}

/// Delivery client trait (object-safe).
pub trait DeliveryClient: Send + Sync {
    fn fetch_messages(
        &self,
        conversation_id: &str,
        after_cursor: Option<&str>,
    ) -> Result<FetchResult, TransportError>;
}
