//! Thin facade over [`ciborium`] for CBOR encode / decode.
//!
//! All wire-format types in [`crate::formats`] go through this module
//! so the CBOR backend choice lives in one place. The strict
//! trailing-byte semantics match the original `chat-storage-search`
//! contract.

pub use ciborium::value::Integer;
pub use ciborium::value::Value;

/// Result of [`to_vec`].
pub type EncodeError = ciborium::ser::Error<std::io::Error>;

/// Result of [`from_slice`].
pub type DecodeError = ciborium::de::Error<std::io::Error>;

/// Encode `value` to a freshly-allocated CBOR byte vector.
pub fn to_vec<T: serde::Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, EncodeError> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf)?;
    Ok(buf)
}

/// Decode a value of type `T` from a CBOR byte slice.
///
/// Strict: the entire input must be consumed by exactly one CBOR value.
/// Trailing bytes return a `Semantic` error.
pub fn from_slice<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, DecodeError> {
    let mut cursor = std::io::Cursor::new(bytes);
    let value: T = ciborium::from_reader(&mut cursor)?;
    let consumed = cursor.position() as usize;
    if consumed < bytes.len() {
        return Err(ciborium::de::Error::Semantic(
            Some(consumed),
            format!(
                "trailing bytes after CBOR value: {} byte(s) unread at offset {}",
                bytes.len() - consumed,
                consumed,
            ),
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct Sample {
        magic: [u8; 4],
        n: u32,
        msg: String,
    }

    #[test]
    fn roundtrip_struct() {
        let s = Sample {
            magic: *b"CBOR",
            n: 42,
            msg: "hello".into(),
        };
        let bytes = to_vec(&s).expect("encode");
        let back: Sample = from_slice(&bytes).expect("decode");
        assert_eq!(s, back);
    }

    #[test]
    fn from_slice_rejects_trailing_bytes() {
        let mut bytes = to_vec(&42u32).expect("encode");
        let valid_len = bytes.len();
        bytes.extend_from_slice(&[0xff, 0xff, 0xff]);

        let err = from_slice::<u32>(&bytes).expect_err("strict decode must reject trailing bytes");
        match err {
            ciborium::de::Error::Semantic(Some(offset), msg) => {
                assert_eq!(offset, valid_len);
                assert!(msg.contains("trailing bytes"));
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn from_slice_accepts_exact_input() {
        let bytes = to_vec(&"hello").expect("encode");
        let decoded: String = from_slice(&bytes).expect("decode");
        assert_eq!(decoded, "hello");
    }
}
