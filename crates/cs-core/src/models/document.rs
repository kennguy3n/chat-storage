//! Document embedding (stub).

use crate::models::ModelError;

pub fn embed_document(_text: &str) -> Result<Vec<f32>, ModelError> {
    Err(ModelError::NotAvailable(
        "document embedding not available without onnx feature".into(),
    ))
}
