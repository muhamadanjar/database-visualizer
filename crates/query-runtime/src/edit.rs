//! Scoped encrypted snapshots and atomic, typed table mutations.
use crate::{Profile, WireRow, crypto::Cipher, error, now_ms, wire_row};
use db_core::DatabaseCore;
use db_types::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Mutex,
    time::Duration,
};
use uuid::Uuid;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditOwner {
    pub user: String,
    pub tab: String,
    pub scope: String,
}
#[derive(Serialize, Deserialize)]
struct SnapshotData {
    object: ObjectId,
    metadata: MetadataResponse,
    rows: Vec<ResultRow>,
    revision: u32,
    #[serde(default)]
    readback_ready: bool,
}
struct Snapshot {
    owner: EditOwner,
    connection: Uuid,
    expires: u64,
    bytes: Vec<u8>,
    claimed: bool,
}
pub struct EditRuntime {
    cipher: Cipher,
    snapshots: Mutex<HashMap<Uuid, Snapshot>>,
}
#[derive(Serialize)]
pub struct EditSnapshot {
    pub token: Uuid,
    pub offset: u32,
    pub columns: Vec<ColumnMetadata>,
    pub rows: Vec<WireRow>,
    pub primary_key: Vec<String>,
    pub generated: Vec<String>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EditInput {
    Default,
    Value { value: Option<String> },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EditChange {
    Insert {
        values: BTreeMap<String, EditInput>,
    },
    Update {
        row: usize,
        values: BTreeMap<String, EditInput>,
    },
    Delete {
        row: usize,
    },
}
#[derive(Clone, Serialize, Deserialize)]
pub struct EditBatch {
    pub tab_id: Uuid,
    pub token: Uuid,
    pub changes: Vec<EditChange>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum EditOutcome {
    Applied,
    Conflict,
    Failed,
    Unknown,
}
impl Default for EditRuntime {
    fn default() -> Self {
        Self {
            cipher: Cipher::random(),
            snapshots: Mutex::new(HashMap::new()),
        }
    }
}
fn invalid() -> DbError {
    error(
        "INVALID_EDIT",
        "Edit request is invalid or column is read-only",
    )
}
fn guard<T>(mutex: &Mutex<T>) -> DbResult<std::sync::MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|_| error("EDIT_UNAVAILABLE", "Editing unavailable"))
}
fn quote(engine: Engine, name: &str) -> String {
    if engine == Engine::Postgres {
        format!("\"{}\"", name.replace('"', "\"\""))
    } else {
        format!("`{}`", name.replace('`', "``"))
    }
}
fn table(object: &ObjectId) -> DbResult<String> {
    let namespace = if object.engine == Engine::Postgres {
        object.schema.as_deref().ok_or_else(invalid)?
    } else {
        &object.database
    };
    Ok(format!(
        "{}.{}",
        quote(object.engine, namespace),
        quote(object.engine, &object.name)
    ))
}
fn family(ty: &str) -> Option<&'static str> {
    let ty = ty.to_lowercase();
    let base = ty.split(['(', ' ']).next()?;
    Some(match base {
        "smallint" | "integer" | "bigint" | "int" | "tinyint" | "mediumint" => "integer",
        "numeric" | "decimal" => "decimal",
        "real" | "double" | "float" => "float",
        "boolean" | "bool" => "boolean",
        "text" | "character" | "varchar" | "char" | "tinytext" | "mediumtext" | "longtext" => {
            "text"
        }
        "bytea" | "binary" | "varbinary" | "blob" | "tinyblob" | "mediumblob" | "longblob" => {
            "binary"
        }
        "json" | "jsonb" => "json",
        "date" => "date",
        "time" if !ty.contains("with time zone") => "time",
        "timestamp" | "datetime" if !ty.contains("with time zone") => "timestamp",
        _ => return None,
    })
}
pub fn writable_type(ty: &str) -> bool {
    family(ty).is_some()
}
fn parsed(column: &ColumnMetadata, input: &EditInput) -> DbResult<Option<DbValue>> {
    let EditInput::Value { value } = input else {
        return Ok(None);
    };
    let Some(value) = value else {
        return if column.nullable {
            Ok(Some(DbValue::Null))
        } else {
            Err(invalid())
        };
    };
    let parse_error = |_| invalid();
    let result = match family(&column.data_type).ok_or_else(invalid)? {
        "integer" if column.data_type.to_lowercase().contains("unsigned") => {
            DbValue::Unsigned(value.parse().map_err(parse_error)?)
        }
        "integer" => DbValue::Signed(value.parse().map_err(parse_error)?),
        "decimal" => {
            value
                .parse::<bigdecimal::BigDecimal>()
                .map_err(|_| invalid())?;
            DbValue::Decimal(value.clone())
        }
        "float" => {
            if !value.parse::<f64>().map_err(|_| invalid())?.is_finite() {
                return Err(invalid());
            }
            DbValue::Float(value.clone())
        }
        "boolean" => DbValue::Boolean(match value.as_str() {
            "true" => true,
            "false" => false,
            _ => return Err(invalid()),
        }),
        "text" => DbValue::Text(value.clone()),
        "binary" => {
            use base64::Engine;
            DbValue::Binary(
                base64::engine::general_purpose::STANDARD
                    .decode(value)
                    .map_err(|_| invalid())?,
            )
        }
        "json" => DbValue::Json(serde_json::from_str(value).map_err(|_| invalid())?),
        "date" => DbValue::Date(value.clone()),
        "time" => DbValue::Time(value.clone()),
        "timestamp" => DbValue::Timestamp(value.clone()),
        _ => return Err(invalid()),
    };
    Ok(Some(result))
}
fn placeholder(engine: Engine, column: &ColumnMetadata, index: usize) -> DbResult<String> {
    if engine == Engine::MySql {
        return Ok("?".into());
    }
    let ty = match family(&column.data_type).ok_or_else(invalid)? {
        "integer" => "bigint",
        "decimal" => "numeric",
        "float" => "double precision",
        "boolean" => "boolean",
        "text" => "text",
        "binary" => "bytea",
        "json" => {
            if column.data_type == "jsonb" {
                "jsonb"
            } else {
                "json"
            }
        }
        "date" => "date",
        "time" => "time",
        "timestamp" => "timestamp",
        _ => return Err(invalid()),
    };
    Ok(format!("CAST(${index} AS {ty})"))
}
fn predicate(data: &SnapshotData, row: usize, parameters: &mut Vec<DbValue>) -> DbResult<String> {
    let original = data.rows.get(row).ok_or_else(invalid)?;
    let metadata = data.metadata.editing.as_ref().ok_or_else(invalid)?;
    metadata
        .primary_key
        .iter()
        .map(|key| {
            let index = data
                .metadata
                .columns
                .iter()
                .position(|c| &c.name == key)
                .ok_or_else(invalid)?;
            let value = original.get(index).ok_or_else(invalid)?;
            if matches!(value, DbValue::Null | DbValue::Opaque { .. }) {
                return Err(invalid());
            }
            parameters.push(value.clone());
            Ok(format!(
                "{} = {}",
                quote(data.object.engine, key),
                placeholder(
                    data.object.engine,
                    &data.metadata.columns[index],
                    parameters.len()
                )?
            ))
        })
        .collect::<DbResult<Vec<_>>>()
        .map(|parts| parts.join(" AND "))
}
fn same_row(left: &ResultRow, right: &ResultRow) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(a, b)| match (a, b) {
            (DbValue::Decimal(a), DbValue::Decimal(b)) => a
                .parse::<bigdecimal::BigDecimal>()
                .ok()
                .zip(b.parse::<bigdecimal::BigDecimal>().ok())
                .is_some_and(|(a, b)| a == b),
            _ => a == b,
        })
}
pub async fn collect(
    core: &DatabaseCore,
    session: Uuid,
    sql: String,
    params: Vec<DbValue>,
) -> DbResult<(Vec<ResultRow>, ExecutionSummary)> {
    let mut stream = core
        .execute_typed(
            QueryRequest {
                session_id: session,
                sql,
                timeout: Some(Duration::from_secs(25)),
            },
            params,
        )
        .await?;
    let mut rows = Vec::new();
    let mut bytes = 0usize;
    while let Some(event) = stream.recv().await {
        match event.data {
            QueryEventData::Rows { rows: batch, .. } => {
                for row in batch {
                    bytes =
                        bytes.saturating_add(row.iter().map(DbValue::payload_bytes).sum::<usize>());
                    if rows.len() >= 100 || bytes > 8 * 1024 * 1024 {
                        return Err(error(
                            "EDIT_CAPACITY_EXCEEDED",
                            "Choose a smaller edit page",
                        ));
                    }
                    rows.push(row)
                }
            }
            QueryEventData::QueryCompleted { summary, .. } if summary.results_complete => {
                return Ok((rows, summary));
            }
            QueryEventData::QueryFailed { error, .. } => return Err(error),
            QueryEventData::QueryCancelled { .. } => {
                return Err(error("EDIT_CANCELLED", "Edit operation was cancelled"));
            }
            _ => {}
        }
    }
    Err(error(
        "QUERY_OUTCOME_UNKNOWN",
        "Mutation outcome could not be confirmed",
    ))
}
impl EditRuntime {
    fn data(&self, owner: &EditOwner, connection: Uuid, token: Uuid) -> DbResult<SnapshotData> {
        let mut snapshots = guard(&self.snapshots)?;
        snapshots.retain(|_, s| s.expires > now_ms());
        let snapshot = snapshots
            .get(&token)
            .ok_or_else(|| error("EDIT_SNAPSHOT_EXPIRED", "Refresh the edit page"))?;
        if &snapshot.owner != owner || snapshot.connection != connection {
            return Err(invalid());
        }
        serde_json::from_slice(&self.cipher.open(&snapshot.bytes, token.as_bytes())?)
            .map_err(|_| invalid())
    }
    fn store_receipt(
        &self,
        owner: &EditOwner,
        token: Uuid,
        mut data: SnapshotData,
        rows: Vec<ResultRow>,
    ) -> DbResult<()> {
        data.rows = rows;
        data.readback_ready = true;
        let bytes = self.cipher.seal(
            &serde_json::to_vec(&data).map_err(|_| invalid())?,
            token.as_bytes(),
        )?;
        let mut snapshots = guard(&self.snapshots)?;
        snapshots.retain(|_, s| s.expires > now_ms());
        let original = snapshots.get(&token).ok_or_else(invalid)?.bytes.len();
        let within = |limit: usize, filter: &dyn Fn(&Snapshot) -> bool| {
            snapshots
                .values()
                .filter(|s| filter(s))
                .map(|s| s.bytes.len())
                .sum::<usize>()
                .saturating_sub(original)
                .saturating_add(bytes.len())
                <= limit
        };
        if !within(8 * 1024 * 1024, &|s| s.owner == *owner)
            || !within(32 * 1024 * 1024, &|s| s.owner.user == owner.user)
            || !within(128 * 1024 * 1024, &|_| true)
        {
            return Err(error(
                "EDIT_CAPACITY_EXCEEDED",
                "Readback exceeds snapshot quota",
            ));
        }
        snapshots.get_mut(&token).ok_or_else(invalid)?.bytes = bytes;
        Ok(())
    }
    pub fn readback(
        &self,
        owner: &EditOwner,
        connection: Uuid,
        token: Uuid,
    ) -> DbResult<Vec<WireRow>> {
        let data = self.data(owner, connection, token)?;
        if !data.readback_ready {
            return Err(error(
                "EDIT_READBACK_UNAVAILABLE",
                "Readback requires confirmed Apply",
            ));
        }
        Ok(data.rows.iter().map(wire_row).collect())
    }
    pub fn object(&self, owner: &EditOwner, connection: Uuid, token: Uuid) -> DbResult<ObjectId> {
        Ok(self.data(owner, connection, token)?.object)
    }
    pub fn prune(&self) {
        if let Ok(mut snapshots) = self.snapshots.lock() {
            snapshots.retain(|_, s| s.expires > now_ms());
        }
    }
    pub fn discard_tab_prefix(&self, prefix: &str) {
        if let Ok(mut snapshots) = self.snapshots.lock() {
            snapshots.retain(|_, s| !s.owner.tab.starts_with(prefix));
        }
    }
    pub fn discard_owner(&self, owner: &EditOwner) {
        if let Ok(mut snapshots) = self.snapshots.lock() {
            snapshots.retain(|_, s| &s.owner != owner);
        }
    }
    pub fn discard_user(&self, user: &str) {
        if let Ok(mut snapshots) = self.snapshots.lock() {
            snapshots.retain(|_, s| s.owner.user != user);
        }
    }
    pub fn discard_scope(&self, scope: &str, user: Option<&str>, connection: Option<Uuid>) {
        if let Ok(mut snapshots) = self.snapshots.lock() {
            snapshots.retain(|_, s| {
                !(s.owner.scope == scope
                    && user.is_none_or(|u| s.owner.user == u)
                    && connection.is_none_or(|c| s.connection == c))
            });
        }
    }
    pub fn clear(&self) {
        if let Ok(mut snapshots) = self.snapshots.lock() {
            snapshots.clear();
        }
    }
    pub async fn snapshot(
        &self,
        core: &DatabaseCore,
        session: Uuid,
        profile: &Profile,
        owner: EditOwner,
        object: ObjectId,
        metadata: MetadataResponse,
    ) -> DbResult<EditSnapshot> {
        self.snapshot_page(core, session, profile, owner, object, metadata, 0)
            .await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn snapshot_page(
        &self,
        core: &DatabaseCore,
        session: Uuid,
        profile: &Profile,
        owner: EditOwner,
        object: ObjectId,
        metadata: MetadataResponse,
        offset: u32,
    ) -> DbResult<EditSnapshot> {
        if offset > 100_000 {
            return Err(invalid());
        }
        let edit = metadata.editing.as_ref().ok_or_else(invalid)?;
        if object.kind != ObjectKind::Table
            || object.engine != profile.engine
            || object.database != profile.database
            || !edit.transactional
            || edit.primary_key.is_empty()
            || edit.primary_key.iter().any(|k| {
                metadata
                    .columns
                    .iter()
                    .find(|c| &c.name == k)
                    .is_none_or(|c| !writable_type(&c.data_type))
            })
        {
            return Err(error(
                "TABLE_READ_ONLY",
                "This table has no supported transactional primary key",
            ));
        }
        let order = edit
            .primary_key
            .iter()
            .map(|k| quote(object.engine, k))
            .collect::<Vec<_>>()
            .join(",");
        let page = if object.engine == Engine::Postgres {
            "LIMIT $1 OFFSET $2"
        } else {
            "LIMIT ? OFFSET ?"
        };
        let (rows, _) = collect(
            core,
            session,
            format!(
                "SELECT {} FROM {} ORDER BY {order} {page}",
                metadata
                    .columns
                    .iter()
                    .map(|c| quote(object.engine, &c.name))
                    .collect::<Vec<_>>()
                    .join(","),
                table(&object)?
            ),
            vec![DbValue::Signed(100), DbValue::Signed(i64::from(offset))],
        )
        .await?;
        if rows.iter().any(|r| r.len() != metadata.columns.len()) {
            return Err(invalid());
        }
        let result = EditSnapshot {
            token: Uuid::new_v4(),
            offset,
            columns: metadata.columns.clone(),
            rows: rows.iter().map(wire_row).collect(),
            primary_key: edit.primary_key.clone(),
            generated: edit.generated.clone(),
            warnings: edit.warnings.clone(),
        };
        let data = SnapshotData {
            object,
            metadata,
            rows,
            revision: profile.revision,
            readback_ready: false,
        };
        let bytes = self.cipher.seal(
            &serde_json::to_vec(&data).map_err(|_| invalid())?,
            result.token.as_bytes(),
        )?;
        let mut snapshots = guard(&self.snapshots)?;
        snapshots.retain(|_, s| s.expires > now_ms());
        let sum = |filter: &dyn Fn(&Snapshot) -> bool| {
            snapshots
                .values()
                .filter(|s| filter(s))
                .map(|s| s.bytes.len())
                .sum::<usize>()
        };
        if bytes.len() + sum(&|s| s.owner == owner) > 8 * 1024 * 1024
            || bytes.len() + sum(&|s| s.owner.user == owner.user) > 32 * 1024 * 1024
            || bytes.len() + sum(&|_| true) > 128 * 1024 * 1024
        {
            return Err(error(
                "EDIT_CAPACITY_EXCEEDED",
                "Discard an edit draft before opening another",
            ));
        }
        snapshots.insert(
            result.token,
            Snapshot {
                owner,
                connection: profile.id,
                expires: now_ms() + 600_000,
                bytes,
                claimed: false,
            },
        );
        Ok(result)
    }
    pub fn preview(
        &self,
        owner: &EditOwner,
        connection: Uuid,
        batch: &EditBatch,
    ) -> DbResult<serde_json::Value> {
        if batch.changes.is_empty()
            || batch.changes.len() > 100
            || serde_json::to_vec(batch).map_err(|_| invalid())?.len() > 1024 * 1024
        {
            return Err(invalid());
        }
        let data = self.data(owner, connection, batch.token)?;
        let mut seen = std::collections::HashSet::new();
        let mut statements = Vec::new();
        for change in &batch.changes {
            let mut parameters = Vec::new();
            let sql = self.statement(&data, change, &mut parameters)?;
            let row = match change {
                EditChange::Update { row, .. } | EditChange::Delete { row } => Some(row),
                _ => None,
            };
            if row.is_some_and(|row| !seen.insert(row)) {
                return Err(invalid());
            }
            statements.push(sql);
        }
        Ok(
            serde_json::json!({"statements":statements,"warnings":data.metadata.editing.as_ref().map(|m|&m.warnings),"changes":batch.changes.len()}),
        )
    }
    fn statement(
        &self,
        data: &SnapshotData,
        change: &EditChange,
        parameters: &mut Vec<DbValue>,
    ) -> DbResult<String> {
        let edit = data.metadata.editing.as_ref().ok_or_else(invalid)?;
        let engine = data.object.engine;
        let target = table(&data.object)?;
        let values = match change {
            EditChange::Insert { values } | EditChange::Update { values, .. } => Some(values),
            _ => None,
        };
        let mut names = Vec::new();
        let mut expressions = Vec::new();
        if let Some(values) = values {
            for (name, input) in values {
                let column = data
                    .metadata
                    .columns
                    .iter()
                    .find(|c| &c.name == name)
                    .ok_or_else(invalid)?;
                if edit.generated.contains(name)
                    || matches!(change, EditChange::Update { .. })
                        && edit.primary_key.contains(name)
                    || !writable_type(&column.data_type)
                {
                    return Err(invalid());
                }
                names.push(quote(engine, name));
                expressions.push(match parsed(column, input)? {
                    None => "DEFAULT".into(),
                    Some(value) => {
                        parameters.push(value);
                        placeholder(engine, column, parameters.len())?
                    }
                });
            }
        }
        match change {
            EditChange::Insert { values } => {
                if engine == Engine::MySql
                    && edit.primary_key.iter().any(|key| {
                        !edit.generated.contains(key)
                            && !matches!(values.get(key), Some(EditInput::Value { value: Some(_) }))
                    })
                {
                    return Err(invalid());
                }
                let base = if names.is_empty() {
                    if engine == Engine::Postgres {
                        format!("INSERT INTO {target} DEFAULT VALUES")
                    } else {
                        format!("INSERT INTO {target} () VALUES ()")
                    }
                } else {
                    format!(
                        "INSERT INTO {target} ({}) VALUES ({})",
                        names.join(","),
                        expressions.join(",")
                    )
                };
                Ok(if engine == Engine::Postgres {
                    base + " RETURNING "
                        + &data
                            .metadata
                            .columns
                            .iter()
                            .map(|c| quote(engine, &c.name))
                            .collect::<Vec<_>>()
                            .join(",")
                } else {
                    base
                })
            }
            EditChange::Update { row, .. } => {
                if names.is_empty() {
                    return Err(invalid());
                }
                let assignments = names
                    .iter()
                    .zip(expressions)
                    .map(|(n, v)| format!("{n}={v}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let where_sql = predicate(data, *row, parameters)?;
                Ok(format!(
                    "UPDATE {target} SET {assignments} WHERE {where_sql}"
                ))
            }
            EditChange::Delete { row } => {
                let where_sql = predicate(data, *row, parameters)?;
                Ok(format!("DELETE FROM {target} WHERE {where_sql}"))
            }
        }
    }
    pub async fn apply(
        &self,
        core: &DatabaseCore,
        session: Uuid,
        profile: &Profile,
        owner: &EditOwner,
        batch: &EditBatch,
        fresh: &MetadataResponse,
    ) -> DbResult<EditOutcome> {
        self.apply_with_deadline(
            core,
            session,
            profile,
            owner,
            batch,
            fresh,
            Duration::from_secs(30),
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn apply_with_deadline(
        &self,
        core: &DatabaseCore,
        session: Uuid,
        profile: &Profile,
        owner: &EditOwner,
        batch: &EditBatch,
        fresh: &MetadataResponse,
        budget: Duration,
    ) -> DbResult<EditOutcome> {
        self.preview(owner, profile.id, batch)?;
        let data = self.data(owner, profile.id, batch.token)?;
        if data.revision != profile.revision
            || serde_json::to_value(fresh).map_err(|_| invalid())?
                != serde_json::to_value(&data.metadata).map_err(|_| invalid())?
        {
            return Ok(EditOutcome::Conflict);
        }
        {
            let mut snapshots = guard(&self.snapshots)?;
            let snapshot = snapshots.get_mut(&batch.token).ok_or_else(invalid)?;
            if snapshot.claimed {
                return Err(error(
                    "EDIT_ALREADY_APPLIED",
                    "Refresh before submitting another edit",
                ));
            }
            snapshot.claimed = true;
        }
        let committing = std::sync::atomic::AtomicBool::new(false);
        let mut readback = Vec::new();
        let job = async {
            collect(core, session, "BEGIN".into(), vec![]).await?;
            let mut targets = batch
                .changes
                .iter()
                .filter_map(|c| match c {
                    EditChange::Update { row, .. } | EditChange::Delete { row } => Some(*row),
                    _ => None,
                })
                .collect::<Vec<_>>();
            targets.sort_unstable();
            for row in targets {
                let mut params = Vec::new();
                let where_sql = predicate(&data, row, &mut params)?;
                let (current, _) = collect(
                    core,
                    session,
                    format!(
                        "SELECT {} FROM {} WHERE {where_sql} FOR UPDATE",
                        data.metadata
                            .columns
                            .iter()
                            .map(|c| quote(data.object.engine, &c.name))
                            .collect::<Vec<_>>()
                            .join(","),
                        table(&data.object)?
                    ),
                    params,
                )
                .await?;
                if current.len() != 1 || !same_row(&current[0], &data.rows[row]) {
                    return Ok(EditOutcome::Conflict);
                }
            }
            for change in &batch.changes {
                let mut params = Vec::new();
                let sql = self.statement(&data, change, &mut params)?;
                let (returned, summary) = collect(core, session, sql, params).await?;
                if summary.affected_rows != 1
                    && !(data.object.engine == Engine::Postgres
                        && matches!(change, EditChange::Insert { .. })
                        && summary.rows_returned == 1)
                    && !(data.object.engine == Engine::MySql
                        && matches!(change, EditChange::Update { .. })
                        && summary.affected_rows == 0)
                {
                    return Ok(EditOutcome::Conflict);
                }
                if !matches!(change, EditChange::Delete { .. }) {
                    let mut identity = match change {
                        EditChange::Update { row, .. } => data.rows[*row].clone(),
                        EditChange::Insert { values } if data.object.engine == Engine::MySql => {
                            let edit = data.metadata.editing.as_ref().ok_or_else(invalid)?;
                            let (last, _) =
                                collect(core, session, "SELECT LAST_INSERT_ID()".into(), vec![])
                                    .await?;
                            data.metadata
                                .columns
                                .iter()
                                .map(|c| {
                                    if edit.primary_key.contains(&c.name)
                                        && edit.generated.contains(&c.name)
                                    {
                                        last.first()
                                            .and_then(|r| r.first())
                                            .cloned()
                                            .ok_or_else(invalid)
                                    } else if let Some(v) = values.get(&c.name) {
                                        Ok(parsed(c, v)?.unwrap_or(DbValue::Null))
                                    } else {
                                        Ok(DbValue::Null)
                                    }
                                })
                                .collect::<DbResult<ResultRow>>()?
                        }
                        EditChange::Insert { .. } => {
                            returned.first().cloned().ok_or_else(invalid)?
                        }
                        _ => return Err(invalid()),
                    };
                    let target = SnapshotData {
                        object: data.object.clone(),
                        metadata: data.metadata.clone(),
                        rows: vec![std::mem::take(&mut identity)],
                        revision: data.revision,
                        readback_ready: false,
                    };
                    let mut parameters = Vec::new();
                    let predicate = predicate(&target, 0, &mut parameters)?;
                    let projection = data
                        .metadata
                        .columns
                        .iter()
                        .map(|c| quote(data.object.engine, &c.name))
                        .collect::<Vec<_>>()
                        .join(",");
                    let (rows, _) = collect(
                        core,
                        session,
                        format!(
                            "SELECT {projection} FROM {} WHERE {predicate}",
                            table(&data.object)?
                        ),
                        parameters,
                    )
                    .await?;
                    if rows.len() != 1 {
                        return Err(invalid());
                    }
                    readback.extend(rows);
                    if serde_json::to_vec(&readback).map_err(|_| invalid())?.len() > 8 * 1024 * 1024
                    {
                        return Err(error(
                            "EDIT_CAPACITY_EXCEEDED",
                            "Mutation readback exceeds capacity",
                        ));
                    }
                }
            }
            committing.store(true, std::sync::atomic::Ordering::Release);
            collect(core, session, "COMMIT".into(), vec![]).await?;
            Ok::<_, DbError>(EditOutcome::Applied)
        };
        let result = match tokio::time::timeout(budget, job).await {
            Ok(value) => value,
            Err(_) => Err(error("QUERY_OUTCOME_UNKNOWN", "Edit deadline exceeded")),
        };
        match result {
            Ok(EditOutcome::Applied) => {
                let _ = self.store_receipt(owner, batch.token, data, readback);
                Ok(EditOutcome::Applied)
            }
            _ if committing.load(std::sync::atomic::Ordering::Acquire) => Ok(EditOutcome::Unknown),
            other => {
                let rolled_back = tokio::time::timeout(
                    Duration::from_secs(5),
                    collect(core, session, "ROLLBACK".into(), vec![]),
                )
                .await
                .is_ok_and(|r| r.is_ok());
                Ok(if !rolled_back {
                    EditOutcome::Unknown
                } else if matches!(other, Ok(EditOutcome::Conflict)) {
                    EditOutcome::Conflict
                } else {
                    EditOutcome::Failed
                })
            }
        }
    }
}
