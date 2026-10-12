use crate::error::{ApiError, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use query_runtime::crypto::Cipher;
use serde::Deserialize;
use std::{collections::HashMap, path::Path};
use zeroize::Zeroizing;

pub trait SecretStore: Send + Sync {
    fn seal(&self, context: &[u8], secret: &[u8]) -> Result<Vec<u8>>;
    fn open(&self, context: &[u8], ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>>;
    fn fingerprint(&self, context: &[u8], value: &[u8]) -> Result<Vec<u8>>;
    fn matches_fingerprint(&self, context: &[u8], value: &[u8], stored: &[u8]) -> Result<bool>;
}
pub struct KeyRing {
    active: String,
    keys: HashMap<String, Cipher>,
}
#[derive(Deserialize)]
struct KeyFile {
    active: String,
    keys: HashMap<String, String>,
}
impl KeyRing {
    pub fn load(path: &Path) -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::symlink_metadata(path).map_err(|_| ApiError::unavailable())?;
            if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 {
                return Err(ApiError::unavailable());
            }
        }
        let bytes = Zeroizing::new(std::fs::read(path).map_err(|_| ApiError::unavailable())?);
        if bytes.len() > 65536 {
            return Err(ApiError::invalid());
        }
        let mut file: KeyFile = serde_json::from_slice(&bytes).map_err(|_| ApiError::invalid())?;
        let mut keys = HashMap::new();
        for (version, mut key) in file.keys.drain() {
            if version.is_empty()
                || version.len() > 64
                || !version
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            {
                return Err(ApiError::invalid());
            }
            let decoded = STANDARD.decode(&key);
            use zeroize::Zeroize;
            key.zeroize();
            let decoded = decoded.map_err(|_| ApiError::invalid())?;
            let decoded = Zeroizing::new(decoded);
            let value: [u8; 32] = decoded
                .as_slice()
                .try_into()
                .map_err(|_| ApiError::invalid())?;
            keys.insert(version, Cipher::from_key(value));
        }
        if !keys.contains_key(&file.active) {
            return Err(ApiError::invalid());
        }
        Ok(Self {
            active: file.active,
            keys,
        })
    }
    pub fn from_key(version: &str, key: [u8; 32]) -> Self {
        Self {
            active: version.into(),
            keys: HashMap::from([(version.into(), Cipher::from_key(key))]),
        }
    }
    pub fn fingerprint(&self, context: &[u8], value: &[u8]) -> Result<Vec<u8>> {
        self.fingerprint_version(&self.active, context, value)
    }
    fn fingerprint_version(&self, version: &str, context: &[u8], value: &[u8]) -> Result<Vec<u8>> {
        use hmac::{Hmac, Mac};
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(
            self.keys
                .get(version)
                .ok_or_else(ApiError::unavailable)?
                .key(),
        )
        .map_err(|_| ApiError::unavailable())?;
        mac.update(b"studio.web.idempotency.v1\0");
        mac.update(context);
        mac.update(&[0]);
        mac.update(value);
        let mut output = vec![1, version.len() as u8];
        output.extend(version.as_bytes());
        output.extend(mac.finalize().into_bytes());
        Ok(output)
    }
    pub fn matches_fingerprint(&self, context: &[u8], value: &[u8], stored: &[u8]) -> Result<bool> {
        use subtle::ConstantTimeEq;
        if stored.len() < 2 || stored[0] != 1 || stored.len() != 2 + stored[1] as usize + 32 {
            return Err(ApiError::unavailable());
        }
        let version = std::str::from_utf8(&stored[2..2 + stored[1] as usize])
            .map_err(|_| ApiError::unavailable())?;
        Ok(bool::from(
            self.fingerprint_version(version, context, value)?
                .as_slice()
                .ct_eq(stored),
        ))
    }
}
impl SecretStore for KeyRing {
    fn fingerprint(&self, context: &[u8], value: &[u8]) -> Result<Vec<u8>> {
        KeyRing::fingerprint(self, context, value)
    }
    fn matches_fingerprint(&self, context: &[u8], value: &[u8], stored: &[u8]) -> Result<bool> {
        KeyRing::matches_fingerprint(self, context, value, stored)
    }
    fn seal(&self, context: &[u8], secret: &[u8]) -> Result<Vec<u8>> {
        let context = [
            b"studio.web.v1\0".as_slice(),
            self.active.as_bytes(),
            b"\0",
            context,
        ]
        .concat();
        let encrypted = self
            .keys
            .get(&self.active)
            .ok_or_else(ApiError::unavailable)?
            .seal(secret, &context)?;
        let mut result = vec![1, self.active.len() as u8];
        result.extend(self.active.as_bytes());
        result.extend(encrypted);
        Ok(result)
    }
    fn open(&self, context: &[u8], ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        if ciphertext.len() < 2
            || ciphertext[0] != 1
            || ciphertext.len() < 2 + ciphertext[1] as usize
        {
            return Err(ApiError::unavailable());
        }
        let n = ciphertext[1] as usize;
        let version =
            std::str::from_utf8(&ciphertext[2..2 + n]).map_err(|_| ApiError::unavailable())?;
        let context = [
            b"studio.web.v1\0".as_slice(),
            version.as_bytes(),
            b"\0",
            context,
        ]
        .concat();
        let key = self.keys.get(version).ok_or_else(|| {
            ApiError::new(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "SECRET_KEY_MISSING",
                "Restore the required key version",
            )
        })?;
        Ok(Zeroizing::new(key.open(&ciphertext[2 + n..], &context)?))
    }
}
