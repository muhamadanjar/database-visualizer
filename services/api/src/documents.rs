use crate::{
    App,
    auth::{Principal, csrf},
    error::{ApiError, Result, data},
    store::{Pagination, Scope, Store},
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use query_runtime::{ExecutionInfo, Profile};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct DocumentQuery {
    #[serde(default = "saved")]
    pub kind: String,
    pub revision: Option<i64>,
    #[serde(flatten)]
    pub pagination: Pagination,
}
fn saved() -> String {
    "saved".into()
}
#[derive(Serialize, Deserialize)]
pub struct DocumentWrite {
    pub id: Uuid,
    pub kind: String,
    pub name: String,
    pub content: Value,
    #[serde(default)]
    pub revision: i64,
}
#[derive(Deserialize)]
pub struct Rename {
    pub name: String,
    pub revision: i64,
}
fn context(scope: Scope, id: Uuid, user: Uuid, kind: &str) -> Vec<u8> {
    scope.context(id, &format!("document/{user}/{kind}"))
}
fn valid_kind(kind: &str) -> Result<()> {
    if !matches!(kind, "saved" | "history" | "editor") {
        return Err(ApiError::invalid());
    }
    Ok(())
}
impl Store {
    pub async fn save_history(
        &self,
        scope: Scope,
        user: Uuid,
        profile: &Profile,
        id: Uuid,
        sql: &str,
        info: &ExecutionInfo,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let enabled:Option<Value>=sqlx::query_scalar("SELECT profile FROM connections WHERE organization_id=$1 AND workspace_id=$2 AND id=$3 FOR SHARE").bind(scope.organization).bind(scope.workspace).bind(profile.id).fetch_optional(&mut *tx).await?;
        if enabled.is_none_or(|p| p["history_enabled"] != true) {
            return Ok(());
        }
        let value = json!({"sql":sql,"execution":info});
        let encrypted = self.keys.seal(
            &context(scope, id, user, "history"),
            &serde_json::to_vec(&value).map_err(|_| ApiError::invalid())?,
        )?;
        sqlx::query("INSERT INTO documents(organization_id,workspace_id,connection_id,id,user_id,kind,name,content,created_at_ms) VALUES($1,$2,$3,$4,$5,'history',$6,$7,$8) ON CONFLICT DO NOTHING")
            .bind(scope.organization).bind(scope.workspace).bind(profile.id).bind(id).bind(user).bind(format!("Execution {}",info.started_at_ms)).bind(encrypted).bind(info.started_at_ms as i64).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn finish_execution(
        &self,
        scope: Scope,
        user: Uuid,
        info: &ExecutionInfo,
    ) -> Result<()> {
        let status = format!("{:?}", info.status);
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE executions SET state=$5,outcome=$6,finished_at=now() WHERE organization_id=$1 AND workspace_id=$2 AND user_id=$3 AND id=$4")
            .bind(scope.organization).bind(scope.workspace).bind(user).bind(info.id).bind(&status).bind(if info.status==db_types::QueryStatus::Completed{"ExecutionCompleted"}else{"Unknown"}).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO audit_events(organization_id,workspace_id,id,user_id,action,resource_id,outcome) VALUES($1,$2,$3,$4,'execution.finished',$5,$6)").bind(scope.organization).bind(scope.workspace).bind(Uuid::new_v4()).bind(user).bind(info.id).bind(status).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
pub async fn headers(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    Query(query): Query<DocumentQuery>,
) -> Result<Json<Value>> {
    let scope = crate::routes::scope(&path)?;
    let connection = crate::routes::id(&path, "connection_id")?;
    if !app.store.role(scope, principal.user).await?.execute() {
        return Err(ApiError::denied());
    }
    app.store.profile(scope, connection).await?;
    valid_kind(&query.kind)?;
    let offset = query.pagination.offset()?;
    let total:i64=sqlx::query_scalar("SELECT count(*) FROM documents WHERE organization_id=$1 AND workspace_id=$2 AND connection_id=$3 AND kind=$4 AND (kind='saved' OR user_id=$5)").bind(scope.organization).bind(scope.workspace).bind(connection).bind(&query.kind).bind(principal.user).fetch_one(&app.store.pool).await?;
    let role = app.store.role(scope, principal.user).await?;
    let rows=sqlx::query("SELECT id,name,kind,user_id,created_at_ms,revision,EXISTS(SELECT 1 FROM saved_query_favorites f WHERE f.organization_id=documents.organization_id AND f.workspace_id=documents.workspace_id AND f.document_id=documents.id AND f.user_id=$5) AS favorite FROM documents WHERE organization_id=$1 AND workspace_id=$2 AND connection_id=$3 AND kind=$4 AND (kind='saved' OR user_id=$5) ORDER BY created_at_ms DESC,id DESC LIMIT $6 OFFSET $7").bind(scope.organization).bind(scope.workspace).bind(connection).bind(&query.kind).bind(principal.user).bind(query.pagination.per_page).bind(offset).fetch_all(&app.store.pool).await?;
    let mut values = Vec::new();
    for row in rows {
        values.push(json!({"id":row.try_get::<Uuid,_>("id")?,"connection_id":connection,"kind":row.try_get::<String,_>("kind")?,"name":row.try_get::<String,_>("name")?,"created_at_ms":row.try_get::<i64,_>("created_at_ms")?,"revision":row.try_get::<i64,_>("revision")?,"favorite":row.try_get::<bool,_>("favorite")?,"can_edit":row.try_get::<Uuid,_>("user_id")?==principal.user || role.manage()}));
    }
    Ok(Json(query.pagination.envelope(json!(values), total)))
}
pub async fn read(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    Query(query): Query<DocumentQuery>,
) -> Result<Json<Value>> {
    let scope = crate::routes::scope(&path)?;
    let connection = crate::routes::id(&path, "connection_id")?;
    let id = crate::routes::id(&path, "document_id")?;
    if !app.store.role(scope, principal.user).await?.execute() {
        return Err(ApiError::denied());
    }
    valid_kind(&query.kind)?;
    let row=sqlx::query("SELECT name,kind,content,user_id,created_at_ms,revision FROM documents WHERE organization_id=$1 AND workspace_id=$2 AND connection_id=$3 AND id=$4 AND kind=$5 AND (kind='saved' OR user_id=$6)").bind(scope.organization).bind(scope.workspace).bind(connection).bind(id).bind(&query.kind).bind(principal.user).fetch_optional(&app.store.pool).await?.ok_or_else(ApiError::missing)?;
    let owner: Uuid = row.try_get("user_id")?;
    let encrypted: Vec<u8> = row.try_get("content")?;
    let bytes = app
        .store
        .keys
        .open(&context(scope, id, owner, &query.kind), &encrypted)?;
    let content: Value = serde_json::from_slice(&bytes).map_err(|_| ApiError::unavailable())?;
    Ok(data(
        json!({"id":id,"connection_id":connection,"kind":query.kind,"name":row.try_get::<String,_>("name")?,"content":content,"created_at_ms":row.try_get::<i64,_>("created_at_ms")?,"revision":row.try_get::<i64,_>("revision")?}),
    ))
}
pub async fn write(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(body): Json<DocumentWrite>,
) -> Result<(StatusCode, Json<Value>)> {
    csrf(&app, &headers, &principal)?;
    let scope = crate::routes::scope(&path)?;
    let connection = crate::routes::id(&path, "connection_id")?;
    let role = app.store.role(scope, principal.user).await?;
    if !role.execute() {
        return Err(ApiError::denied());
    }
    let profile = app.store.profile(scope, connection).await?;
    if !matches!(body.kind.as_str(), "saved" | "editor")
        || body.kind == "editor" && !profile.restore_sql
        || body.name.trim().is_empty()
        || body.name.len() > 512
        || body.content["sql"]
            .as_str()
            .is_none_or(|sql| sql.len() > 1024 * 1024)
        || body.revision < 0
    {
        return Err(ApiError::invalid());
    }
    let content = json!({"sql":body.content["sql"],"cursor":body.content.get("cursor").and_then(|v|v.as_u64()).filter(|n|*n<=1_048_576)});
    let mut tx = app.store.pool.begin().await?;
    if body.kind == "editor" {
        sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
            .bind(principal.user)
            .fetch_one(&mut *tx)
            .await?;
        let total: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM documents WHERE user_id=$1 AND kind='editor' AND id!=$2",
        )
        .bind(principal.user)
        .bind(body.id)
        .fetch_one(&mut *tx)
        .await?;
        if total >= 64 {
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "EDITOR_RESTORE_CAPACITY",
                "Restore allows 64 editors per user",
            ));
        }
    }
    let locked:Value=sqlx::query_scalar("SELECT profile FROM connections WHERE organization_id=$1 AND workspace_id=$2 AND id=$3 FOR UPDATE").bind(scope.organization).bind(scope.workspace).bind(connection).fetch_one(&mut *tx).await?;
    if body.kind == "editor" && locked["restore_sql"] != true {
        return Err(ApiError::invalid());
    }
    let existing=sqlx::query("SELECT user_id,revision,kind,connection_id FROM documents WHERE organization_id=$1 AND workspace_id=$2 AND id=$3 FOR UPDATE").bind(scope.organization).bind(scope.workspace).bind(body.id).fetch_optional(&mut *tx).await?;
    let owner = if let Some(row) = existing {
        let owner: Uuid = row.try_get("user_id")?;
        if row.try_get::<String, _>("kind")? != body.kind
            || row.try_get::<Uuid, _>("connection_id")? != connection
            || owner != principal.user && !(body.kind == "saved" && role.manage())
        {
            return Err(ApiError::denied());
        }
        if row.try_get::<i64, _>("revision")? != body.revision {
            return Err(ApiError::conflict());
        }
        owner
    } else {
        if body.revision != 0 {
            return Err(ApiError::conflict());
        }
        if body.kind == "editor" {
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM documents WHERE organization_id=$1 AND workspace_id=$2 AND connection_id=$3 AND user_id=$4 AND kind='editor'").bind(scope.organization).bind(scope.workspace).bind(connection).bind(principal.user).fetch_one(&mut *tx).await?;
            if count >= 16 {
                return Err(ApiError::new(
                    axum::http::StatusCode::CONFLICT,
                    "EDITOR_RESTORE_CAPACITY",
                    "Restore allows 16 editors per connection",
                ));
            }
        }
        principal.user
    };
    let encrypted = app.store.keys.seal(
        &context(scope, body.id, owner, &body.kind),
        &serde_json::to_vec(&content).map_err(|_| ApiError::invalid())?,
    )?;
    let revision = body
        .revision
        .checked_add(1)
        .ok_or_else(ApiError::conflict)?;
    sqlx::query("INSERT INTO documents(organization_id,workspace_id,connection_id,id,user_id,kind,name,content,revision,created_at_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(organization_id,workspace_id,id) DO UPDATE SET name=excluded.name,content=excluded.content,revision=excluded.revision").bind(scope.organization).bind(scope.workspace).bind(connection).bind(body.id).bind(owner).bind(&body.kind).bind(body.name).bind(encrypted).bind(revision).bind(query_runtime::now_ms() as i64).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok((
        if body.revision == 0 {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        data(json!({"id":body.id,"revision":revision})),
    ))
}
pub async fn rename(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    Query(query): Query<DocumentQuery>,
    headers: HeaderMap,
    Json(body): Json<Rename>,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = crate::routes::scope(&path)?;
    let connection = crate::routes::id(&path, "connection_id")?;
    let id = crate::routes::id(&path, "document_id")?;
    let role = app.store.role(scope, principal.user).await?;
    if !role.execute() {
        return Err(ApiError::denied());
    }
    if query.kind != "saved"
        || body.name.trim().is_empty()
        || body.name.len() > 512
        || body.revision < 1
    {
        return Err(ApiError::invalid());
    }
    let changed=sqlx::query("UPDATE documents SET name=$6,revision=revision+1 WHERE organization_id=$1 AND workspace_id=$2 AND connection_id=$3 AND id=$4 AND kind='saved' AND revision=$5 AND (user_id=$7 OR $8)").bind(scope.organization).bind(scope.workspace).bind(connection).bind(id).bind(body.revision).bind(body.name).bind(principal.user).bind(role.manage()).execute(&app.store.pool).await?;
    if changed.rows_affected() != 1 {
        return Err(ApiError::conflict());
    }
    Ok(data(json!({"id":id,"revision":body.revision+1})))
}
pub async fn delete(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    Query(query): Query<DocumentQuery>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = crate::routes::scope(&path)?;
    let connection = crate::routes::id(&path, "connection_id")?;
    let role = app.store.role(scope, principal.user).await?;
    if !role.execute() {
        return Err(ApiError::denied());
    }
    valid_kind(&query.kind)?;
    let id = path
        .get("document_id")
        .map(|_| crate::routes::id(&path, "document_id"))
        .transpose()?;
    if id.is_none() && query.kind == "saved" {
        return Err(ApiError::invalid());
    }
    if id.is_some() && query.kind != "history" && query.revision.is_none_or(|revision| revision < 1)
    {
        return Err(ApiError::invalid());
    }
    let changed=sqlx::query("DELETE FROM documents WHERE organization_id=$1 AND workspace_id=$2 AND connection_id=$3 AND kind=$4 AND (id=$5 OR $5 IS NULL) AND (user_id=$6 OR (kind='saved' AND $7)) AND ($8::bigint IS NULL OR revision=$8)").bind(scope.organization).bind(scope.workspace).bind(connection).bind(query.kind).bind(id).bind(principal.user).bind(role.manage()).bind(query.revision).execute(&app.store.pool).await?;
    if id.is_some() && query.revision.is_some() && changed.rows_affected() != 1 {
        return Err(ApiError::conflict());
    }
    Ok(data(Value::Null))
}
