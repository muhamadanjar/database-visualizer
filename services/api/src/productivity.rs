use crate::{
    App,
    auth::{Principal, csrf},
    error::{ApiError, Result, data},
    store::Scope,
};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use query_runtime::search::{SearchBudget, SearchCursor, SearchRequest};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;
fn scope(path: &HashMap<String, String>) -> Result<Scope> {
    crate::routes::scope(path)
}
async fn authorize(app: &App, principal: &Principal, scope: Scope, connection: Uuid) -> Result<()> {
    if !app.store.role(scope, principal.user).await?.execute() {
        return Err(ApiError::denied());
    }
    app.store.profile(scope, connection).await?;
    Ok(())
}
pub async fn search(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(request): Json<SearchRequest>,
) -> Result<Json<Value>> {
    let mut budget = SearchBudget::default();
    csrf(&app, &headers, &principal)?;
    request.validate()?;
    let scope = scope(&path)?;
    let connection = crate::routes::id(&path, "connection_id")?;
    tokio::time::timeout(
        budget.remaining(),
        authorize(&app, &principal, scope, connection),
    )
    .await
    .map_err(|_| ApiError::unavailable())??;
    let context = scope.context(connection, &format!("search/{}", principal.user));
    let mut cursor = match &request.cursor {
        Some(value) => serde_json::from_slice::<SearchCursor>(&app.store.keys.open(
            &context,
            &B64.decode(value).map_err(|_| ApiError::invalid())?,
        )?)
        .map_err(|_| ApiError::invalid())?,
        None => SearchCursor::initial(&request),
    };
    cursor.validate(&request)?;
    let role = tokio::time::timeout(budget.remaining(), app.store.role(scope, principal.user))
        .await
        .map_err(|_| ApiError::unavailable())??;
    let mut hits = Vec::new();
    let mut complete = false;
    // Small database pages avoid decrypting/materializing the entire document collection.
    'scan: loop {
        if budget.remaining() <= std::time::Duration::from_millis(100) {
            break;
        }
        let rows=tokio::time::timeout(budget.remaining().saturating_sub(std::time::Duration::from_millis(100)), sqlx::query("SELECT d.id,d.name,d.content,d.user_id,d.created_at_ms,d.revision,EXISTS(SELECT 1 FROM saved_query_favorites f WHERE f.organization_id=d.organization_id AND f.workspace_id=d.workspace_id AND f.document_id=d.id AND f.user_id=$5) AS favorite FROM documents d WHERE d.organization_id=$1 AND d.workspace_id=$2 AND d.connection_id=$3 AND d.kind=$4 AND (d.kind='saved' OR d.user_id=$5) AND d.created_at_ms<=$6 AND (d.created_at_ms,d.id)<($7,$8) ORDER BY d.created_at_ms DESC,d.id DESC LIMIT 1")
   .bind(scope.organization).bind(scope.workspace).bind(connection).bind(&request.kind).bind(principal.user).bind(cursor.watermark as i64).bind(cursor.before_ms as i64).bind(cursor.before_id).fetch_all(&app.store.pool)).await;
        let rows = match rows {
            Ok(rows) => rows?,
            Err(_) => break 'scan,
        };
        if rows.is_empty() {
            complete = true;
            break;
        }
        for row in rows {
            let bytes: Vec<u8> = row.try_get("content")?;
            if bytes.len() > 10 * 1024 * 1024 {
                return Err(ApiError::new(
                    axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                    "SEARCH_DOCUMENT_TOO_LARGE",
                    "Document exceeds search scan budget",
                ));
            }
            if !budget.admit(bytes.len()) {
                break 'scan;
            }
            let id: Uuid = row.try_get("id")?;
            let owner: Uuid = row.try_get("user_id")?;
            let created: i64 = row.try_get("created_at_ms")?;
            let plaintext = app.store.keys.open(
                &scope.context(id, &format!("document/{owner}/{}", request.kind)),
                &bytes,
            )?;
            let content: Value =
                serde_json::from_slice(&plaintext).map_err(|_| ApiError::unavailable())?;
            let name: String = row.try_get("name")?;
            let favorite: bool = row.try_get("favorite")?;
            cursor.before_ms = created as u64;
            cursor.before_id = id;
            if request.matches(&name, &content, created as u64, favorite) {
                hits.push(json!({"id":id,"connection_id":connection,"kind":request.kind,"name":name,"created_at_ms":created,"revision":row.try_get::<i64,_>("revision")?,"can_edit":owner==principal.user||role.manage(),"favorite":favorite}));
            }
            if hits.len() == 100 {
                break 'scan;
            }
        }
    }
    tokio::time::timeout(
        budget.remaining(),
        authorize(&app, &principal, scope, connection),
    )
    .await
    .map_err(|_| ApiError::unavailable())??;
    let next = if complete {
        None
    } else {
        Some(B64.encode(app.store.keys.seal(
            &context,
            &serde_json::to_vec(&cursor).map_err(|_| ApiError::invalid())?,
        )?))
    };
    Ok(data(
        json!({"items":hits,"next_cursor":next,"complete":complete,"scanned":budget.scanned}),
    ))
}
#[derive(Deserialize)]
pub struct Favorite {
    pub favorite: bool,
}
pub async fn favorite(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(value): Json<Favorite>,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    let connection = crate::routes::id(&path, "connection_id")?;
    let id = crate::routes::id(&path, "document_id")?;
    authorize(&app, &principal, scope, connection).await?;
    let mut tx = app.store.pool.begin().await?;
    let found:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM documents WHERE organization_id=$1 AND workspace_id=$2 AND connection_id=$3 AND id=$4 AND kind='saved')").bind(scope.organization).bind(scope.workspace).bind(connection).bind(id).fetch_one(&mut *tx).await?;
    if !found {
        return Err(ApiError::missing());
    }
    if value.favorite {
        sqlx::query("INSERT INTO saved_query_favorites VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(scope.organization)
            .bind(scope.workspace)
            .bind(principal.user)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("DELETE FROM saved_query_favorites WHERE organization_id=$1 AND workspace_id=$2 AND user_id=$3 AND document_id=$4").bind(scope.organization).bind(scope.workspace).bind(principal.user).bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(data(json!({"favorite":value.favorite})))
}
