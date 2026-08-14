//! Module 11: Transport resilience tests.

use cs_core::transport::circuit_breaker::{BreakerState, CircuitBreaker};
use cs_core::transport::offline::{OfflineQueue, PendingOperation};
use cs_core::transport::request_coalescer::RequestCoalescer;
use cs_core::transport::ChatStorageTransport;
use std::time::Duration;

// --- Circuit breaker ---

#[test]
fn transport_circuit_breaker_closed_to_open() {
    let breaker = CircuitBreaker::new(3);
    assert_eq!(breaker.state(), BreakerState::Closed);
    assert!(breaker.allow_request());

    breaker.record_failure();
    breaker.record_failure();
    breaker.record_failure();

    assert_eq!(breaker.state(), BreakerState::Open);
    assert!(!breaker.allow_request());
}

#[test]
fn transport_circuit_breaker_open_rejects() {
    let breaker = CircuitBreaker::new(2);
    breaker.record_failure();
    breaker.record_failure();

    assert!(!breaker.allow_request(), "should reject when open");
}

#[test]
fn transport_circuit_breaker_half_open_recovery() {
    let breaker = CircuitBreaker::new(2).with_recovery_timeout(Duration::from_millis(50));
    breaker.record_failure();
    breaker.record_failure();
    assert_eq!(breaker.state(), BreakerState::Open);

    // Wait for recovery timeout
    std::thread::sleep(Duration::from_millis(60));

    // Should transition to half-open
    assert_eq!(breaker.state(), BreakerState::HalfOpen);
    assert!(
        breaker.allow_request(),
        "half-open should allow a probe request"
    );

    // Successful probe → close
    breaker.record_success();
    assert_eq!(breaker.state(), BreakerState::Closed);
}

#[test]
fn transport_circuit_breaker_success_resets() {
    let breaker = CircuitBreaker::new(5);
    breaker.record_failure();
    breaker.record_failure();
    breaker.record_success();

    assert_eq!(breaker.state(), BreakerState::Closed);
    assert!(breaker.allow_request());
}

// --- Request coalescer ---

#[test]
fn transport_request_coalescer_dedup() {
    let coalescer = RequestCoalescer::new();

    // First acquire succeeds
    assert!(coalescer.try_acquire("key-1"));
    // Second acquire for same key fails
    assert!(!coalescer.try_acquire("key-1"));
    // Different key succeeds
    assert!(coalescer.try_acquire("key-2"));

    // Release key-1 and try again
    coalescer.release("key-1");
    assert!(coalescer.try_acquire("key-1"));
}

#[test]
fn transport_request_coalescer_release() {
    let coalescer = RequestCoalescer::new();
    coalescer.try_acquire("key-a");
    coalescer.release("key-a");
    // Should be able to re-acquire after release
    assert!(coalescer.try_acquire("key-a"));
}

// --- Offline queue ---

#[test]
fn transport_offline_queue() {
    let mut queue = OfflineQueue::new();
    assert!(queue.is_empty());

    queue.enqueue(PendingOperation {
        op_type: "send".to_string(),
        payload: b"msg-1".to_vec(),
        created_at_ms: 1_700_000_000_000,
    });
    queue.enqueue(PendingOperation {
        op_type: "send".to_string(),
        payload: b"msg-2".to_vec(),
        created_at_ms: 1_700_000_001_000,
    });
    queue.enqueue(PendingOperation {
        op_type: "send".to_string(),
        payload: b"msg-3".to_vec(),
        created_at_ms: 1_700_000_002_000,
    });

    assert_eq!(queue.len(), 3);

    let drained = queue.drain();
    assert_eq!(drained.len(), 3);
    assert_eq!(drained[0].payload, b"msg-1");
    assert_eq!(drained[1].payload, b"msg-2");
    assert_eq!(drained[2].payload, b"msg-3");

    // Queue should be empty after drain
    assert!(queue.is_empty());
}

#[test]
fn transport_offline_empty() {
    let mut queue = OfflineQueue::new();
    let drained = queue.drain();
    assert!(drained.is_empty());
}

// --- KdriveTransport roundtrip (requires gateway) ---

#[test]
#[ignore]
fn transport_kdrive_roundtrip() {
    let gateway = crate::harness::GatewayHarness::start().expect("gateway failed");
    let transport = cs_core::transport::kdrive_bridge::KdriveTransport::new(
        gateway.base_url.clone(),
        "test-token".to_string(),
        "tenant-test".to_string(),
        "user-test".to_string(),
    );

    let segment_id = format!("seg-{}", uuid::Uuid::now_v7());
    let data = b"test segment data for roundtrip";

    // Upload
    let uploaded_id = transport
        .upload_archive_segment(&segment_id, data)
        .expect("upload failed");
    assert_eq!(uploaded_id, segment_id);

    // Download
    let downloaded = transport
        .download_archive_segment(&segment_id)
        .expect("download failed");
    assert_eq!(downloaded, data);
}

#[test]
#[ignore]
fn transport_kdrive_404() {
    let gateway = crate::harness::GatewayHarness::start().expect("gateway failed");
    let transport = cs_core::transport::kdrive_bridge::KdriveTransport::new(
        gateway.base_url.clone(),
        "test-token".to_string(),
        "tenant-test".to_string(),
        "user-test".to_string(),
    );

    let result = transport.download_archive_segment("nonexistent-segment-id");
    assert!(
        result.is_err(),
        "downloading non-existent segment should error"
    );
}
