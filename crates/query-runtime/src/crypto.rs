use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use db_types::DbResult;
use rand::{RngCore, rngs::OsRng};
use zeroize::Zeroizing;

pub struct Cipher(Zeroizing<[u8; 32]>);
impl Cipher {
    pub fn random() -> Self {
        let mut key = [0; 32];
        OsRng.fill_bytes(&mut key);
        Self(Zeroizing::new(key))
    }
    pub fn from_key(key: [u8; 32]) -> Self {
        Self(Zeroizing::new(key))
    }
    pub fn key(&self) -> &[u8] {
        self.0.as_ref()
    }
    pub fn seal(&self, data: &[u8], context: &[u8]) -> DbResult<Vec<u8>> {
        let mut nonce = [0; 24];
        OsRng.fill_bytes(&mut nonce);
        let cipher = XChaCha20Poly1305::new_from_slice(self.0.as_ref())
            .map_err(|_| crate::error("ENCRYPTION_FAILED", "Encryption unavailable"))?;
        let encrypted = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: data,
                    aad: context,
                },
            )
            .map_err(|_| crate::error("ENCRYPTION_FAILED", "Encryption failed"))?;
        Ok([nonce.as_slice(), encrypted.as_slice()].concat())
    }
    pub fn open(&self, data: &[u8], context: &[u8]) -> DbResult<Vec<u8>> {
        if data.len() < 40 {
            return Err(crate::error(
                "DECRYPTION_FAILED",
                "Encrypted data is invalid",
            ));
        }
        XChaCha20Poly1305::new_from_slice(self.0.as_ref())
            .map_err(|_| crate::error("DECRYPTION_FAILED", "Encryption key unavailable"))?
            .decrypt(
                XNonce::from_slice(&data[..24]),
                Payload {
                    msg: &data[24..],
                    aad: context,
                },
            )
            .map_err(|_| {
                crate::error(
                    "DECRYPTION_FAILED",
                    "Encrypted data could not be authenticated",
                )
            })
    }
}
