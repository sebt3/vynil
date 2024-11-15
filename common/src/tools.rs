use crate::{Error, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
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
