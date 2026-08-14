//! kdrive transport bridge — implements `ChatStorageTransport` via HTTP
//! calls to the kdrive Go gateway.
//!
//! All endpoints are extensions to the kdrive gateway's `/v1/chat/` API:
//! - `POST /v1/chat/archive/segments/{id}` — upload archive segment
//! - `GET  /v1/chat/archive/segments/{id}` — download archive segment
//! - `GET  /v1/chat/archive/manifests?after={gen}` — fetch archive manifests
//! - `POST /v1/chat/archive/manifests` — upload archive manifest
//! - `POST /v1/chat/search/shards/{key}` — upload search index shard
//! - `GET  /v1/chat/search/shards/{key}` — download search index shard
//! - `POST /v1/chat/backup/manifests` — upload backup manifest
//! - `GET  /v1/chat/backup/manifests?after={gen}` — fetch backup manifests
//! - `GET  /v1/chat/messages/{conversation_id}?after={cursor}` — fetch messages
//!
//! ## Production features
//!
//! - **Bearer token authentication** via `Authorization` header
//! - **Retry with exponential backoff** for transient failures (5xx, 429, network)
//! - **Circuit breaker** to avoid cascading failures when the gateway is down
//! - **Proper HTTP status code mapping** to `TransportError` variants

use std::sync::Arc;
use std::time::Duration;

use crate::transport::circuit_breaker::CircuitBreaker;
use crate::transport::{ChatStorageTransport, FetchResult, RawDeliveryMessage, TransportError};

/// Default request timeout (30 seconds).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default maximum retry attempts for transient failures.
const DEFAULT_MAX_RETRIES: u32 = 3;

/// Base delay for exponential backoff (100ms → 200ms → 400ms).
const BASE_RETRY_DELAY: Duration = Duration::from_millis(100);

/// HTTP-based transport that talks to the kdrive Go gateway.
#[derive(Debug)]
pub struct KdriveTransport {
    base_url: String,
    auth_token: String,
    tenant_id: String,
    user_id: String,
    client: Arc<reqwest::blocking::Client>,
    breaker: CircuitBreaker,
    max_retries: u32,
}

impl KdriveTransport {
    /// Create a new `KdriveTransport` with Bearer token authentication.
    ///
    /// The `auth_token` is sent as `Authorization: Bearer {auth_token}` on
    /// every request. The `tenant_id` and `user_id` are sent as
    /// `X-Tenant-Id` and `X-User-Id` headers for multi-tenant routing.
    pub fn new(base_url: String, auth_token: String, tenant_id: String, user_id: String) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self {
            base_url,
            auth_token,
            tenant_id,
            user_id,
            client: Arc::new(client),
            breaker: CircuitBreaker::new(5).with_recovery_timeout(Duration::from_secs(30)),
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }

    /// Set a custom request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        self.client = Arc::new(client);
        self
    }

    /// Set the maximum number of retry attempts for transient failures.
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Add authentication and routing headers to a request builder.
    fn add_headers(
        &self,
        req: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        req.header("Authorization", format!("Bearer {}", self.auth_token))
            .header("X-Tenant-Id", &self.tenant_id)
            .header("X-User-Id", &self.user_id)
            .header("Content-Type", "application/octet-stream")
    }

    /// Send a request with retry, circuit breaker, and proper error mapping.
    ///
    /// Retries on: network errors, 429 Too Many Requests, 5xx server errors.
    /// Does not retry on: 4xx (except 429), auth errors (401/403).
    fn send_with_retry(
        &self,
        build_request: impl Fn() -> reqwest::blocking::RequestBuilder,
    ) -> Result<reqwest::blocking::Response, TransportError> {
        let mut last_err = TransportError::Custom("no attempts made".to_string());

        for attempt in 0..=self.max_retries {
            // Check circuit breaker before each attempt
            if !self.breaker.allow_request() {
                return Err(TransportError::Server(
                    "circuit breaker open — gateway unavailable".to_string(),
                ));
            }

            let req = self.add_headers(build_request());
            match req.send() {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        self.breaker.record_success();
                        return Ok(resp);
                    }

                    // Map HTTP status to TransportError
                    let err = map_http_error(status);

                    // Check if retryable
                    if is_retryable_status(status) && attempt < self.max_retries {
                        self.breaker.record_failure();
                        last_err = err;
                        let delay = BASE_RETRY_DELAY * 2u32.saturating_pow(attempt);
                        std::thread::sleep(delay);
                        continue;
                    }

                    // Non-retryable error or out of retries
                    self.breaker.record_failure();
                    return Err(err);
                }
                Err(e) => {
                    // Network error — retryable
                    self.breaker.record_failure();
                    let err = TransportError::Network(e.to_string());
                    if attempt < self.max_retries {
                        last_err = err;
                        let delay = BASE_RETRY_DELAY * 2u32.saturating_pow(attempt);
                        std::thread::sleep(delay);
                        continue;
                    }
                    return Err(err);
                }
            }
        }

        Err(last_err)
    }

    /// Send a request and return the response body as bytes, with retry.
    fn send_and_get_bytes(
        &self,
        build_request: impl Fn() -> reqwest::blocking::RequestBuilder,
    ) -> Result<Vec<u8>, TransportError> {
        let resp = self.send_with_retry(build_request)?;
        resp.bytes()
            .map(|b| b.to_vec())
            .map_err(|e| TransportError::Network(e.to_string()))
    }

    /// Send a request and deserialize JSON, with retry.
    fn send_and_get_json<T: serde::de::DeserializeOwned>(
        &self,
        build_request: impl Fn() -> reqwest::blocking::RequestBuilder,
    ) -> Result<T, TransportError> {
        let resp = self.send_with_retry(build_request)?;
        resp.json::<T>()
            .map_err(|e| TransportError::Network(format!("decode: {e}")))
    }
}

