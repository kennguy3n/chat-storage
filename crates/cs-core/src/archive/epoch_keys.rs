//! Epoch key management for the archive engine.

use crate::crypto::{key_bridge, key_wrap, Key32};

/// Manages archive epoch keys, including rotation and wrapping
/// of prior epoch keys for restore.
#[derive(Debug)]
pub struct EpochKeyManager {
    archive_root: Key32,
    current_epoch: u64,
    /// Wrapped prior epoch keys (for restore).
    wrapped_prior_epochs: Vec<(u64, Vec<u8>)>,
}

impl EpochKeyManager {
    pub fn new(wrapping_key: &Key32) -> Result<Self, crate::crypto::CryptoError> {
        let archive_root = key_bridge::derive_archive_root(wrapping_key)?;
        Ok(Self {
            archive_root,
            current_epoch: current_epoch_id(),
            wrapped_prior_epochs: Vec::new(),
        })
    }

    /// Get the current epoch ID.
    pub fn current_epoch(&self) -> u64 {
        self.current_epoch
    }

    /// Get the current epoch key.
    pub fn current_epoch_key(&self) -> Result<Key32, crate::crypto::CryptoError> {
        key_bridge::derive_archive_epoch(&self.archive_root, self.current_epoch)
    }

    /// Get an epoch key for a specific epoch (from wrapped prior epochs).
    pub fn epoch_key(&self, epoch_id: u64) -> Option<Key32> {
        if epoch_id == self.current_epoch {
            key_bridge::derive_archive_epoch(&self.archive_root, epoch_id).ok()
        } else {
            // Unwrap from stored wrapped keys
            for (stored_epoch, wrapped) in &self.wrapped_prior_epochs {
                if *stored_epoch == epoch_id {
                    return key_wrap::unwrap_key(&self.archive_root, wrapped).ok();
                }
            }
            None
        }
    }

    /// Rotate to a new epoch. Wraps the current epoch key before switching.
    pub fn rotate(&mut self) -> Result<(), crate::Error> {
        let old_key = self.current_epoch_key()?;
        let wrapped = key_wrap::wrap_key(&self.archive_root, &old_key)?;
        self.wrapped_prior_epochs
            .push((self.current_epoch, wrapped));
        self.current_epoch += 1;
        Ok(())
    }

    /// Get wrapped prior epoch keys for manifest inclusion.
    pub fn wrapped_prior_epochs(&self) -> &[(u64, Vec<u8>)] {
        &self.wrapped_prior_epochs
    }

    /// M6: Prune old epoch key entries, keeping only the last `keep_count`.
    /// Defaults to keeping 12 entries.
    pub fn prune_old_epochs(&mut self, keep_count: usize) {
        let keep_count = keep_count.max(1);
        if self.wrapped_prior_epochs.len() > keep_count {
            let start = self.wrapped_prior_epochs.len() - keep_count;
            self.wrapped_prior_epochs = self.wrapped_prior_epochs.split_off(start);
        }
    }

    /// M6: Prune old epoch key entries, keeping the default of 12 entries.
    pub fn prune_old_epochs_default(&mut self) {
        self.prune_old_epochs(12);
    }
}

/// Compute the current epoch ID based on monthly rotation.
pub fn current_epoch_id() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Monthly epochs: years * 12 + months
    let years = now / (365 * 24 * 3600);
    let remaining = now % (365 * 24 * 3600);
    let months = remaining / (30 * 24 * 3600);
    years * 12 + months
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_key_manager() {
        let mgr = EpochKeyManager::new(&[0x42u8; 32]).unwrap();
        let key1 = mgr.current_epoch_key().unwrap();
        assert!(!key1.iter().all(|&b| b == 0));
    }
}
