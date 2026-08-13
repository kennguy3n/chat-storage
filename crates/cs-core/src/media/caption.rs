//! Caption generation via CLIP model (stub — requires ONNX feature).

pub fn generate_caption(_plaintext: &[u8], _mime_type: &str) -> Result<String, crate::Error> {
    Err(crate::Error::NotImplemented(
        "media::caption::generate_caption",
    ))
}
