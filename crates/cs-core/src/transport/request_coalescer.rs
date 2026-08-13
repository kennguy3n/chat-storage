//! Request coalescer — deduplicate concurrent identical requests.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct RequestCoalescer {
    in_flight: Mutex<HashMap<String, ()>>,
}

impl RequestCoalescer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_acquire(&self, key: &str) -> bool {
        let Ok(mut map) = self.in_flight.lock() else {
            return false;
        };
        if map.contains_key(key) {
            false
        } else {
            map.insert(key.to_string(), ());
            true
        }
    }

    pub fn release(&self, key: &str) {
        if let Ok(mut map) = self.in_flight.lock() {
            map.remove(key);
        }
    }
}
