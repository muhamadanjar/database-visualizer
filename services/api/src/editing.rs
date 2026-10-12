use crate::{
    App,
    auth::{Principal, csrf},
    error::{ApiError, Result, data},
    runtime::Runtime,
    store::Scope,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use db_types::ObjectId;
use query_runtime::edit::{EditBatch, EditOutcome};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;
async fn access(
    app: &App,
    owner: &Principal,
    path: &HashMap<String, String>,
) -> Result<(Scope, Uuid)> {
    let scope = crate::routes::scope(path)?;
    let connection = crate::routes::id(path, "connection_id")?;
    Runtime::edit_owner(scope, owner, Uuid::nil())?;
    if !app.store.role(scope, owner.user).await?.execute() {
        return Err(ApiError::denied());
    }
    app.store.profile(scope, connection).await?;
    Ok((scope, connection))
}
#[derive(Deserialize)]
pub struct SnapshotRequest {
    pub object: ObjectId,
    pub tab_id: Uuid,
    #[serde(default)]
    pub offset: u32,
}
pub async fn snapshot(
    State(app): State<Arc<App>>,
    owner: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(value): Json<SnapshotRequest>,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &owner)?;
    let (scope, connection) = access(&app, &owner, &path).await?;
    Ok(data(
        serde_json::to_value(
            app.runtime
                .edit_snapshot(
                    scope,
                    owner,
                    connection,
                    value.object,
                    value.tab_id,
                    value.offset,
                )
                .await?,
        )
        .map_err(|_| ApiError::invalid())?,
    ))
}
#[derive(Deserialize)]
pub struct Readback {
    pub tab_id: Uuid,
    pub token: Uuid,
}
pub async fn readback(
    State(app): State<Arc<App>>,
    owner: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(value): Json<Readback>,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &owner)?;
    let (scope, connection) = access(&app, &owner, &path).await?;
    Ok(data(
        serde_json::to_value(app.runtime.edits.readback(
            &Runtime::edit_owner(scope, &owner, value.tab_id)?,
            connection,
            value.token,
        )?)
        .map_err(|_| ApiError::invalid())?,
    ))
}
pub async fn preview(
    State(app): State<Arc<App>>,
    owner: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(batch): Json<EditBatch>,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &owner)?;
    let (scope, connection) = access(&app, &owner, &path).await?;
    Ok(data(app.runtime.edits.preview(
        &Runtime::edit_owner(scope, &owner, batch.tab_id)?,
        connection,
        &batch,
    )?))
}
pub async fn discard(
    State(app): State<Arc<App>>,
    owner: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &owner)?;
    let (scope, _) = access(&app, &owner, &path).await?;
    app.runtime.edits.discard_owner(&Runtime::edit_owner(
        scope,
        &owner,
        crate::routes::id(&path, "tab_id")?,
    )?);
    Ok(data(Value::Null))
}
pub async fn apply(
    State(app): State<Arc<App>>,
    owner: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(batch): Json<EditBatch>,
) -> Result<(StatusCode, Json<Value>)> {
    csrf(&app, &headers, &owner)?;
    let (scope, connection) = access(&app, &owner, &path).await?;
    let key = headers
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(ApiError::invalid)?;
    if key.is_empty() || key.len() > 128 {
        return Err(ApiError::invalid());
    }
    let bytes = serde_json::to_vec(&batch).map_err(|_| ApiError::invalid())?;
    if bytes.len() > 1024 * 1024 {
        return Err(ApiError::invalid());
    }
    let context = scope.context(owner.user, &format!("mutation/{connection}/{}", key));
    let mut tx = app.store.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "{}/{}/{}/{}",
            scope.organization, scope.workspace, owner.user, key
        ))
        .execute(&mut *tx)
        .await?;
    let existing=sqlx::query("SELECT id,connection_id,login_hash,tab_hash,request_hash,state FROM mutations WHERE organization_id=$1 AND workspace_id=$2 AND user_id=$3 AND idempotency_key=$4")
 .bind(scope.organization).bind(scope.workspace).bind(owner.user).bind(key).fetch_optional(&mut *tx).await?;
    if let Some(row) = existing {
        if row.try_get::<Uuid, _>("connection_id")? != connection
            || row.try_get::<Vec<u8>, _>("login_hash")? != owner.login
            || Some(row.try_get::<Vec<u8>, _>("tab_hash")?) != owner.tab
            || !app.store.keys.matches_fingerprint(
                &context,
                &bytes,
                &row.try_get::<Vec<u8>, _>("request_hash")?,
            )?
        {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "IDEMPOTENCY_CONFLICT",
                "Mutation key already used",
            ));
        }
        return Ok((
            StatusCode::OK,
            data(
                json!({"id":row.try_get::<Uuid,_>("id")?,"state":row.try_get::<String,_>("state")?}),
            ),
        ));
    }
    let edit_owner = Runtime::edit_owner(scope, &owner, batch.tab_id)?;
    app.runtime.edits.preview(&edit_owner, connection, &batch)?;
    let object = app
        .runtime
        .edits
        .object(&edit_owner, connection, batch.token)?;
    let id = Uuid::new_v4();
    let fingerprint = app.store.keys.fingerprint(&context, &bytes)?;
    sqlx::query("INSERT INTO mutations(organization_id,workspace_id,connection_id,id,user_id,login_hash,tab_hash,idempotency_key,request_hash,state) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'Applying')")
 .bind(scope.organization).bind(scope.workspace).bind(connection).bind(id).bind(owner.user).bind(&owner.login).bind(&owner.tab).bind(key).bind(fingerprint).execute(&mut *tx).await?;
    tx.commit().await?;
    app.runtime
        .edit_cancellations
        .lock()
        .map_err(|_| ApiError::unavailable())?
        .insert(id, tokio_util::sync::CancellationToken::new());
    let task = app.clone();
    tokio::spawn(async move {
        let result = task
            .runtime
            .edit_apply(scope, owner.clone(), connection, id, batch, object)
            .await
            .unwrap_or(EditOutcome::Unknown);
        if let Ok(mut cancellations) = task.runtime.edit_cancellations.lock() {
            cancellations.remove(&id);
        }
        let state = format!("{result:?}");
        let _=sqlx::query("UPDATE mutations SET state=$4,finished_at=now() WHERE organization_id=$1 AND workspace_id=$2 AND id=$3").bind(scope.organization).bind(scope.workspace).bind(id).bind(&state).execute(&task.store.pool).await;
        let _ = task
            .store
            .audit(scope, owner.user, "mutation.finished", Some(id), &state)
            .await;
    });
    Ok((
        StatusCode::ACCEPTED,
        data(json!({"id":id,"state":"Applying"})),
    ))
}
#[derive(Deserialize)]
pub struct Lookup {
    pub idempotency_key: Option<String>,
}
pub async fn status(
    State(app): State<Arc<App>>,
    owner: Principal,
    Path(path): Path<HashMap<String, String>>,
    Query(query): Query<Lookup>,
) -> Result<Json<Value>> {
    let (scope, connection) = access(&app, &owner, &path).await?;
    let id = path
        .get("mutation_id")
        .and_then(|v| Uuid::parse_str(v).ok());
    let row=sqlx::query("SELECT id,CASE WHEN state='Applying' AND created_at<now()-interval '2 minutes' THEN 'Unknown' ELSE state END AS state FROM mutations WHERE organization_id=$1 AND workspace_id=$2 AND connection_id=$3 AND user_id=$4 AND login_hash=$5 AND tab_hash=$6 AND (($7::uuid IS NOT NULL AND id=$7) OR ($7 IS NULL AND idempotency_key=$8))")
 .bind(scope.organization).bind(scope.workspace).bind(connection).bind(owner.user).bind(&owner.login).bind(&owner.tab).bind(id).bind(query.idempotency_key).fetch_optional(&app.store.pool).await?.ok_or_else(ApiError::missing)?;
    Ok(data(
        json!({"id":row.try_get::<Uuid,_>("id")?,"state":row.try_get::<String,_>("state")?}),
    ))
}
pub async fn cancel(
    State(app): State<Arc<App>>,
    owner: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &owner)?;
    let (scope, connection) = access(&app, &owner, &path).await?;
    let id = crate::routes::id(&path, "mutation_id")?;
    let own:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mutations WHERE organization_id=$1 AND workspace_id=$2 AND connection_id=$3 AND id=$4 AND user_id=$5 AND login_hash=$6 AND tab_hash=$7)").bind(scope.organization).bind(scope.workspace).bind(connection).bind(id).bind(owner.user).bind(&owner.login).bind(&owner.tab).fetch_one(&app.store.pool).await?;
    if !own {
        return Err(ApiError::missing());
    }
    if let Some(token) = app
        .runtime
        .edit_cancellations
        .lock()
        .map_err(|_| ApiError::unavailable())?
        .get(&id)
    {
        token.cancel();
    }
    let session = app
        .runtime
        .edit_sessions
        .lock()
        .map_err(|_| ApiError::unavailable())?
        .get(&id)
        .copied();
    if let Some(session) = session {
        let _ = app.runtime.disconnect(scope, &owner, session, true).await;
    }
    Ok(data(json!({"id":id,"cancel_requested":true})))
}
