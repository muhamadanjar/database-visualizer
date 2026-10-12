//! Desktop host services kept testable without a WebView. Customer SQL remains in db-core.
mod crypto;
mod host;
mod persistence;
mod spool;
pub use host::*;
pub use persistence::*;
pub use spool::*;

pub(crate) fn error(code: &str, message: &str) -> db_types::DbError {
    db_types::DbError::new(code, "desktop", message)
}
pub use query_runtime::Profile;
