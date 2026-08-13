//! Media chunker — uses KDRV1 chunk sizing.

/// Compute the KDRV1 chunk size for a given file size.
pub fn chunk_size_for(file_size: u64) -> u64 {
    // Delegate to kdrive-rust-sdk's select_chunk_size
    kchat_drive_crypto::select_chunk_size(file_size) as u64
}

/// Split plaintext into chunks of the computed size.
pub fn chunk(plaintext: &[u8]) -> Vec<&[u8]> {
    let chunk_size = chunk_size_for(plaintext.len() as u64) as usize;
    plaintext.chunks(chunk_size.max(1)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk() {
        let data = vec![0u8; 10 * 1024 * 1024]; // 10 MB
        let chunks = chunk(&data);
        assert!(chunks.len() > 1);
    }
}
