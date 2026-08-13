//! kdrive gateway media sink (default).

/// Upload a media blob to the kdrive gateway.
pub fn upload_blob(
    _transport: &dyn crate::transport::ChatStorageTransport,
    _blob_key: &str,
    _ciphertext: &[u8],
) -> Result<(), crate::Error> {
    Err(crate::Error::NotImplemented(
        "media::sinks::kdrive_sink::upload_blob",
    ))
}

/// Download a media blob from the kdrive gateway.
pub fn download_blob(
    _transport: &dyn crate::transport::ChatStorageTransport,
    _blob_key: &str,
) -> Result<Vec<u8>, crate::Error> {
    Err(crate::Error::NotImplemented(
        "media::sinks::kdrive_sink::download_blob",
    ))
}
