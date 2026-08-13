//! Performance monitoring.

use std::time::Instant;

/// A simple timer for measuring operation latency.
#[derive(Debug)]
pub struct PerfTimer {
    start: Instant,
    label: &'static str,
}

impl PerfTimer {
    pub fn start(label: &'static str) -> Self {
        Self {
            start: Instant::now(),
            label,
        }
    }

    pub fn elapsed_ms(&self) -> u128 {
        self.start.elapsed().as_millis()
    }

    pub fn label(&self) -> &'static str {
        self.label
    }
}

impl std::fmt::Display for PerfTimer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}ms", self.label, self.elapsed_ms())
    }
}
