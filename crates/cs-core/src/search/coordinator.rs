//! Search coordinator — coordinates local + cold search and hydration requests.

/// The search coordinator manages search execution across local and cold
/// shards, and queues hydration requests for messages found in cold shards.
#[derive(Debug)]
pub struct Coordinator {
    /// Pending hydration requests.
    hydration_queue: Vec<crate::offload::hydration::HydrationRequest>,
}

impl Coordinator {
    pub fn new() -> Self {
        Self {
            hydration_queue: Vec::new(),
        }
    }

    /// Queue a hydration request for a message found in cold search.
    pub fn queue_hydration(&mut self, message_id: uuid::Uuid, reason: crate::HydrationReason) {
        self.hydration_queue
            .push(crate::offload::hydration::HydrationRequest { message_id, reason });
    }

    /// Drain pending hydration requests.
    pub fn drain_hydration(&mut self) -> Vec<crate::offload::hydration::HydrationRequest> {
        std::mem::take(&mut self.hydration_queue)
    }
}

impl Default for Coordinator {
    fn default() -> Self {
        Self::new()
    }
}
