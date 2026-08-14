//! Hydration — rehydration requests for offloaded messages.

use crate::local_store::LocalStoreDb;
use crate::message::processor::{IngestedMessage, MessagePersister};
use crate::transport::ChatStorageTransport;
use crate::HydrationReason;
use uuid::Uuid;

/// A hydration request for an offloaded message.
#[derive(Debug, Clone)]
pub struct HydrationRequest {
    pub message_id: Uuid,
    pub reason: HydrationReason,
}

/// Hydration queue — manages pending rehydration requests.
#[derive(Debug, Default)]
pub struct HydrationQueue {
    queue: Vec<HydrationRequest>,
}

impl HydrationQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, request: HydrationRequest) {
        // Deduplicate by message_id
        if !self
            .queue
            .iter()
            .any(|r| r.message_id == request.message_id)
        {
            self.queue.push(request);
        }
    }

    pub fn drain(&mut self) -> Vec<HydrationRequest> {
        std::mem::take(&mut self.queue)
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Process pending hydration requests by fetching messages from the gateway.
    pub fn process(
        &mut self,
        db: &LocalStoreDb,
        transport: &dyn ChatStorageTransport,
    ) -> Result<usize, crate::Error> {
        let requests = self.drain();
        let mut hydrated = 0;

        for req in &requests {
            // Look up the skeleton to get the conversation_id for fetching.
            // If the skeleton is missing, the message was never ingested locally
            // and we can't know which conversation to fetch from.
            let skel = match db.get_message_skeleton(&req.message_id.to_string()) {
                Ok(Some(s)) => s,
                Ok(None) => continue,
                Err(_) => continue,
            };

            // Fetch the message page from the gateway using the correct conversation_id
            let fetch_result = transport
                .fetch_messages(&skel.conversation_id, Some(&req.message_id.to_string()))
                .map_err(|e| crate::Error::Storage(e.to_string().into()))?;

            if fetch_result.messages.is_empty() {
                continue;
            }

            // Convert and ingest each fetched message
            for raw in &fetch_result.messages {
                let msg = IngestedMessage {
                    message_id: raw.message_id.clone(),
                    conversation_id: raw.conversation_id.clone(),
                    sender_id: raw.sender_id.clone(),
                    created_at_ms: raw.created_at_ms,
                    text_content: raw.text_content.clone(),
                    media_descriptors: vec![],
                    reply_to: None,
                };

                let persister = MessagePersister::new(db);
                persister
                    .persist_ingested_message(&msg)
                    .map_err(crate::Error::from)?;
                hydrated += 1;
            }
        }

        Ok(hydrated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup() {
        let mut q = HydrationQueue::new();
        let id = Uuid::now_v7();
        q.push(HydrationRequest {
            message_id: id,
            reason: HydrationReason::UserTap,
        });
        q.push(HydrationRequest {
            message_id: id,
            reason: HydrationReason::SearchHit,
        });
        assert_eq!(q.len(), 1); // deduplicated
    }
}