impl ChatStorageTransport for KdriveTransport {
    fn fetch_messages(
        &self,
        conversation_id: &str,
        after_cursor: Option<&str>,
    ) -> Result<FetchResult, TransportError> {
        let mut url = format!("/v1/chat/messages/{}", conversation_id);
        if let Some(cursor) = after_cursor {
            url.push_str("?after=");
            url.push_str(&crate::util::url_encode(cursor));
        }

        let body: MessagesResponse = self.send_and_get_json(|| {
            self.client
                .get(self.url(&url))
                .header("Accept", "application/json")
        })?;

        Ok(FetchResult {
            messages: body.messages,
            next_cursor: body.next_cursor,
        })
    }

    fn upload_archive_segment(
        &self,
        segment_id: &str,
        ciphertext: &[u8],
    ) -> Result<String, TransportError> {
        let url = format!("/v1/chat/archive/segments/{}", segment_id);
        self.send_with_retry(|| self.client.post(self.url(&url)).body(ciphertext.to_vec()))?;
        Ok(segment_id.to_string())
    }

    fn download_archive_segment(&self, segment_id: &str) -> Result<Vec<u8>, TransportError> {
        let url = format!("/v1/chat/archive/segments/{}", segment_id);
        self.send_and_get_bytes(|| self.client.get(self.url(&url)))
    }

    fn fetch_archive_manifests(
        &self,
        after_generation: u64,
    ) -> Result<Vec<Vec<u8>>, TransportError> {
        let url = format!("/v1/chat/archive/manifests?after={}", after_generation);
        let body: ManifestsResponse = self.send_and_get_json(|| {
            self.client
                .get(self.url(&url))
                .header("Accept", "application/json")
        })?;
        Ok(body.manifests)
    }

    fn upload_archive_manifest(&self, manifest: &[u8]) -> Result<(), TransportError> {
        let url = "/v1/chat/archive/manifests";
        self.send_with_retry(|| self.client.post(self.url(url)).body(manifest.to_vec()))?;
        Ok(())
    }

    fn upload_search_shard(
        &self,
        shard_key: &str,
        ciphertext: &[u8],
    ) -> Result<(), TransportError> {
        let url = format!("/v1/chat/search/shards/{}", shard_key);
        self.send_with_retry(|| self.client.post(self.url(&url)).body(ciphertext.to_vec()))?;
        Ok(())
    }

    fn download_search_shard(&self, shard_key: &str) -> Result<Vec<u8>, TransportError> {
        let url = format!("/v1/chat/search/shards/{}", shard_key);
        self.send_and_get_bytes(|| self.client.get(self.url(&url)))
    }

    fn upload_backup_manifest(&self, manifest: &[u8]) -> Result<(), TransportError> {
        let url = "/v1/chat/backup/manifests";
        self.send_with_retry(|| self.client.post(self.url(url)).body(manifest.to_vec()))?;
        Ok(())
    }

    fn fetch_backup_manifests(
        &self,
        after_generation: u64,
    ) -> Result<Vec<Vec<u8>>, TransportError> {
        let url = format!("/v1/chat/backup/manifests?after={}", after_generation);
        let body: ManifestsResponse = self.send_and_get_json(|| {
            self.client
                .get(self.url(&url))
                .header("Accept", "application/json")
        })?;
        Ok(body.manifests)
    }

    fn upload_media_blob(
        &self,
        blob_id: &str,
        ciphertext: &[u8],
    ) -> Result<String, TransportError> {
        let url = format!("/v1/chat/media/{}", blob_id);
        self.send_with_retry(|| self.client.post(self.url(&url)).body(ciphertext.to_vec()))?;
        Ok(blob_id.to_string())
    }

    fn download_media_blob(&self, blob_id: &str) -> Result<Vec<u8>, TransportError> {
        let url = format!("/v1/chat/media/{}", blob_id);
        self.send_and_get_bytes(|| self.client.get(self.url(&url)))
    }

    fn download_backup_segment(&self, segment_id: &str) -> Result<Vec<u8>, TransportError> {
        let url = format!("/v1/chat/backup/segment/{}", segment_id);
        self.send_and_get_bytes(|| self.client.get(self.url(&url)))
    }
}

// ---------------------------------------------------------------------------
// HTTP status code mapping
// ---------------------------------------------------------------------------

/// Map an HTTP status code to the appropriate `TransportError` variant.
fn map_http_error(status: reqwest::StatusCode) -> TransportError {
    match status.as_u16() {
        401 | 403 => TransportError::Auth(format!("HTTP {} — authentication failed", status)),
        404 => TransportError::Server(format!("HTTP {} — not found", status)),
        429 => TransportError::Server(format!("HTTP {} — rate limited", status)),
        s if s >= 500 => TransportError::Server(format!("HTTP {} — server error", status)),
        s if s >= 400 => TransportError::Server(format!("HTTP {} — client error", status)),
        _ => TransportError::Server(format!("HTTP {}", status)),
    }
}

/// Whether an HTTP status code indicates a retryable transient failure.
fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status.as_u16() == 429 || status.as_u16() >= 500
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct MessagesResponse {
    messages: Vec<RawDeliveryMessage>,
    next_cursor: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ManifestsResponse {
    manifests: Vec<Vec<u8>>,
}
