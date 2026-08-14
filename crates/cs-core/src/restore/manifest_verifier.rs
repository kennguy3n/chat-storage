//! Manifest verifier — verify manifest chain integrity.

use crate::formats::manifest::BackupManifest;
use crate::local_store::StorageError;

/// Verify the manifest chain: check `previous_manifest_hash` links.
pub fn verify_chain(manifests: &[BackupManifest]) -> Result<(), crate::Error> {
    if manifests.is_empty() {
        return Ok(());
    }

    // First manifest should have zero previous hash
    if manifests[0].generation != 0 && manifests[0].previous_manifest_hash != [0u8; 32] {
        return Err(StorageError::Custom(
            "first manifest has non-zero previous_manifest_hash".into(),
        )
        .into());
    }

    for i in 1..manifests.len() {
        let prev_hash = crate::backup::manifest_builder::manifest_hash(&manifests[i - 1])?;
        if manifests[i].previous_manifest_hash != prev_hash {
            return Err(StorageError::Custom(format!(
                "manifest chain broken at generation {}",
                manifests[i].generation
            ))
            .into());
        }
    }

    Ok(())
}
