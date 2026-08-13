//! E2E test suite for chat-storage cs-core.
//!
//! Tests that require the kdrive Go gateway are marked `#[ignore]`.
//! Run all tests: `cargo test --test e2e`
//! Run gateway tests: `cargo test --test e2e -- --ignored`
//! Run everything: `cargo test --test e2e -- --include-ignored`

mod harness;
mod helpers;

mod e2e_archive;
mod e2e_backup;
mod e2e_knowledge;
mod e2e_media;
mod e2e_message;
mod e2e_offload;
mod e2e_restore;
mod e2e_search_fts;
mod e2e_search_fuzzy;
mod e2e_search_query;
mod e2e_tenant;
mod e2e_transport;
