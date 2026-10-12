use crate::{
    App,
    auth::{Principal, csrf},
    error::{ApiError, Result, data},
    store::{Pagination, Scope},
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use query_runtime::{Profile, TableRequest};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

pub fn id(path: &HashMap<String, String>, key: &str) -> Result<Uuid> {
    path.get(key)
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or_else(ApiError::invalid)
}
pub fn scope(path: &HashMap<String, String>) -> Result<Scope> {
    Ok(Scope {
        organization: id(path, "organization_id")?,
        workspace: id(path, "workspace_id")?,
    })
}
#[derive(Deserialize)]
pub struct Named {
    pub name: String,
}
fn name(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 512 {
        return Err(ApiError::invalid());
    }
    Ok(())
}
#[derive(Deserialize)]
pub struct MemberWrite {
    pub user_id: Uuid,
    pub role: Option<String>,
}
#[derive(Deserialize)]
pub struct ConnectionWrite {
    pub profile: Profile,
    pub password: Option<String>,
}
#[derive(Deserialize)]
pub struct Execute {
    pub sql: String,
}
#[derive(Deserialize)]
pub struct AutoCommit {
    pub enabled: bool,
}
#[derive(Deserialize)]
pub struct Force {
    #[serde(default)]
    pub force: bool,
}
pub use crate::runtime::Transform;

pub async fn organizations(
    State(app): State<Arc<App>>,
    principal: Principal,
    Query(page): Query<Pagination>,
) -> Result<Json<Value>> {
    let offset = page.offset()?;
    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM organization_members WHERE user_id=$1")
            .bind(principal.user)
            .fetch_one(&app.store.pool)
            .await?;
    let rows=sqlx::query("SELECT o.id,o.name,m.role FROM organizations o JOIN organization_members m ON m.organization_id=o.id WHERE m.user_id=$1 ORDER BY o.id LIMIT $2 OFFSET $3").bind(principal.user).bind(page.per_page).bind(offset).fetch_all(&app.store.pool).await?;
    let mut values = Vec::new();
    for row in rows {
        values.push(json!({"id":row.try_get::<Uuid,_>("id")?,"name":row.try_get::<String,_>("name")?,"role":row.try_get::<String,_>("role")?}));
    }
    Ok(Json(page.envelope(json!(values), total)))
}
pub async fn workspaces(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    Query(page): Query<Pagination>,
) -> Result<Json<Value>> {
    let organization = id(&path, "organization_id")?;
    let offset = page.offset()?;
    let total:i64=sqlx::query_scalar("SELECT count(*) FROM workspaces w JOIN organization_members om ON om.organization_id=w.organization_id AND om.user_id=$2 LEFT JOIN workspace_members wm ON wm.organization_id=w.organization_id AND wm.workspace_id=w.id AND wm.user_id=$2 WHERE w.organization_id=$1 AND (om.role='Owner' OR wm.role IS NOT NULL)").bind(organization).bind(principal.user).fetch_one(&app.store.pool).await?;
    let rows=sqlx::query("SELECT w.id,w.name,CASE WHEN om.role='Owner' THEN 'Owner' ELSE wm.role END AS role FROM workspaces w JOIN organization_members om ON om.organization_id=w.organization_id AND om.user_id=$2 LEFT JOIN workspace_members wm ON wm.organization_id=w.organization_id AND wm.workspace_id=w.id AND wm.user_id=$2 WHERE w.organization_id=$1 AND (om.role='Owner' OR wm.role IS NOT NULL) ORDER BY w.id LIMIT $3 OFFSET $4").bind(organization).bind(principal.user).bind(page.per_page).bind(offset).fetch_all(&app.store.pool).await?;
    let mut values = Vec::new();
    for row in rows {
        values.push(json!({"id":row.try_get::<Uuid,_>("id")?,"name":row.try_get::<String,_>("name")?,"role":row.try_get::<String,_>("role")?}));
    }
    Ok(Json(page.envelope(json!(values), total)))
}
pub async fn create_workspace(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(body): Json<Named>,
) -> Result<(StatusCode, Json<Value>)> {
    csrf(&app, &headers, &principal)?;
    name(&body.name)?;
    let organization = id(&path, "organization_id")?;
    app.store
        .organization_owner(organization, principal.user)
        .await?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces(organization_id,id,name) VALUES($1,$2,$3)")
        .bind(organization)
        .bind(id)
        .bind(&body.name)
        .execute(&app.store.pool)
        .await?;
    app.store
        .audit(
            Scope {
                organization,
                workspace: id,
            },
            principal.user,
            "workspace.created",
            Some(id),
            "Completed",
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        data(json!({"id":id,"name":body.name,"role":"Owner"})),
    ))
}
pub async fn rename_workspace(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(body): Json<Named>,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    name(&body.name)?;
    let scope = scope(&path)?;
    if !app.store.role(scope, principal.user).await?.manage() {
        return Err(ApiError::denied());
    }
    sqlx::query("UPDATE workspaces SET name=$3 WHERE organization_id=$1 AND id=$2")
        .bind(scope.organization)
        .bind(scope.workspace)
        .bind(body.name)
        .execute(&app.store.pool)
        .await?;
    app.store
        .audit(
            scope,
            principal.user,
            "workspace.updated",
            Some(scope.workspace),
            "Completed",
        )
        .await?;
    Ok(data(Value::Null))
}
pub async fn delete_workspace(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    if !app.store.role(scope, principal.user).await?.manage() {
        return Err(ApiError::denied());
    }
    app.runtime.invalidate(scope, None, None).await;
    app.store
        .audit(
            scope,
            principal.user,
            "workspace.deleted",
            Some(scope.workspace),
            "Completed",
        )
        .await?;
    sqlx::query("DELETE FROM workspaces WHERE organization_id=$1 AND id=$2")
        .bind(scope.organization)
        .bind(scope.workspace)
        .execute(&app.store.pool)
        .await?;
    Ok(data(Value::Null))
}
pub async fn members(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    Query(page): Query<Pagination>,
) -> Result<Json<Value>> {
    let organization = id(&path, "organization_id")?;
    let workspace = path
        .get("workspace_id")
        .map(|_| id(&path, "workspace_id"))
        .transpose()?;
    let offset = page.offset()?;
    if let Some(workspace) = workspace {
        if !app
            .store
            .role(
                Scope {
                    organization,
                    workspace,
                },
                principal.user,
            )
            .await?
            .manage()
        {
            return Err(ApiError::denied());
        }
    } else {
        app.store
            .organization_owner(organization, principal.user)
            .await?;
    }
    let rows = if let Some(workspace) = workspace {
        sqlx::query("SELECT wm.user_id,u.display_name,wm.role,count(*) OVER() AS total FROM workspace_members wm JOIN users u ON u.id=wm.user_id WHERE wm.organization_id=$1 AND wm.workspace_id=$2 ORDER BY wm.user_id LIMIT $3 OFFSET $4").bind(organization).bind(workspace).bind(page.per_page).bind(offset).fetch_all(&app.store.pool).await?
    } else {
        sqlx::query("SELECT om.user_id,u.display_name,om.role,count(*) OVER() AS total FROM organization_members om JOIN users u ON u.id=om.user_id WHERE om.organization_id=$1 ORDER BY om.user_id LIMIT $2 OFFSET $3").bind(organization).bind(page.per_page).bind(offset).fetch_all(&app.store.pool).await?
    };
    let total: i64 = if let Some(workspace) = workspace {
        sqlx::query_scalar(
            "SELECT count(*) FROM workspace_members WHERE organization_id=$1 AND workspace_id=$2",
        )
        .bind(organization)
        .bind(workspace)
        .fetch_one(&app.store.pool)
        .await?
    } else {
        sqlx::query_scalar("SELECT count(*) FROM organization_members WHERE organization_id=$1")
            .bind(organization)
            .fetch_one(&app.store.pool)
            .await?
    };
    let mut values = Vec::new();
    for row in rows {
        values.push(json!({"user_id":row.try_get::<Uuid,_>("user_id")?,"display_name":row.try_get::<String,_>("display_name")?,"role":row.try_get::<String,_>("role")?}));
    }
    Ok(Json(page.envelope(json!(values), total)))
}
pub async fn add_member(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(body): Json<MemberWrite>,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let organization = id(&path, "organization_id")?;
    if path.contains_key("workspace_id") {
        let scope = scope(&path)?;
        let role = app.store.role(scope, principal.user).await?;
        if !role.manage() {
            return Err(ApiError::denied());
        }
        let assigned = body.role.as_deref().unwrap_or("Viewer");
        if !matches!(assigned, "Admin" | "Developer" | "Viewer") {
            return Err(ApiError::invalid());
        }
        sqlx::query("INSERT INTO workspace_members(organization_id,workspace_id,user_id,role) VALUES($1,$2,$3,$4) ON CONFLICT(organization_id,workspace_id,user_id) DO UPDATE SET role=excluded.role").bind(organization).bind(scope.workspace).bind(body.user_id).bind(assigned).execute(&app.store.pool).await?;
        if assigned == "Viewer" {
            app.runtime
                .invalidate(scope, Some(body.user_id), None)
                .await;
        }
        app.store
            .audit(
                scope,
                principal.user,
                "membership.updated",
                Some(body.user_id),
                "Completed",
            )
            .await?;
    } else {
        app.store
            .organization_owner(organization, principal.user)
            .await?;
        if body.role.as_deref().is_some_and(|role| role != "Member") {
            return Err(ApiError::invalid());
        }
        sqlx::query(
            "INSERT INTO organization_members VALUES($1,$2,'Member') ON CONFLICT DO NOTHING",
        )
        .bind(organization)
        .bind(body.user_id)
        .execute(&app.store.pool)
        .await?;
        app.store
            .audit_organization(
                organization,
                principal.user,
                "membership.added",
                Some(body.user_id),
            )
            .await?;
    }
    Ok(data(Value::Null))
}
pub async fn remove_member(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let organization = id(&path, "organization_id")?;
    let user = id(&path, "user_id")?;
    if path.contains_key("workspace_id") {
        let scope = scope(&path)?;
        if !app.store.role(scope, principal.user).await?.manage() {
            return Err(ApiError::denied());
        }
        sqlx::query("DELETE FROM workspace_members WHERE organization_id=$1 AND workspace_id=$2 AND user_id=$3").bind(organization).bind(scope.workspace).bind(user).execute(&app.store.pool).await?;
        app.runtime.invalidate(scope, Some(user), None).await;
        app.store
            .audit(
                scope,
                principal.user,
                "membership.removed",
                Some(user),
                "Completed",
            )
            .await?;
    } else {
        app.store
            .organization_owner(organization, principal.user)
            .await?;
        let changed=sqlx::query("DELETE FROM organization_members WHERE organization_id=$1 AND user_id=$2 AND role!='Owner'").bind(organization).bind(user).execute(&app.store.pool).await?;
        if changed.rows_affected() != 1 {
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "OWNER_PRESERVED",
                "Organization Owner cannot be removed through member deletion",
            ));
        }
        let scopes =
            sqlx::query_scalar::<_, Uuid>("SELECT id FROM workspaces WHERE organization_id=$1")
                .bind(organization)
                .fetch_all(&app.store.pool)
                .await?;
        app.store
            .audit_organization(
                organization,
                principal.user,
                "membership.removed",
                Some(user),
            )
            .await?;
        for workspace in scopes {
            app.runtime
                .invalidate(
                    Scope {
                        organization,
                        workspace,
                    },
                    Some(user),
                    None,
                )
                .await;
        }
    }
    Ok(data(Value::Null))
}
pub async fn connections(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    Query(page): Query<Pagination>,
) -> Result<Json<Value>> {
    let scope = scope(&path)?;
    app.store.role(scope, principal.user).await?;
    let offset = page.offset()?;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM connections WHERE organization_id=$1 AND workspace_id=$2",
    )
    .bind(scope.organization)
    .bind(scope.workspace)
    .fetch_one(&app.store.pool)
    .await?;
    let rows=sqlx::query_scalar::<_,Value>("SELECT profile FROM connections WHERE organization_id=$1 AND workspace_id=$2 ORDER BY id LIMIT $3 OFFSET $4").bind(scope.organization).bind(scope.workspace).bind(page.per_page).bind(offset).fetch_all(&app.store.pool).await?;
    Ok(Json(page.envelope(json!(rows), total)))
}
pub async fn save_connection(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(mut body): Json<ConnectionWrite>,
) -> Result<(StatusCode, Json<Value>)> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    if !app.store.role(scope, principal.user).await?.manage() {
        return Err(ApiError::denied());
    }
    app.config.network.validate(&body.profile)?;
    if let Some(id) = path.get("connection_id")
        && Uuid::parse_str(id).ok() != Some(body.profile.id)
    {
        return Err(ApiError::invalid());
    }
    let mut tx = app.store.pool.begin().await?;
    sqlx::query("SELECT id FROM workspaces WHERE organization_id=$1 AND id=$2 FOR UPDATE")
        .bind(scope.organization)
        .bind(scope.workspace)
        .fetch_one(&mut *tx)
        .await?;
    let old=sqlx::query("SELECT revision,secret,profile FROM connections WHERE organization_id=$1 AND workspace_id=$2 AND id=$3 FOR UPDATE").bind(scope.organization).bind(scope.workspace).bind(body.profile.id).fetch_optional(&mut *tx).await?;
    let revision = if let Some(ref old) = old {
        if !path.contains_key("connection_id")
            || old.try_get::<i64, _>("revision")? != i64::from(body.profile.revision)
        {
            return Err(ApiError::conflict());
        }
        body.profile
            .revision
            .checked_add(1)
            .ok_or_else(ApiError::conflict)?
    } else {
        if path.contains_key("connection_id") || body.profile.revision != 0 {
            return Err(ApiError::conflict());
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM connections WHERE organization_id=$1 AND workspace_id=$2",
        )
        .bind(scope.organization)
        .bind(scope.workspace)
        .fetch_one(&mut *tx)
        .await?;
        if count >= 100 {
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "CONNECTION_CAPACITY",
                "A workspace allows 100 connections",
            ));
        }
        1
    };
    let secret = match body.password.take() {
        Some(password) => {
            let password = zeroize::Zeroizing::new(password);
            if password.len() > 65536 {
                return Err(ApiError::invalid());
            }
            app.store.keys.seal(
                &scope.context(body.profile.id, "credential"),
                password.as_bytes(),
            )?
        }
        None => old
            .as_ref()
            .ok_or_else(ApiError::invalid)?
            .try_get::<Vec<u8>, _>("secret")?,
    };
    body.profile.revision = revision;
    let value = serde_json::to_value(&body.profile).map_err(|_| ApiError::invalid())?;
    sqlx::query("INSERT INTO connections(organization_id,workspace_id,id,revision,profile,secret) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(organization_id,workspace_id,id) DO UPDATE SET revision=excluded.revision,profile=excluded.profile,secret=excluded.secret").bind(scope.organization).bind(scope.workspace).bind(body.profile.id).bind(i64::from(revision)).bind(value).bind(secret).execute(&mut *tx).await?;
    if !body.profile.history_enabled {
        sqlx::query("DELETE FROM documents WHERE organization_id=$1 AND workspace_id=$2 AND connection_id=$3 AND kind='history'").bind(scope.organization).bind(scope.workspace).bind(body.profile.id).execute(&mut *tx).await?;
    }
    if !body.profile.restore_sql {
        sqlx::query("DELETE FROM documents WHERE organization_id=$1 AND workspace_id=$2 AND connection_id=$3 AND kind='editor'").bind(scope.organization).bind(scope.workspace).bind(body.profile.id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    app.store
        .audit(
            scope,
            principal.user,
            "connection.saved",
            Some(body.profile.id),
            "Completed",
        )
        .await?;
    Ok((
        if old.is_none() {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        data(body.profile),
    ))
}
pub async fn delete_connection(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    let id = id(&path, "connection_id")?;
    if !app.store.role(scope, principal.user).await?.manage() {
        return Err(ApiError::denied());
    }
    app.store.profile(scope, id).await?;
    app.runtime.invalidate(scope, None, Some(id)).await;
    sqlx::query("DELETE FROM connections WHERE organization_id=$1 AND workspace_id=$2 AND id=$3")
        .bind(scope.organization)
        .bind(scope.workspace)
        .bind(id)
        .execute(&app.store.pool)
        .await?;
    app.store
        .audit(
            scope,
            principal.user,
            "connection.deleted",
            Some(id),
            "Completed",
        )
        .await?;
    Ok(data(Value::Null))
}
pub async fn test_connection(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(body): Json<ConnectionWrite>,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    if !app.store.role(scope, principal.user).await?.manage() {
        return Err(ApiError::denied());
    }
    let password = match body.password {
        Some(password) => password,
        None => app
            .store
            .password(scope, body.profile.id)
            .await?
            .to_string(),
    };
    app.runtime.test(body.profile, password).await?;
    Ok(data(Value::Null))
}
pub async fn metadata(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(body): Json<db_types::MetadataRequest>,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    app.store.role(scope, principal.user).await?;
    Ok(data(
        app.runtime
            .metadata(scope, id(&path, "connection_id")?, body)
            .await?,
    ))
}
pub async fn prepare(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(body): Json<Execute>,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    execute_role(&app, scope, &principal).await?;
    let profile = app
        .store
        .profile(scope, id(&path, "connection_id")?)
        .await?;
    let prepared = db_core::prepare_script(profile.engine, &body.sql)?;
    Ok(data(prepared.iter().map(|statement|json!({"sql":statement.sql,"start":statement.start,"end":statement.end})).collect::<Vec<_>>()))
}
async fn execute_role(app: &App, scope: Scope, principal: &Principal) -> Result<()> {
    if !app.store.role(scope, principal.user).await?.execute() {
        return Err(ApiError::denied());
    }
    Ok(())
}
pub async fn connect(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<Value>)> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    execute_role(&app, scope, &principal).await?;
    Ok((
        StatusCode::CREATED,
        data(
            app.runtime
                .connect(scope, principal, id(&path, "connection_id")?)
                .await?,
        ),
    ))
}
pub async fn session(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
) -> Result<Json<Value>> {
    let scope = scope(&path)?;
    execute_role(&app, scope, &principal).await?;
    Ok(data(
        app.runtime
            .session(scope, &principal, id(&path, "session_id")?)
            .await?,
    ))
}
pub async fn lease(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    execute_role(&app, scope, &principal).await?;
    app.runtime
        .renew(scope, &principal, id(&path, "session_id")?)?;
    Ok(data(Value::Null))
}
pub async fn disconnect(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    Query(query): Query<Force>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    execute_role(&app, scope, &principal).await?;
    app.runtime
        .disconnect(scope, &principal, id(&path, "session_id")?, query.force)
        .await?;
    Ok(data(Value::Null))
}
pub async fn auto_commit(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(body): Json<AutoCommit>,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    execute_role(&app, scope, &principal).await?;
    Ok(data(
        app.runtime
            .auto_commit(scope, &principal, id(&path, "session_id")?, body.enabled)
            .await?,
    ))
}
async fn admission(
    app: &Arc<App>,
    scope: Scope,
    principal: Principal,
    session: Uuid,
    headers: &HeaderMap,
    sql: String,
    parameters: Vec<String>,
) -> Result<(StatusCode, Json<Value>)> {
    if principal.tab.is_none() {
        return Err(ApiError::invalid());
    }
    let key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or_else(ApiError::invalid)?;
    let request = serde_json::to_vec(
        &json!({"session_id":session,"sql":sql,"parameters":parameters,"tab_hash":principal.tab}),
    )
    .map_err(|_| ApiError::invalid())?;
    let context = scope.context(principal.user, "idempotency");
    let mut tx = app.store.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "{}:{}:{}:{key}",
            scope.organization, scope.workspace, principal.user
        ))
        .execute(&mut *tx)
        .await?;
    if let Some(row)=sqlx::query("SELECT id,request_hash FROM executions WHERE organization_id=$1 AND workspace_id=$2 AND user_id=$3 AND idempotency_key=$4").bind(scope.organization).bind(scope.workspace).bind(principal.user).bind(key).fetch_optional(&mut *tx).await? {
        let fingerprint:Vec<u8>=row.try_get("request_hash")?;
        if !app.store.keys.matches_fingerprint(&context,&request,&fingerprint)? {return Err(ApiError::new(axum::http::StatusCode::CONFLICT,"IDEMPOTENCY_CONFLICT","This key belongs to another request"));}
        return Ok((StatusCode::OK,data(json!({"execution_id":row.try_get::<Uuid,_>("id")?}))));
    }
    let info = app.runtime.session(scope, &principal, session).await?;
    db_core::prepare_script(
        app.runtime
            .session_profile(scope, &principal, session)?
            .engine,
        &sql,
    )?;
    let id = Uuid::new_v4();
    let fingerprint = app.store.keys.fingerprint(&context, &request)?;
    sqlx::query("INSERT INTO executions(organization_id,workspace_id,connection_id,id,user_id,session_id,login_hash,tab_hash,idempotency_key,request_hash) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(scope.organization).bind(scope.workspace).bind(info.connection_id).bind(id).bind(principal.user).bind(session).bind(&principal.login).bind(principal.tab.as_ref().ok_or_else(ApiError::invalid)?).bind(key).bind(fingerprint).execute(&mut *tx).await?;
    tx.commit().await?;
    // Commit the no-replay record before executing any customer statement.
    if let Err(error) = app
        .runtime
        .start(scope, principal.clone(), session, id, sql, parameters)
        .await
    {
        sqlx::query("UPDATE executions SET state='Failed',outcome='NotExecuted',finished_at=now() WHERE organization_id=$1 AND workspace_id=$2 AND user_id=$3 AND id=$4").bind(scope.organization).bind(scope.workspace).bind(principal.user).bind(id).execute(&app.store.pool).await?;
        return Err(error);
    }
    app.store
        .audit(
            scope,
            principal.user,
            "execution.created",
            Some(id),
            "Created",
        )
        .await?;
    Ok((StatusCode::CREATED, data(json!({"execution_id":id}))))
}
pub async fn execute(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(body): Json<Execute>,
) -> Result<(StatusCode, Json<Value>)> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    execute_role(&app, scope, &principal).await?;
    admission(
        &app,
        scope,
        principal,
        id(&path, "session_id")?,
        &headers,
        body.sql,
        Vec::new(),
    )
    .await
}
pub async fn table(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(body): Json<TableRequest>,
) -> Result<(StatusCode, Json<Value>)> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    execute_role(&app, scope, &principal).await?;
    let session = id(&path, "session_id")?;
    let profile = app.runtime.session_profile(scope, &principal, session)?;
    let (sql, parameters) = query_runtime::table_query(&profile, &body)?;
    admission(&app, scope, principal, session, &headers, sql, parameters).await
}
pub async fn execution(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
) -> Result<Json<Value>> {
    let scope = scope(&path)?;
    execute_role(&app, scope, &principal).await?;
    let id = id(&path, "execution_id")?;
    match app.runtime.snapshot(scope, &principal, id) {
        Ok(info) => Ok(data(info)),
        Err(error) if error.status == axum::http::StatusCode::NOT_FOUND => {
            let row=sqlx::query("SELECT connection_id,session_id,state,outcome FROM executions WHERE organization_id=$1 AND workspace_id=$2 AND id=$3 AND user_id=$4 AND login_hash=$5 AND tab_hash=$6").bind(scope.organization).bind(scope.workspace).bind(id).bind(principal.user).bind(principal.login).bind(principal.tab.as_ref().ok_or_else(ApiError::invalid)?).fetch_optional(&app.store.pool).await?.ok_or_else(ApiError::missing)?;
            Ok(data(
                json!({"id":id,"connection_id":row.try_get::<Uuid,_>("connection_id")?,"session_id":row.try_get::<Uuid,_>("session_id")?,"status":"Failed","results":[],"error":{"code":"RESULT_UNAVAILABLE","category":"connection","message":"Live results are unavailable; SQL was not replayed","database_code":null},"started_at_ms":0,"finished_at_ms":query_runtime::now_ms(),"persistence_error":null,"outcome":row.try_get::<String,_>("outcome")?,"recorded_state":row.try_get::<String,_>("state")?}),
            ))
        }
        Err(error) => Err(error),
    }
}
pub async fn cancel(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    execute_role(&app, scope, &principal).await?;
    app.runtime
        .cancel(scope, &principal, id(&path, "execution_id")?)
        .await?;
    Ok(data(Value::Null))
}
pub async fn close_execution(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    execute_role(&app, scope, &principal).await?;
    app.runtime
        .close(scope, &principal, id(&path, "execution_id")?)?;
    Ok(data(Value::Null))
}
pub async fn transform(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(body): Json<Transform>,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    execute_role(&app, scope, &principal).await?;
    Ok(data(app.runtime.transform(
        scope,
        &principal,
        id(&path, "execution_id")?,
        id(&path, "result_id")?,
        &body,
    )?))
}
pub async fn layout(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
) -> Result<Json<Value>> {
    let scope = scope(&path)?;
    app.store.role(scope, principal.user).await?;
    let layout: Option<Value> = sqlx::query_scalar(
        "SELECT layout FROM settings WHERE organization_id=$1 AND workspace_id=$2 AND user_id=$3",
    )
    .bind(scope.organization)
    .bind(scope.workspace)
    .bind(principal.user)
    .fetch_optional(&app.store.pool)
    .await?;
    Ok(data(layout.unwrap_or(json!({}))))
}
pub async fn save_layout(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    let scope = scope(&path)?;
    app.store.role(scope, principal.user).await?;
    let layout = json!({"sidebar_width":body["sidebar_width"].as_u64().unwrap_or(248).clamp(180,600),"editor_height":body["editor_height"].as_u64().unwrap_or(330).clamp(150,800)});
    sqlx::query("INSERT INTO settings VALUES($1,$2,$3,$4) ON CONFLICT(organization_id,workspace_id,user_id) DO UPDATE SET layout=excluded.layout").bind(scope.organization).bind(scope.workspace).bind(principal.user).bind(layout).execute(&app.store.pool).await?;
    Ok(data(Value::Null))
}
pub async fn audit(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    Query(page): Query<Pagination>,
) -> Result<Json<Value>> {
    let scope = scope(&path)?;
    if !app.store.role(scope, principal.user).await?.manage() {
        return Err(ApiError::denied());
    }
    let offset = page.offset()?;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE organization_id=$1 AND workspace_id=$2",
    )
    .bind(scope.organization)
    .bind(scope.workspace)
    .fetch_one(&app.store.pool)
    .await?;
    let rows=sqlx::query("SELECT id,user_id,action,resource_id,outcome,created_at FROM audit_events WHERE organization_id=$1 AND workspace_id=$2 ORDER BY created_at DESC,id DESC LIMIT $3 OFFSET $4").bind(scope.organization).bind(scope.workspace).bind(page.per_page).bind(offset).fetch_all(&app.store.pool).await?;
    let mut values = Vec::new();
    for row in rows {
        values.push(json!({"id":row.try_get::<Uuid,_>("id")?,"user_id":row.try_get::<Uuid,_>("user_id")?,"action":row.try_get::<String,_>("action")?,"resource_id":row.try_get::<Option<Uuid>,_>("resource_id")?,"outcome":row.try_get::<String,_>("outcome")?,"created_at":row.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at")?}));
    }
    Ok(Json(page.envelope(json!(values), total)))
}

pub async fn workspace(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
) -> Result<Json<Value>> {
    let scope = scope(&path)?;
    let role = app.store.role(scope, principal.user).await?;
    let name: String =
        sqlx::query_scalar("SELECT name FROM workspaces WHERE organization_id=$1 AND id=$2")
            .bind(scope.organization)
            .bind(scope.workspace)
            .fetch_one(&app.store.pool)
            .await?;
    Ok(data(
        json!({"id":scope.workspace,"name":name,"role":role,"version":1}),
    ))
}
pub async fn organization_audit(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    Query(page): Query<Pagination>,
) -> Result<Json<Value>> {
    let organization = id(&path, "organization_id")?;
    app.store
        .organization_owner(organization, principal.user)
        .await?;
    let offset = page.offset()?;
    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE organization_id=$1")
            .bind(organization)
            .fetch_one(&app.store.pool)
            .await?;
    let rows=sqlx::query("SELECT id,workspace_id,user_id,action,resource_id,outcome,created_at FROM audit_events WHERE organization_id=$1 ORDER BY created_at DESC,id DESC LIMIT $2 OFFSET $3").bind(organization).bind(page.per_page).bind(offset).fetch_all(&app.store.pool).await?;
    let mut values = Vec::new();
    for row in rows {
        values.push(json!({"id":row.try_get::<Uuid,_>("id")?,"workspace_id":row.try_get::<Option<Uuid>,_>("workspace_id")?,"user_id":row.try_get::<Uuid,_>("user_id")?,"action":row.try_get::<String,_>("action")?,"resource_id":row.try_get::<Option<Uuid>,_>("resource_id")?,"outcome":row.try_get::<String,_>("outcome")?,"created_at":row.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at")?}));
    }
    Ok(Json(page.envelope(json!(values), total)))
}

pub async fn metrics(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
) -> Result<Json<Value>> {
    let scope = scope(&path)?;
    if !app.store.role(scope, principal.user).await?.manage() {
        return Err(ApiError::denied());
    }
    Ok(data(app.runtime.metrics(scope)?))
}

#[derive(Deserialize)]
pub struct ExecutionKey {
    pub idempotency_key: String,
}
pub async fn execution_key(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    Query(query): Query<ExecutionKey>,
) -> Result<Json<Value>> {
    let scope = scope(&path)?;
    execute_role(&app, scope, &principal).await?;
    if query.idempotency_key.is_empty() || query.idempotency_key.len() > 128 {
        return Err(ApiError::invalid());
    }
    let tab = principal.tab.ok_or_else(ApiError::invalid)?;
    let id:Uuid=sqlx::query_scalar("SELECT id FROM executions WHERE organization_id=$1 AND workspace_id=$2 AND user_id=$3 AND login_hash=$4 AND tab_hash=$5 AND idempotency_key=$6").bind(scope.organization).bind(scope.workspace).bind(principal.user).bind(principal.login).bind(tab).bind(query.idempotency_key).fetch_optional(&app.store.pool).await?.ok_or_else(ApiError::missing)?;
    Ok(data(json!({"execution_id":id})))
}
