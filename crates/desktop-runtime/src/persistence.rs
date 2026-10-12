use crate::{crypto::Cipher, error};
use db_types::*;
use query_runtime::Profile;
use rusqlite::{Connection, DatabaseName, params};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub trait SecretStore: Send + Sync {
    fn get(&self, reference: &str) -> DbResult<Vec<u8>>;
    fn set(&self, reference: &str, secret: &[u8]) -> DbResult<()>;
    fn delete(&self, reference: &str) -> DbResult<()>;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct RecentFile {
    pub id: Uuid,
    pub path: std::path::PathBuf,
    pub name: String,
}
pub struct OsSecretStore {
    gate: Mutex<()>,
    service: String,
}
impl Default for OsSecretStore {
    fn default() -> Self {
        Self::with_service("dev.database-studio.desktop".into())
    }
}
impl OsSecretStore {
    pub fn with_service(service: String) -> Self {
        Self {
            gate: Mutex::new(()),
            service,
        }
    }
    fn entry(&self, reference: &str) -> DbResult<keyring::Entry> {
        keyring::Entry::new(&self.service, reference).map_err(secret_error)
    }
}
fn secret_error(e: keyring::Error) -> DbError {
    match e {
        keyring::Error::NoEntry => error(
            "SECRET_MISSING",
            "Saved credential was not found; enter it again",
        ),
        _ => error(
            "SECRET_STORE_UNAVAILABLE",
            "Unlock the OS credential store or use a memory-only connection",
        ),
    }
}
impl SecretStore for OsSecretStore {
    fn get(&self, reference: &str) -> DbResult<Vec<u8>> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| error("SECRET_STORE_UNAVAILABLE", "Credential store unavailable"))?;
        self.entry(reference)?.get_secret().map_err(secret_error)
    }
    fn set(&self, reference: &str, secret: &[u8]) -> DbResult<()> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| error("SECRET_STORE_UNAVAILABLE", "Credential store unavailable"))?;
        self.entry(reference)?
            .set_secret(secret)
            .map_err(secret_error)
    }
    fn delete(&self, reference: &str) -> DbResult<()> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| error("SECRET_STORE_UNAVAILABLE", "Credential store unavailable"))?;
        match self.entry(reference)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(secret_error(e)),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub kind: String,
    pub name: String,
    pub content: serde_json::Value,
    pub created_at_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentHeader {
    pub favorite: bool,
    pub id: Uuid,
    pub connection_id: Uuid,
    pub kind: String,
    pub name: String,
    pub created_at_ms: u64,
}
pub struct Persistence {
    db: Mutex<Connection>,
    secrets: Arc<dyn SecretStore>,
    cipher_gate: Mutex<()>,
}
fn storage_error(_: rusqlite::Error) -> DbError {
    error(
        "LOCAL_STORAGE_ERROR",
        "Local storage operation failed; data was not discarded",
    )
}
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_millis() as u64)
        .unwrap_or(0)
}
pub(crate) fn private_directory(path: &Path) -> DbResult<()> {
    std::fs::create_dir_all(path).map_err(|_| {
        error(
            "LOCAL_STORAGE_ERROR",
            "Cannot create private application directory",
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(|_| {
            error(
                "LOCAL_STORAGE_ERROR",
                "Cannot protect application directory",
            )
        })?;
    }
    Ok(())
}
impl Persistence {
    pub fn open(directory: &Path, secrets: Arc<dyn SecretStore>) -> DbResult<Self> {
        private_directory(directory)?;
        let path = directory.join("workspace.sqlite3");
        let connection = Connection::open(&path).map_err(storage_error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .map_err(|_| error("LOCAL_STORAGE_ERROR", "Cannot protect local database"))?;
        }
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(storage_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=DELETE;")
            .map_err(storage_error)?;
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(storage_error)?;
        if version > 2 {
            return Err(error(
                "SCHEMA_TOO_NEW",
                "Use the newer application version; automatic downgrade is disabled",
            ));
        }
        if version < 1 {
            if path.metadata().map(|v| v.len() > 0).unwrap_or(false) {
                let backup = directory.join(format!("workspace.before-v1-{}.sqlite3", now_ms()));
                connection
                    .backup(DatabaseName::Main, &backup, None)
                    .map_err(storage_error)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o600))
                        .map_err(|_| error("LOCAL_STORAGE_ERROR", "Cannot protect backup"))?;
                }
            }
            connection.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE profiles(id TEXT PRIMARY KEY, body TEXT NOT NULL, secret_ref TEXT);
                CREATE TABLE pending_secrets(reference TEXT PRIMARY KEY);
                CREATE TABLE documents(id TEXT PRIMARY KEY, connection_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE, kind TEXT NOT NULL CHECK(kind IN ('history','saved','editor')), name TEXT NOT NULL, content BLOB NOT NULL, created_at_ms INTEGER NOT NULL);
                CREATE INDEX documents_scope ON documents(connection_id,kind,created_at_ms DESC);
                CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                PRAGMA user_version=1; COMMIT;").map_err(storage_error)?;
        }
        if version < 2 {
            let backup = directory.join(format!("workspace.before-v2-{}.sqlite3", now_ms()));
            connection
                .backup(DatabaseName::Main, &backup, None)
                .map_err(storage_error)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o600))
                    .map_err(|_| error("LOCAL_STORAGE_ERROR", "Cannot protect backup"))?;
            }
            connection.execute_batch("BEGIN IMMEDIATE; CREATE TABLE favorites(document_id TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE); CREATE TABLE mutations(id TEXT PRIMARY KEY,connection_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,tab_id TEXT NOT NULL,request_hash BLOB NOT NULL,state TEXT NOT NULL,created_at_ms INTEGER NOT NULL); PRAGMA user_version=2; COMMIT;").map_err(storage_error)?;
        }
        connection
            .execute(
                "UPDATE mutations SET state='Unknown' WHERE state='Applying'",
                [],
            )
            .map_err(storage_error)?;
        let store = Self {
            db: Mutex::new(connection),
            secrets,
            cipher_gate: Mutex::new(()),
        };
        store.cleanup_secrets()?;
        store.prune_history()?;
        Ok(store)
    }
    fn lock(&self) -> DbResult<std::sync::MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|_| error("LOCAL_STORAGE_ERROR", "Local storage unavailable"))
    }
    pub fn profiles(&self) -> DbResult<Vec<Profile>> {
        let db = self.lock()?;
        let mut statement = db
            .prepare("SELECT body FROM profiles ORDER BY json_extract(body,'$.name')")
            .map_err(storage_error)?;
        let values = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(storage_error)?;
        values
            .map(|v| {
                serde_json::from_str(&v.map_err(storage_error)?)
                    .map_err(|_| error("LOCAL_STORAGE_ERROR", "Connection metadata is invalid"))
            })
            .collect()
    }
    pub fn profile(&self, id: Uuid) -> DbResult<Profile> {
        self.profiles()?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| error("PROFILE_NOT_FOUND", "Connection was not found"))
    }
    pub fn password(&self, id: Uuid) -> DbResult<String> {
        let reference: Option<String> = self
            .lock()?
            .query_row(
                "SELECT secret_ref FROM profiles WHERE id=?",
                [id.to_string()],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        let reference = reference
            .ok_or_else(|| error("SECRET_MISSING", "Enter a password for this session"))?;
        String::from_utf8(self.secrets.get(&reference)?)
            .map_err(|_| error("SECRET_MISSING", "Stored credential is invalid"))
    }
    pub fn save_profile(
        &self,
        mut profile: Profile,
        password: Option<&str>,
        remember: bool,
    ) -> DbResult<Profile> {
        profile.config(String::new())?;
        let mut db = self.lock()?;
        let existing: Option<(String, Option<String>)> = db
            .query_row(
                "SELECT body,secret_ref FROM profiles WHERE id=?",
                [profile.id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?;
        let old_reference = existing.as_ref().and_then(|v| v.1.clone());
        profile.revision = existing
            .as_ref()
            .and_then(|v| serde_json::from_str::<Profile>(&v.0).ok())
            .map(|p| p.revision.saturating_add(1))
            .unwrap_or(1);
        let new_reference = if remember {
            match password {
                Some(password) => {
                    let reference = format!("connection/{}", Uuid::new_v4());
                    db.execute(
                        "INSERT INTO pending_secrets(reference) VALUES (?)",
                        [&reference],
                    )
                    .map_err(storage_error)?;
                    self.secrets.set(&reference, password.as_bytes())?;
                    Some(reference)
                }
                None => old_reference.clone(),
            }
        } else if password.is_none() {
            old_reference.clone()
        } else {
            None
        };
        let transaction = db.transaction().map_err(storage_error)?;
        let body = serde_json::to_string(&profile).map_err(|_| {
            error(
                "LOCAL_STORAGE_ERROR",
                "Connection metadata could not be encoded",
            )
        })?;
        transaction.execute("INSERT INTO profiles(id,body,secret_ref) VALUES (?,?,?) ON CONFLICT(id) DO UPDATE SET body=excluded.body,secret_ref=excluded.secret_ref",params![profile.id.to_string(),body,new_reference]).map_err(storage_error)?;
        if let Some(reference) = &new_reference {
            transaction
                .execute("DELETE FROM pending_secrets WHERE reference=?", [reference])
                .map_err(storage_error)?;
        }
        if old_reference != new_reference
            && let Some(reference) = old_reference
        {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO pending_secrets VALUES (?)",
                    [reference],
                )
                .map_err(storage_error)?;
        }
        if !profile.restore_sql {
            transaction
                .execute(
                    "DELETE FROM documents WHERE connection_id=? AND kind='editor'",
                    [profile.id.to_string()],
                )
                .map_err(storage_error)?;
        }
        transaction.commit().map_err(storage_error)?;
        drop(db);
        self.cleanup_secrets()?;
        Ok(profile)
    }
    pub fn delete_profile(&self, id: Uuid) -> DbResult<()> {
        let mut db = self.lock()?;
        let transaction = db.transaction().map_err(storage_error)?;
        let reference: Option<String> = transaction
            .query_row(
                "SELECT secret_ref FROM profiles WHERE id=?",
                [id.to_string()],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        if let Some(reference) = reference {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO pending_secrets VALUES (?)",
                    [reference],
                )
                .map_err(storage_error)?;
        }
        transaction
            .execute("DELETE FROM profiles WHERE id=?", [id.to_string()])
            .map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        drop(db);
        self.cleanup_secrets()
    }
    pub fn cleanup_secrets(&self) -> DbResult<()> {
        let db = self.lock()?;
        let mut statement=db.prepare("SELECT reference FROM pending_secrets WHERE reference NOT IN (SELECT secret_ref FROM profiles WHERE secret_ref IS NOT NULL)").map_err(storage_error)?;
        let refs = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        for reference in refs {
            if self.secrets.delete(&reference).is_ok() {
                db.execute("DELETE FROM pending_secrets WHERE reference=?", [reference])
                    .map_err(storage_error)?;
            }
        }
        Ok(())
    }
    fn cipher(&self) -> DbResult<Cipher> {
        let key = match self.secrets.get("sql-key/v1") {
            Ok(key) => key,
            Err(e) if e.code == "SECRET_MISSING" => {
                // Never replace a missing key if ciphertext exists: recovery must preserve it.
                let count: i64 = self
                    .lock()?
                    .query_row("SELECT COUNT(*) FROM documents", [], |r| r.get(0))
                    .map_err(storage_error)?;
                if count > 0 {
                    return Err(error(
                        "ENCRYPTION_KEY_MISSING",
                        "Restore the OS encryption key to read saved SQL",
                    ));
                }
                let key = Cipher::random();
                self.secrets.set("sql-key/v1", key.key())?;
                key.key().to_vec()
            }
            Err(e) => return Err(e),
        };
        let key: [u8; 32] = key
            .try_into()
            .map_err(|_| error("ENCRYPTION_KEY_INVALID", "OS encryption key is invalid"))?;
        Ok(Cipher::from_key(key))
    }
    pub fn save_document(&self, document: &Document) -> DbResult<()> {
        let _cipher_guard = self
            .cipher_gate
            .lock()
            .map_err(|_| error("LOCAL_STORAGE_ERROR", "Encryption service unavailable"))?;
        let profile = self.profile(document.connection_id)?;
        if (document.kind == "history" && !profile.history_enabled)
            || (document.kind == "editor" && !profile.restore_sql)
        {
            return Err(error(
                "SQL_PERSISTENCE_DISABLED",
                "Enable SQL persistence for this connection first",
            ));
        }
        if !["history", "saved", "editor"].contains(&document.kind.as_str()) {
            return Err(error("INVALID_DOCUMENT", "Invalid SQL document type"));
        }
        if document.name.len() > 512 {
            return Err(error(
                "INVALID_DOCUMENT",
                "Document name is limited to 512 bytes",
            ));
        }
        if document.kind == "editor" {
            let count:u64=self.lock()?.query_row("SELECT COUNT(*) FROM documents WHERE connection_id=? AND kind='editor' AND id<>?",params![document.connection_id.to_string(),document.id.to_string()],|r|r.get(0)).map_err(storage_error)?;
            if count >= 16 {
                return Err(error(
                    "RESTORE_CAPACITY_EXCEEDED",
                    "SQL restore supports 16 editors per connection; save additional queries explicitly",
                ));
            }
        }
        if document.kind == "editor" {
            let count: u64 = self
                .lock()?
                .query_row(
                    "SELECT COUNT(*) FROM documents WHERE kind='editor' AND id<>?",
                    [document.id.to_string()],
                    |r| r.get(0),
                )
                .map_err(storage_error)?;
            if count >= 64 {
                return Err(error(
                    "RESTORE_CAPACITY_EXCEEDED",
                    "SQL restore supports 64 editors per workspace",
                ));
            }
        }
        let data = serde_json::to_vec(&document.content)
            .map_err(|_| error("LOCAL_STORAGE_ERROR", "SQL document could not be encoded"))?;
        if data.len() > 1024 * 1024 {
            return Err(error("DOCUMENT_TOO_LARGE", "SQL document exceeds 1 MiB"));
        }
        let context = format!(
            "v1/{}/{}/{}",
            document.connection_id, document.kind, document.id
        );
        let encrypted = self.cipher()?.seal(&data, context.as_bytes())?;
        let changed=self.lock()?.execute("INSERT INTO documents VALUES (?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,content=excluded.content,created_at_ms=excluded.created_at_ms WHERE documents.connection_id=excluded.connection_id AND documents.kind=excluded.kind",params![document.id.to_string(),document.connection_id.to_string(),document.kind,document.name,encrypted,document.created_at_ms]).map_err(storage_error)?;
        if changed == 0 {
            return Err(error(
                "DOCUMENT_ID_CONFLICT",
                "SQL document identifier belongs to another document type",
            ));
        }
        self.prune_history()
    }
    pub fn document_headers(
        &self,
        connection_id: Uuid,
        kind: &str,
        offset: u64,
        limit: u64,
    ) -> DbResult<Vec<DocumentHeader>> {
        if !["history", "saved", "editor"].contains(&kind)
            || limit == 0
            || limit > 100
            || offset > 100_000
        {
            return Err(error("INVALID_PAGE", "Request 1–100 SQL document headers"));
        }
        self.prune_history()?;
        let db = self.lock()?;
        let mut statement=db.prepare("SELECT id,name,created_at_ms,EXISTS(SELECT 1 FROM favorites f WHERE f.document_id=documents.id) FROM documents WHERE connection_id=? AND kind=? ORDER BY created_at_ms DESC,id LIMIT ? OFFSET ?").map_err(storage_error)?;
        statement
            .query_map(
                params![connection_id.to_string(), kind, limit, offset],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u64>(2)?,
                        row.get::<_, bool>(3)?,
                    ))
                },
            )
            .map_err(storage_error)?
            .map(|record| {
                let (id, name, created_at_ms, favorite) = record.map_err(storage_error)?;
                Ok(DocumentHeader {
                    favorite,
                    id: Uuid::parse_str(&id)
                        .map_err(|_| error("LOCAL_STORAGE_ERROR", "Document identifier invalid"))?,
                    connection_id,
                    kind: kind.into(),
                    name,
                    created_at_ms,
                })
            })
            .collect()
    }
    pub fn document(&self, connection_id: Uuid, kind: &str, id: Uuid) -> DbResult<Document> {
        let _guard = self
            .cipher_gate
            .lock()
            .map_err(|_| error("LOCAL_STORAGE_ERROR", "Encryption service unavailable"))?;
        let (name,bytes,created_at_ms):(String,Vec<u8>,u64)=self.lock()?.query_row("SELECT name,content,created_at_ms FROM documents WHERE connection_id=? AND kind=? AND id=?",params![connection_id.to_string(),kind,id.to_string()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).map_err(storage_error)?;
        let context = format!("v1/{connection_id}/{kind}/{id}");
        let content = serde_json::from_slice(&self.cipher()?.open(&bytes, context.as_bytes())?)
            .map_err(|_| error("LOCAL_STORAGE_ERROR", "SQL document invalid"))?;
        Ok(Document {
            id,
            connection_id,
            kind: kind.into(),
            name,
            content,
            created_at_ms,
        })
    }
    pub fn rename_document(
        &self,
        connection_id: Uuid,
        kind: &str,
        id: Uuid,
        name: &str,
    ) -> DbResult<()> {
        if name.trim().is_empty() || name.len() > 512 {
            return Err(error("INVALID_DOCUMENT", "Provide a name of 1–512 bytes"));
        }
        let changed = self
            .lock()?
            .execute(
                "UPDATE documents SET name=? WHERE connection_id=? AND kind=? AND id=?",
                params![name, connection_id.to_string(), kind, id.to_string()],
            )
            .map_err(storage_error)?;
        if changed == 0 {
            return Err(error("DOCUMENT_NOT_FOUND", "SQL document was not found"));
        }
        Ok(())
    }
    pub fn documents(&self, connection_id: Uuid, kind: &str) -> DbResult<Vec<Document>> {
        let _cipher_guard = self
            .cipher_gate
            .lock()
            .map_err(|_| error("LOCAL_STORAGE_ERROR", "Encryption service unavailable"))?;
        self.prune_history()?;
        let cipher = self.cipher()?;
        let db = self.lock()?;
        let mut statement=db.prepare("SELECT id,name,content,created_at_ms FROM documents WHERE connection_id=? AND kind=? ORDER BY created_at_ms DESC").map_err(storage_error)?;
        let records = statement
            .query_map(params![connection_id.to_string(), kind], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                    r.get::<_, u64>(3)?,
                ))
            })
            .map_err(storage_error)?;
        records
            .map(|r| {
                let (id, name, data, created_at_ms) = r.map_err(storage_error)?;
                let id = Uuid::parse_str(&id)
                    .map_err(|_| error("LOCAL_STORAGE_ERROR", "Document identifier invalid"))?;
                let context = format!("v1/{connection_id}/{kind}/{id}");
                let content = serde_json::from_slice(&cipher.open(&data, context.as_bytes())?)
                    .map_err(|_| error("LOCAL_STORAGE_ERROR", "SQL document invalid"))?;
                Ok(Document {
                    id,
                    connection_id,
                    kind: kind.into(),
                    name,
                    content,
                    created_at_ms,
                })
            })
            .collect()
    }
    pub fn admit_mutation(
        &self,
        id: Uuid,
        connection: Uuid,
        tab: Uuid,
        request: &[u8],
    ) -> DbResult<Option<String>> {
        use rusqlite::OptionalExtension;
        let cipher = self.cipher()?;
        let db = self.lock()?;
        let existing: Option<(String, String, Vec<u8>, String)> = db
            .query_row(
                "SELECT connection_id,tab_id,request_hash,state FROM mutations WHERE id=?",
                [id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(storage_error)?;
        if let Some((conn, owner, encrypted, state)) = existing {
            if conn != connection.to_string()
                || owner != tab.to_string()
                || cipher.open(&encrypted, id.as_bytes())? != {
                    use hmac::{Hmac, Mac};
                    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(cipher.key())
                        .map_err(|_| error("ENCRYPTION_FAILED", "Fingerprint unavailable"))?;
                    mac.update(request);
                    mac.finalize().into_bytes().to_vec()
                }
            {
                return Err(error(
                    "IDEMPOTENCY_CONFLICT",
                    "Mutation key was already used",
                ));
            }
            return Ok(Some(state));
        }
        // Only a keyed fingerprint is persisted; no SQL or row payload.
        use hmac::{Hmac, Mac};
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(cipher.key())
            .map_err(|_| error("ENCRYPTION_FAILED", "Fingerprint unavailable"))?;
        mac.update(request);
        let hash = mac.finalize().into_bytes();
        let encrypted = cipher.seal(&hash, id.as_bytes())?;
        db.execute(
            "INSERT INTO mutations VALUES(?,?,?,?,'Applying',?)",
            params![
                id.to_string(),
                connection.to_string(),
                tab.to_string(),
                encrypted,
                now_ms()
            ],
        )
        .map_err(storage_error)?;
        Ok(None)
    }
    pub fn finish_mutation(&self, id: Uuid, state: &str) -> DbResult<()> {
        self.lock()?.execute("DELETE FROM mutations WHERE id IN (SELECT id FROM mutations WHERE state!='Applying' AND created_at_ms<? LIMIT 1000)", [now_ms().saturating_sub(2_592_000_000)]).map_err(storage_error)?;
        self.lock()?
            .execute(
                "UPDATE mutations SET state=? WHERE id=?",
                params![state, id.to_string()],
            )
            .map_err(storage_error)?;
        Ok(())
    }
    pub fn mutation_status(
        &self,
        id: Uuid,
        connection: Uuid,
        tab: Uuid,
    ) -> DbResult<serde_json::Value> {
        let state: String = self
            .lock()?
            .query_row(
                "SELECT CASE WHEN state='Applying' AND created_at_ms<? THEN 'Unknown' ELSE state END FROM mutations WHERE id=? AND connection_id=? AND tab_id=?",
                params![now_ms().saturating_sub(120_000), id.to_string(), connection.to_string(), tab.to_string()],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        Ok(serde_json::json!({"id":id,"state":state}))
    }
    pub fn favorite(&self, connection: Uuid, id: Uuid, favorite: bool) -> DbResult<()> {
        let db = self.lock()?;
        let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM documents WHERE connection_id=? AND id=? AND kind='saved')",params![connection.to_string(),id.to_string()],|r|r.get(0)).map_err(|_|error("LOCAL_STORAGE_ERROR","Search storage unavailable"))?;
        if !exists {
            return Err(error("DOCUMENT_NOT_FOUND", "Saved query not found"));
        }
        if favorite {
            db.execute(
                "INSERT OR IGNORE INTO favorites VALUES(?)",
                [id.to_string()],
            )
            .map_err(|_| error("LOCAL_STORAGE_ERROR", "Search storage unavailable"))?;
        } else {
            db.execute(
                "DELETE FROM favorites WHERE document_id=?",
                [id.to_string()],
            )
            .map_err(|_| error("LOCAL_STORAGE_ERROR", "Search storage unavailable"))?;
        }
        Ok(())
    }
    pub fn search_documents(
        &self,
        connection: Uuid,
        request: query_runtime::search::SearchRequest,
    ) -> DbResult<serde_json::Value> {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
        use query_runtime::search::{SearchBudget, SearchCursor};
        request.validate()?;
        self.prune_history()?;
        let _guard = self
            .cipher_gate
            .lock()
            .map_err(|_| error("LOCAL_STORAGE_ERROR", "Search storage unavailable"))?;
        let cipher = self.cipher()?;
        let context = format!("search/{connection}");
        let mut cursor = match &request.cursor {
            Some(value) => serde_json::from_slice::<SearchCursor>(
                &cipher.open(
                    &B64.decode(value)
                        .map_err(|_| error("LOCAL_STORAGE_ERROR", "Search storage unavailable"))?,
                    context.as_bytes(),
                )?,
            )
            .map_err(|_| error("LOCAL_STORAGE_ERROR", "Search storage unavailable"))?,
            None => SearchCursor::initial(&request),
        };
        cursor.validate(&request)?;
        let db = self.lock()?;
        let mut budget = SearchBudget::default();
        let mut hits = Vec::new();
        let mut complete = false;
        let mut statement=db.prepare("SELECT d.id,d.name,d.content,d.created_at_ms,EXISTS(SELECT 1 FROM favorites f WHERE f.document_id=d.id) FROM documents d WHERE d.connection_id=? AND d.kind=? AND d.created_at_ms<=? AND (d.created_at_ms,d.id)<(?,?) ORDER BY d.created_at_ms DESC,d.id DESC LIMIT 1001").map_err(|_|error("LOCAL_STORAGE_ERROR","Search storage unavailable"))?;
        let mut rows = statement
            .query(params![
                connection.to_string(),
                request.kind,
                cursor.watermark,
                cursor.before_ms,
                cursor.before_id.to_string()
            ])
            .map_err(|_| error("LOCAL_STORAGE_ERROR", "Search storage unavailable"))?;
        loop {
            let Some(row) = rows
                .next()
                .map_err(|_| error("LOCAL_STORAGE_ERROR", "Search storage unavailable"))?
            else {
                complete = true;
                break;
            };
            let bytes: Vec<u8> = row
                .get(2)
                .map_err(|_| error("LOCAL_STORAGE_ERROR", "Search storage unavailable"))?;
            if bytes.len() > 10 * 1024 * 1024 {
                return Err(error(
                    "SEARCH_DOCUMENT_TOO_LARGE",
                    "Document exceeds search scan budget",
                ));
            }
            if !budget.admit(bytes.len()) {
                break;
            }
            let id: String = row
                .get(0)
                .map_err(|_| error("LOCAL_STORAGE_ERROR", "Search storage unavailable"))?;
            let id = Uuid::parse_str(&id)
                .map_err(|_| error("LOCAL_STORAGE_ERROR", "Search storage unavailable"))?;
            let name: String = row
                .get(1)
                .map_err(|_| error("LOCAL_STORAGE_ERROR", "Search storage unavailable"))?;
            let created: u64 = row
                .get(3)
                .map_err(|_| error("LOCAL_STORAGE_ERROR", "Search storage unavailable"))?;
            let favorite: bool = row
                .get(4)
                .map_err(|_| error("LOCAL_STORAGE_ERROR", "Search storage unavailable"))?;
            let content: serde_json::Value = serde_json::from_slice(&cipher.open(
                &bytes,
                format!("v1/{connection}/{}/{id}", request.kind).as_bytes(),
            )?)
            .map_err(|_| error("LOCAL_STORAGE_ERROR", "Search storage unavailable"))?;
            cursor.before_ms = created;
            cursor.before_id = id;
            if request.matches(&name, &content, created, favorite) {
                hits.push(serde_json::json!({"id":id,"connection_id":connection,"kind":request.kind,"name":name,"created_at_ms":created,"favorite":favorite,"can_edit":true}));
            }
            if hits.len() == 100 {
                break;
            }
        }

        let next =
            if complete {
                None
            } else {
                Some(B64.encode(
                    cipher.seal(
                        &serde_json::to_vec(&cursor).map_err(|_| {
                            error("LOCAL_STORAGE_ERROR", "Search storage unavailable")
                        })?,
                        context.as_bytes(),
                    )?,
                ))
            };
        Ok(
            serde_json::json!({"items":hits,"complete":complete,"next_cursor":next,"scanned":budget.scanned}),
        )
    }
    pub fn delete_documents(
        &self,
        connection_id: Uuid,
        kind: &str,
        id: Option<Uuid>,
    ) -> DbResult<()> {
        self.lock()?
            .execute(
                "DELETE FROM documents WHERE connection_id=? AND kind=? AND (? IS NULL OR id=?)",
                params![
                    connection_id.to_string(),
                    kind,
                    id.map(|v| v.to_string()),
                    id.map(|v| v.to_string())
                ],
            )
            .map_err(storage_error)?;
        Ok(())
    }
    fn prune_history(&self) -> DbResult<()> {
        let db = self.lock()?;
        let cutoff = now_ms().saturating_sub(30 * 24 * 60 * 60 * 1000);
        db.execute(
            "DELETE FROM documents WHERE kind='history' AND created_at_ms<?",
            [cutoff],
        )
        .map_err(storage_error)?;
        db.execute("DELETE FROM documents WHERE id IN (SELECT id FROM (SELECT id, ROW_NUMBER() OVER (PARTITION BY connection_id ORDER BY created_at_ms DESC) AS rank FROM documents WHERE kind='history') WHERE rank>1000)",[]).map_err(storage_error)?;
        Ok(())
    }
    pub fn layout(&self) -> DbResult<serde_json::Value> {
        let value: Option<String> = self
            .lock()?
            .query_row("SELECT value FROM settings WHERE key='layout'", [], |r| {
                r.get(0)
            })
            .optional()
            .map_err(storage_error)?;
        value
            .map(|v| {
                serde_json::from_str(&v).map_err(|_| error("LOCAL_STORAGE_ERROR", "Layout invalid"))
            })
            .unwrap_or_else(|| Ok(serde_json::json!({})))
    }
    pub fn save_layout(&self, value: &serde_json::Value) -> DbResult<()> {
        // Persist only validated layout primitives, never arbitrary frontend state/SQL.
        let mut safe = serde_json::Map::new();
        for key in ["sidebar_width", "editor_height"] {
            if let Some(n) = value.get(key).and_then(|v| v.as_u64()) {
                safe.insert(key.into(), serde_json::json!(n.clamp(150, 1200)));
            }
        }
        let body = serde_json::Value::Object(safe).to_string();
        self.lock()?.execute("INSERT INTO settings VALUES ('layout',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[body]).map_err(storage_error)?;
        Ok(())
    }
    pub fn recent_files(&self) -> DbResult<Vec<RecentFile>> {
        let body: Option<String> = self
            .lock()?
            .query_row(
                "SELECT value FROM settings WHERE key='recent_files'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        body.map(|body| {
            serde_json::from_str(&body)
                .map_err(|_| error("LOCAL_STORAGE_ERROR", "Recent files metadata invalid"))
        })
        .unwrap_or_else(|| Ok(Vec::new()))
    }
    pub fn remember_file(&self, path: std::path::PathBuf) -> DbResult<()> {
        let name = path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| error("INVALID_FILE", "Choose a file with a valid name"))?
            .to_owned();
        let mut files = self.recent_files()?;
        files.retain(|f| f.path != path);
        files.insert(
            0,
            RecentFile {
                id: Uuid::new_v4(),
                path,
                name,
            },
        );
        files.truncate(20);
        let body = serde_json::to_string(&files)
            .map_err(|_| error("LOCAL_STORAGE_ERROR", "Recent files metadata invalid"))?;
        self.lock()?.execute("INSERT INTO settings VALUES ('recent_files',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[body]).map_err(storage_error)?;
        Ok(())
    }
}
use rusqlite::OptionalExtension;
