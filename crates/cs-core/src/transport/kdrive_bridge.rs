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

use std::sync::Arc;

use crate::transport::{ChatStorageTransport, FetchResult, RawDeliveryMessage, TransportError};

/// HTTP-based transport that talks to the kdrive Go gateway.
#[derive(Debug, Clone)]
pub struct KdriveTransport {
    base_url: String,
    tenant_id: String,
    user_id: String,
    client: Arc<reqwest::blocking::Client>,
}

impl KdriveTransport {
    pub fn new(base_url: String, tenant_id: String, user_id: String) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self {
            base_url,
            tenant_id,
            user_id,
            client: Arc::new(client),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn add_demo_headers(
        &self,
        req: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        req.header("X-Demo-Tenant", &self.tenant_id)
            .header("X-Demo-User", &self.user_id)
            .header("Content-Type", "application/octet-stream")
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

        let resp = self
            .add_demo_headers(self.client.get(self.url(&url)))
            .header("Accept", "application/json")
            .send()
            .map_err(|e| TransportError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(TransportError::Server(format!("HTTP {}", resp.status())));
        }

        let body: MessagesResponse = resp
            .json()
            .map_err(|e| TransportError::Network(format!("decode: {}", e)))?;

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
        let resp = self
            .add_demo_headers(self.client.post(self.url(&url)))
            .body(ciphertext.to_vec())
            .send()
            .map_err(|e| TransportError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(TransportError::Server(format!("HTTP {}", resp.status())));
        }

        Ok(segment_id.to_string())
    }

    fn download_archive_segment(&self, segment_id: &str) -> Result<Vec<u8>, TransportError> {
        let url = format!("/v1/chat/archive/segments/{}", segment_id);
        let resp = self
            .add_demo_headers(self.client.get(self.url(&url)))
            .send()
            .map_err(|e| TransportError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(TransportError::Server(format!("HTTP {}", resp.status())));
        }

        resp.bytes()
            .map(|b| b.to_vec())
            .map_err(|e| TransportError::Network(e.to_string()))
    }

    fn fetch_archive_manifests(
        &self,
        after_generation: u64,
    ) -> Result<Vec<Vec<u8>>, TransportError> {
        let url = format!("/v1/chat/archive/manifests?after={}", after_generation);
        let resp = self
            .add_demo_headers(self.client.get(self.url(&url)))
            .header("Accept", "application/json")
            .send()
            .map_err(|e| TransportError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(TransportError::Server(format!("HTTP {}", resp.status())));
        }

        let body: ManifestsResponse = resp
            .json()
            .map_err(|e| TransportError::Network(format!("decode: {}", e)))?;

        Ok(body.manifests)
    }

    fn upload_archive_manifest(&self, manifest: &[u8]) -> Result<(), TransportError> {
        let url = "/v1/chat/archive/manifests";
        let resp = self
            .add_demo_headers(self.client.post(self.url(url)))
            .body(manifest.to_vec())
            .send()
            .map_err(|e| TransportError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(TransportError::Server(format!("HTTP {}", resp.status())));
        }

        Ok(())
    }

    fn upload_search_shard(
        &self,
        shard_key: &str,
        ciphertext: &[u8],
    ) -> Result<(), TransportError> {
        let url = format!("/v1/chat/search/shards/{}", shard_key);
        let resp = self
            .add_demo_headers(self.client.post(self.url(&url)))
            .body(ciphertext.to_vec())
            .send()
            .map_err(|e| TransportError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(TransportError::Server(format!("HTTP {}", resp.status())));
        }

        Ok(())
    }

    fn download_search_shard(&self, shard_key: &str) -> Result<Vec<u8>, TransportError> {
        let url = format!("/v1/chat/search/shards/{}", shard_key);
        let resp = self
            .add_demo_headers(self.client.get(self.url(&url)))
            .send()
            .map_err(|e| TransportError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(TransportError::Server(format!("HTTP {}", resp.status())));
        }

        resp.bytes()
            .map(|b| b.to_vec())
            .map_err(|e| TransportError::Network(e.to_string()))
    }

    fn upload_backup_manifest(&self, manifest: &[u8]) -> Result<(), TransportError> {
        let url = "/v1/chat/backup/manifests";
        let resp = self
            .add_demo_headers(self.client.post(self.url(url)))
            .body(manifest.to_vec())
            .send()
            .map_err(|e| TransportError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(TransportError::Server(format!("HTTP {}", resp.status())));
        }

        Ok(())
    }

    fn fetch_backup_manifests(
        &self,
        after_generation: u64,
    ) -> Result<Vec<Vec<u8>>, TransportError> {
        let url = format!("/v1/chat/backup/manifests?after={}", after_generation);
        let resp = self
            .add_demo_headers(self.client.get(self.url(&url)))
            .header("Accept", "application/json")
            .send()
            .map_err(|e| TransportError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(TransportError::Server(format!("HTTP {}", resp.status())));
        }

        let body: ManifestsResponse = resp
            .json()
            .map_err(|e| TransportError::Network(format!("decode: {}", e)))?;

        Ok(body.manifests)
    }
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
