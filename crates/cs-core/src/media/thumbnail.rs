//! Thumbnail generation (stub — platform-specific impls needed).

pub fn generate_thumbnail(_plaintext: &[u8], _mime_type: &str) -> Result<Vec<u8>, crate::Error> {
    Err(crate::Error::NotImplemented(
        "media::thumbnail::generate_thumbnail",
    ))
}
