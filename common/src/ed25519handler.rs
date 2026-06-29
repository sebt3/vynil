use crate::{Error, Result};
use openssl::{pkey::PKey, rsa::Rsa};
use rhai::Engine;

/// Default RSA modulus size (in bits) used when none is provided.
pub const DEFAULT_RSA_BITS: u32 = 4096;

/// Generate a PEM-encoded PKCS#8 private key for the requested algorithm.
///
/// - `ed25519` : asymmetric Ed25519 key (the `bits` argument is ignored).
/// - `rsa`     : RSA key whose modulus size is given by `bits` (e.g. 2048, 4096).
///
/// The output is a standard `-----BEGIN PRIVATE KEY-----` PKCS#8 PEM block, suitable
/// for a Kubernetes Secret `stringData` field. Persistence/idempotency is the caller's
/// responsibility (check-then-create in the package install logic).
pub fn gen_private_key(algo: &str, bits: u32) -> Result<String> {
    let pem = match algo.to_ascii_lowercase().as_str() {
        "ed25519" => PKey::generate_ed25519()?.private_key_to_pem_pkcs8()?,
        "rsa" => PKey::from_rsa(Rsa::generate(bits)?)?.private_key_to_pem_pkcs8()?,
        other => return Err(Error::UnsupportedKeyAlgorithm(other.to_string())),
    };
    String::from_utf8(pem).map_err(Error::UTF8)
}

pub fn key_rhai_register(engine: &mut Engine) {
    engine
        .register_fn("gen_private_key", |algo: &str| -> crate::RhaiRes<String> {
            gen_private_key(algo, DEFAULT_RSA_BITS).map_err(|e| format!("{e}").into())
        })
        .register_fn(
            "gen_private_key",
            |algo: &str, bits: i64| -> crate::RhaiRes<String> {
                gen_private_key(algo, bits as u32).map_err(|e| format!("{e}").into())
            },
        );
}

#[cfg(test)]
mod tests {
    use super::*;
    use openssl::pkey::Id;

    #[test]
    fn ed25519_produces_valid_pkcs8_pem() {
        let pem = gen_private_key("ed25519", 0).unwrap();
        assert!(pem.starts_with("-----BEGIN PRIVATE KEY-----"));
        let key = PKey::private_key_from_pem(pem.as_bytes()).unwrap();
        assert_eq!(key.id(), Id::ED25519);
    }

    #[test]
    fn rsa_produces_valid_pkcs8_pem() {
        let pem = gen_private_key("rsa", 2048).unwrap();
        assert!(pem.starts_with("-----BEGIN PRIVATE KEY-----"));
        let key = PKey::private_key_from_pem(pem.as_bytes()).unwrap();
        assert_eq!(key.id(), Id::RSA);
        assert_eq!(key.bits(), 2048);
    }

    #[test]
    fn algorithm_is_case_insensitive() {
        gen_private_key("ED25519", 0).unwrap();
    }

    #[test]
    fn unknown_algorithm_is_an_error() {
        assert!(gen_private_key("dsa", 0).is_err());
    }

    #[test]
    fn two_generations_differ() {
        let a = gen_private_key("ed25519", 0).unwrap();
        let b = gen_private_key("ed25519", 0).unwrap();
        assert_ne!(a, b);
    }
}
