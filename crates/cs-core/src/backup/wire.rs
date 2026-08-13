//! Backup wire format helpers.

use crate::formats::backup_manifest::*;

/// Encode a manifest payload for transmission.
pub fn encode_manifest(payload: &BackupManifestPayload) -> Result<Vec<u8>, crate::Error> {
    encode_payload(payload)
}

/// Decode a manifest payload from received data.
pub fn decode_manifest(data: &[u8]) -> Result<BackupManifestPayload, crate::Error> {
    decode_payload(data)
}
