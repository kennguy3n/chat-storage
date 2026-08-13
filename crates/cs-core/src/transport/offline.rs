//! Offline queue — persistent backing for offline operations.

#[derive(Debug, Default)]
pub struct OfflineQueue {
    pending: Vec<PendingOperation>,
}

#[derive(Debug, Clone)]
pub struct PendingOperation {
    pub op_type: String,
    pub payload: Vec<u8>,
    pub created_at_ms: i64,
}

impl OfflineQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&mut self, op: PendingOperation) {
        self.pending.push(op);
    }

    pub fn drain(&mut self) -> Vec<PendingOperation> {
        std::mem::take(&mut self.pending)
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}
