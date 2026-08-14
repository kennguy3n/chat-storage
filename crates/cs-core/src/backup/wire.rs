//! Backup wire format helpers.

use crate::formats::manifest::BackupManifest;

/// Encode a manifest for transmission.
pub fn encode_manifest(manifest: &BackupManifest) -> Result<Vec<u8>, crate::Error> {
    crate::cbor::to_vec(manifest).map_err(|e| crate::Error::Storage(e.to_string().into()))
}

/// Decode a manifest from received data.
pub fn decode_manifest(data: &[u8]) -> Result<BackupManifest, crate::Error> {
    crate::cbor::from_slice(data).map_err(|e| crate::Error::Storage(e.to_string().into()))
}
