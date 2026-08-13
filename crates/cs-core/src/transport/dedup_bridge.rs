//! Dedup transport bridge — implements `DedupTransport` via HTTP calls
//! to the kdrive gateway's dedup endpoints.
//!
//! Endpoints:
//! - `POST /v1/content:check` — check if content exists
//! - `POST /v1/content:checkChunks` — check which chunks exist
//! - `POST /v1/content:register` — register content blob
//! - `POST /v1/uploads:commitDedup` — commit version with dedup info

use std::sync::Arc;

use kchat_drive_transport_core::dedup::{
    ChunkCheckResult, ContentCheckResult, DedupCommitResult, DedupTransport,
};
use kchat_drive_types::{DriveError, Hash256};

/// HTTP-based dedup transport that talks to the kdrive Go gateway.
#[derive(Debug, Clone)]
pub struct DedupBridge {
    base_url: String,
    tenant_id: String,
    user_id: String,
    client: Arc<reqwest::blocking::Client>,
}

impl DedupBridge {
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
            .header("Content-Type", "application/json")
    }
}

impl DedupTransport for DedupBridge {
    fn check_content(&self, content_id: &Hash256) -> Result<ContentCheckResult, DriveError> {
        let body = serde_json::json!({
            "content_id": hex::encode(content_id.as_bytes()),
        });

        let resp = self
            .add_demo_headers(self.client.post(self.url("/v1/content:check")))
            .json(&body)
            .send()
            .map_err(|e| DriveError::InvalidState(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(DriveError::InvalidState(format!("HTTP {}", resp.status())));
        }

        let result: ContentCheckResponse = resp
            .json()
            .map_err(|e| DriveError::InvalidState(format!("decode: {}", e)))?;

        Ok(ContentCheckResult {
            exists: result.exists,
            blob_keys: result.blob_keys.unwrap_or_default(),
            ciphertext_hashes: result.ciphertext_hashes.unwrap_or_default(),
            chunk_count: result.chunk_count.unwrap_or(0),
            plaintext_size: result.plaintext_size.unwrap_or(0),
        })
    }

    fn check_chunks(
        &self,
        content_id: &Hash256,
        chunk_hashes: &[Hash256],
    ) -> Result<ChunkCheckResult, DriveError> {
        let body = serde_json::json!({
            "content_id": hex::encode(content_id.as_bytes()),
            "chunk_hashes": chunk_hashes.iter().map(|h| hex::encode(h.as_bytes())).collect::<Vec<_>>(),
        });

        let resp = self
            .add_demo_headers(self.client.post(self.url("/v1/content:checkChunks")))
            .json(&body)
            .send()
            .map_err(|e| DriveError::InvalidState(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(DriveError::InvalidState(format!("HTTP {}", resp.status())));
        }

        let result: ChunkCheckResponse = resp
            .json()
            .map_err(|e| DriveError::InvalidState(format!("decode: {}", e)))?;

        Ok(ChunkCheckResult {
            results: result.results,
        })
    }

    fn upload_content_blob(
        &self,
        blob_key: &str,
        ciphertext: &[u8],
        ciphertext_sha256: &Hash256,
    ) -> Result<(), DriveError> {
        let url = format!(
            "/v1/content:register?blob_key={}&ciphertext_sha256={}",
            crate::util::url_encode(blob_key),
            hex::encode(ciphertext_sha256.as_bytes()),
        );

        let resp = self
            .add_demo_headers(self.client.post(self.url(&url)))
            .header("Content-Type", "application/octet-stream")
            .body(ciphertext.to_vec())
            .send()
            .map_err(|e| DriveError::InvalidState(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(DriveError::InvalidState(format!("HTTP {}", resp.status())));
        }

        Ok(())
    }

    fn commit_version_dedup(
        &self,
        manifest_ciphertext: &[u8],
        manifest_nonce: &[u8; 12],
        manifest_ciphertext_sha256: &Hash256,
        header: &[u8],
        wrapped_dek: &[u8],
        wrap_nonce: &[u8; 12],
        content_id: &Hash256,
        wrapped_content_key: &[u8],
        content_wrap_nonce: &[u8; 12],
        reused_blob_keys: &[String],
        new_blob_keys: &[String],
    ) -> Result<DedupCommitResult, DriveError> {
        let body = serde_json::json!({
            "manifest_ciphertext": base64_encode(manifest_ciphertext),
            "manifest_nonce": base64_encode(manifest_nonce),
            "manifest_ciphertext_sha256": hex::encode(manifest_ciphertext_sha256.as_bytes()),
            "header": base64_encode(header),
            "wrapped_dek": base64_encode(wrapped_dek),
            "wrap_nonce": base64_encode(wrap_nonce),
            "content_id": hex::encode(content_id.as_bytes()),
            "wrapped_content_key": base64_encode(wrapped_content_key),
            "content_wrap_nonce": base64_encode(content_wrap_nonce),
            "reused_blob_keys": reused_blob_keys,
            "new_blob_keys": new_blob_keys,
        });

        let resp = self
            .add_demo_headers(self.client.post(self.url("/v1/uploads:commitDedup")))
            .json(&body)
            .send()
            .map_err(|e| DriveError::InvalidState(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(DriveError::InvalidState(format!("HTTP {}", resp.status())));
        }

        let result: DedupCommitResponse = resp
            .json()
            .map_err(|e| DriveError::InvalidState(format!("decode: {}", e)))?;

        Ok(DedupCommitResult {
            version_id: result.version_id,
            committed: result.committed.unwrap_or(true),
            deduped_chunks: result.deduped_chunks.unwrap_or(0),
            new_chunks: result.new_chunks.unwrap_or(0),
        })
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct ContentCheckResponse {
    exists: bool,
    blob_keys: Option<Vec<String>>,
    ciphertext_hashes: Option<Vec<String>>,
    chunk_count: Option<u64>,
    plaintext_size: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
struct ChunkCheckResponse {
    results: Vec<kchat_drive_transport_core::dedup::ChunkCheckEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct DedupCommitResponse {
    version_id: String,
    committed: Option<bool>,
    deduped_chunks: Option<u64>,
    new_chunks: Option<u64>,
}

fn base64_encode(data: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data)
}
