//! Key recovery — recover archive/backup keys from wrapped epoch keys.

use crate::crypto::{key_wrap, Key32};

/// Recover an epoch key from its wrapped form.
pub fn recover_epoch_key(archive_root: &Key32, wrapped_key: &[u8]) -> Result<Key32, crate::Error> {
    key_wrap::unwrap_key(archive_root, wrapped_key).map_err(Into::into)
}
