/*use rnd8::rngs::OsRng;
use ed25519_dalek::{SigningKey, VerifyingKey, pkcs8::{EncodePublicKey, EncodePrivateKey, spki::der::pem::LineEnding}};
*/
use openssl::bn::BigNumContext;
use openssl::nid::Nid;
use openssl::ec::{EcGroup, EcKey, PointConversionForm};


use crate::{Result, Error, RhaiRes};

#[derive(Clone)]
pub struct Ed25519 {
    sign_key: EcKey<openssl::pkey::Private>
}

impl Ed25519 {
    #[must_use]
    //pub fn new() -> Self {
    pub fn new() -> Result<Self, Error> {
        let nid = Nid::X9_62_PRIME256V1; // NIST P-256 curve
        let group = EcGroup::from_curve_name(nid).map_err(Error::OpenSSL)?;
        let key = EcKey::generate(&group).map_err(Error::OpenSSL)?;
        Ok(Self {
            sign_key: key,
        })
/*
        let mut rng = OsRng;
        Self {
            sign_key: SigningKey::generate(&mut rng),
        }
*/
    }
    pub fn rhai_new() -> RhaiRes<Self> {
        Self::new().map_err(|e| format!("{e}").into())
    }
    pub fn public_key(&self) -> Result<String> {
        //VerifyingKey::to_public_key_pem(&self.sign_key.verifying_key(), LineEnding::default()).map_err(Error::Ed25519EncodePublicError)
        let pem = self.sign_key.private_key_to_pem().map_err(Error::OpenSSL)?;
        String::from_utf8(pem).map_err(Error::UTF8)
    }
    pub fn private_key(&self) -> Result<String> {
        let pem = self.sign_key.private_key_to_pem().map_err(Error::OpenSSL)?;
        String::from_utf8(pem).map_err(Error::UTF8)
        //let ret = SigningKey::to_pkcs8_pem(&self.sign_key, LineEnding::default()).map_err(Error::Ed25519EncodePrivateError)?;
        //Ok(ret.as_str().to_string())
    }
    pub fn rhai_public_key(&mut self) -> RhaiRes<String> {
        self.public_key().map_err(|e| format!("{e}").into())
    }
    pub fn rhai_private_key(&mut self) -> RhaiRes<String> {
        self.private_key().map_err(|e| format!("{e}").into())
    }

}
