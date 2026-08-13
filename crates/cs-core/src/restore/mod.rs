//! Restore engine — skeleton-first restore pipeline.

pub mod key_recovery;
pub mod manifest_verifier;
pub mod pipeline;
pub mod state_machine;

pub use state_machine::RestoreState;
