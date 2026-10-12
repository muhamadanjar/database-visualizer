//! Platform-neutral encrypted result storage and script lifecycle.
pub mod crypto;
mod dto;
pub mod edit;
mod profile;
mod script;
pub mod search;
mod spool;
mod table;
use db_types::{DbError, DbResult};
pub use dto::*;
pub use profile::*;
pub use script::*;
pub use spool::*;
use std::path::Path;
pub use table::*;
pub fn error(code: &str, message: &str) -> DbError {
    DbError::new(code, "runtime", message)
}
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
pub fn private_directory(path: &Path) -> DbResult<()> {
    std::fs::create_dir_all(path)
        .map_err(|_| error("STORAGE_UNAVAILABLE", "Private storage unavailable"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| error("STORAGE_UNAVAILABLE", "Private storage unavailable"))?;
    }
    Ok(())
}
