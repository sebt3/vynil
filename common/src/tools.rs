use crate::{Error, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use std::io::{Read as _, Write};

pub fn encode_base64_gz(data: String) -> Result<String> {
    let bytes = data.into_bytes();
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&bytes).map_err(Error::Stdio)?;
    let tmp = encoder.finish().map_err(Error::Stdio)?;
    Ok(STANDARD.encode(tmp))
}

pub fn base64_gz_decode(data: String) -> Result<String> {
    let b64decoded = STANDARD.decode(data).map_err(Error::Base64DecodeError)?;
    let mut gz = GzDecoder::new(&b64decoded[..]);
    let mut s = String::new();
    gz.read_to_string(&mut s).map_err(Error::Stdio)?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let original = "hello world\nsome yaml:\n  key: value\n";
        let encoded = encode_base64_gz(original.to_string()).unwrap();
        let decoded = base64_gz_decode(encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_encode_decode_empty_string() {
        let encoded = encode_base64_gz("".to_string()).unwrap();
        let decoded = base64_gz_decode(encoded).unwrap();
        assert_eq!(decoded, "");
    }

    #[test]
    fn test_encode_decode_large_content() {
        let original = "x".repeat(10_000);
        let encoded = encode_base64_gz(original.clone()).unwrap();
        // Compression should reduce size for repetitive content
        assert!(encoded.len() < original.len());
        let decoded = base64_gz_decode(encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_decode_invalid_base64() {
        assert!(base64_gz_decode("not-valid-base64!!!".to_string()).is_err());
    }
}
