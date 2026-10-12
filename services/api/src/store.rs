use crate::{
    error::{ApiError, Result},
    keys::SecretStore,
};
use query_runtime::Profile;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{
    ConnectOptions, PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{str::FromStr, sync::Arc};
use uuid::Uuid;

pub struct Store {
    pub pool: PgPool,
    pub keys: Arc<dyn SecretStore>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Owner,
    Admin,
    Developer,
    Viewer,
}
impl Role {
    pub fn manage(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }
    pub fn execute(self) -> bool {
        self != Self::Viewer
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Scope {
    pub organization: Uuid,
    pub workspace: Uuid,
}
impl Scope {
    pub fn context(&self, id: Uuid, kind: &str) -> Vec<u8> {
        format!("{}/{}/{id}/{kind}", self.organization, self.workspace).into_bytes()
    }
}
#[derive(Clone, Deserialize)]
pub struct Pagination {
    #[serde(default = "first")]
    pub page: i64,
    #[serde(default = "twenty")]
    pub per_page: i64,
}
fn first() -> i64 {
    1
}
fn twenty() -> i64 {
    20
}
impl Pagination {
    pub fn offset(&self) -> Result<i64> {
        if self.page < 1 || self.page > 1_000_000 || self.per_page < 1 || self.per_page > 100 {
            return Err(ApiError::invalid());
        }
        Ok((self.page - 1) * self.per_page)
    }
    pub fn envelope(&self, values: Value, total: i64) -> Value {
        json!({"message":"Resources retrieved","data":values,"meta":{"pagination":{"page":self.page,"per_page":self.per_page,"total":total,"total_pages":(total+self.per_page-1)/self.per_page}}})
    }
}
impl Store {
    pub async fn connect(database_url: &str, keys: Arc<dyn SecretStore>) -> Result<Self> {
        let options = PgConnectOptions::from_str(database_url)
            .map_err(|_| ApiError::unavailable())?
            .disable_statement_logging();
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect_with(options)
            .await?;
        let store = Self { pool, keys };
        let version =
            sqlx::query_scalar::<_, i32>("SELECT version FROM studio_schema WHERE singleton=true")
                .fetch_optional(&store.pool)
                .await
                .map_err(|_| {
                    ApiError::new(
                        axum::http::StatusCode::SERVICE_UNAVAILABLE,
                        "MIGRATION_REQUIRED",
                        "Run the reviewed application migration first",
                    )
                })?;
        if version != Some(2) {
            return Err(ApiError::new(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "SCHEMA_INCOMPATIBLE",
                "Application schema is incompatible",
            ));
        }
        Ok(store)
    }
    pub async fn migrate(pool: &PgPool) -> Result<()> {
        let mut tx = pool.begin().await?;
        let available:bool=sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended(current_database() || '/' || current_schema() || '/studio-api',0))").fetch_one(&mut *tx).await?;
        if !available {
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "INSTANCE_IN_USE",
                "Stop the API before migration or key rotation",
            ));
        }

        sqlx::query("SELECT pg_advisory_xact_lock(764202610)")
            .execute(&mut *tx)
            .await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS studio_schema (singleton boolean PRIMARY KEY CHECK(singleton),version integer NOT NULL)").execute(&mut *tx).await?;
        let version = sqlx::query_scalar::<_, i32>(
            "SELECT version FROM studio_schema WHERE singleton=true FOR UPDATE",
        )
        .fetch_optional(&mut *tx)
        .await?;
        match version {
            Some(1 | 2) => {}
            None => {
                sqlx::raw_sql(include_str!("../migrations/0001_web.sql"))
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("INSERT INTO studio_schema VALUES(true,1)")
                    .execute(&mut *tx)
                    .await?;
            }
            _ => {
                return Err(ApiError::new(
                    axum::http::StatusCode::CONFLICT,
                    "SCHEMA_INCOMPATIBLE",
                    "Never downgrade an application schema automatically",
                ));
            }
        }
        if version != Some(2) {
            sqlx::raw_sql(include_str!("../migrations/0002_productivity.sql"))
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE studio_schema SET version=2 WHERE singleton=true")
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
    pub async fn role(&self, scope: Scope, user: Uuid) -> Result<Role> {
        let role:Option<String>=tokio::time::timeout(std::time::Duration::from_secs(3),sqlx::query_scalar("SELECT CASE WHEN om.role='Owner' THEN 'Owner' ELSE wm.role END FROM workspaces w JOIN organization_members om ON om.organization_id=w.organization_id AND om.user_id=$3 LEFT JOIN workspace_members wm ON wm.organization_id=w.organization_id AND wm.workspace_id=w.id AND wm.user_id=om.user_id WHERE w.organization_id=$1 AND w.id=$2 AND (om.role='Owner' OR wm.role IS NOT NULL)")
            .bind(scope.organization).bind(scope.workspace).bind(user).fetch_optional(&self.pool)).await.map_err(|_|ApiError::unavailable())??;
        match role.as_deref() {
            Some("Owner") => Ok(Role::Owner),
            Some("Admin") => Ok(Role::Admin),
            Some("Developer") => Ok(Role::Developer),
            Some("Viewer") => Ok(Role::Viewer),
            _ => Err(ApiError::denied()),
        }
    }
    pub async fn organization_owner(&self, organization: Uuid, user: Uuid) -> Result<()> {
        let owner:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organization_members WHERE organization_id=$1 AND user_id=$2 AND role='Owner')").bind(organization).bind(user).fetch_one(&self.pool).await?;
        if !owner {
            return Err(ApiError::denied());
        }
        Ok(())
    }
    pub async fn profile(&self, scope: Scope, id: Uuid) -> Result<Profile> {
        let value:Option<Value>=sqlx::query_scalar("SELECT profile FROM connections WHERE organization_id=$1 AND workspace_id=$2 AND id=$3").bind(scope.organization).bind(scope.workspace).bind(id).fetch_optional(&self.pool).await?;
        serde_json::from_value(value.ok_or_else(ApiError::missing)?)
            .map_err(|_| ApiError::unavailable())
    }
    pub async fn password(&self, scope: Scope, id: Uuid) -> Result<zeroize::Zeroizing<String>> {
        let secret: Vec<u8> = sqlx::query_scalar(
            "SELECT secret FROM connections WHERE organization_id=$1 AND workspace_id=$2 AND id=$3",
        )
        .bind(scope.organization)
        .bind(scope.workspace)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(ApiError::missing)?;
        let bytes = self.keys.open(&scope.context(id, "credential"), &secret)?;
        Ok(zeroize::Zeroizing::new(
            std::str::from_utf8(&bytes)
                .map_err(|_| ApiError::unavailable())?
                .to_owned(),
        ))
    }
    pub async fn audit(
        &self,
        scope: Scope,
        user: Uuid,
        action: &str,
        resource: Option<Uuid>,
        outcome: &str,
    ) -> Result<()> {
        sqlx::query("INSERT INTO audit_events(organization_id,workspace_id,id,user_id,action,resource_id,outcome) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(scope.organization).bind(scope.workspace).bind(Uuid::new_v4()).bind(user).bind(action).bind(resource).bind(outcome).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn audit_organization(
        &self,
        organization: Uuid,
        user: Uuid,
        action: &str,
        resource: Option<Uuid>,
    ) -> Result<()> {
        sqlx::query("INSERT INTO audit_events(organization_id,id,user_id,action,resource_id,outcome) VALUES($1,$2,$3,$4,$5,'Completed')").bind(organization).bind(Uuid::new_v4()).bind(user).bind(action).bind(resource).execute(&self.pool).await?;
        Ok(())
    }
    /// Operator-only offline command; the Owner must already have a provider identity.
    pub async fn provision(
        pool: &PgPool,
        provider: &str,
        subject: &str,
        name: &str,
    ) -> Result<Uuid> {
        if !matches!(provider, "github" | "google" | "test")
            || subject.is_empty()
            || name.trim().is_empty()
            || name.len() > 512
        {
            return Err(ApiError::invalid());
        }
        let mut tx = pool.begin().await?;
        let owner: Uuid = sqlx::query_scalar(
            "SELECT user_id FROM provider_identities WHERE provider=$1 AND subject=$2",
        )
        .bind(provider)
        .bind(subject)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(ApiError::missing)?;
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO organizations(id,name) VALUES($1,$2)")
            .bind(id)
            .bind(name)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO organization_members VALUES($1,$2,'Owner')")
            .bind(id)
            .bind(owner)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO audit_events(organization_id,id,user_id,action,resource_id,outcome) VALUES($1,$2,$3,'organization.provisioned',$1,'Completed')").bind(id).bind(Uuid::new_v4()).bind(owner).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(id)
    }
    /// Requires stopped API and an operator backup; row locks make each rewrap atomic.
    pub async fn reencrypt(&self) -> Result<()> {
        use sqlx::Row;
        let mut tx = self.pool.begin().await?;
        let available:bool=sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended(current_database() || '/' || current_schema() || '/studio-api',0))").fetch_one(&mut *tx).await?;
        if !available {
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "INSTANCE_IN_USE",
                "Stop the API before migration or key rotation",
            ));
        }

        sqlx::query("SELECT pg_advisory_xact_lock(764202612)")
            .execute(&mut *tx)
            .await?;
        let rows = sqlx::query(
            "SELECT organization_id,workspace_id,id,secret FROM connections FOR UPDATE",
        )
        .fetch_all(&mut *tx)
        .await?;
        for row in rows {
            let scope = Scope {
                organization: row.try_get("organization_id")?,
                workspace: row.try_get("workspace_id")?,
            };
            let id: Uuid = row.try_get("id")?;
            let plain = self.keys.open(
                &scope.context(id, "credential"),
                &row.try_get::<Vec<u8>, _>("secret")?,
            )?;
            let encrypted = self.keys.seal(&scope.context(id, "credential"), &plain)?;
            sqlx::query("UPDATE connections SET secret=$4 WHERE organization_id=$1 AND workspace_id=$2 AND id=$3").bind(scope.organization).bind(scope.workspace).bind(id).bind(encrypted).execute(&mut *tx).await?;
        }
        let rows = sqlx::query(
            "SELECT organization_id,workspace_id,id,user_id,kind,content FROM documents FOR UPDATE",
        )
        .fetch_all(&mut *tx)
        .await?;
        for row in rows {
            let scope = Scope {
                organization: row.try_get("organization_id")?,
                workspace: row.try_get("workspace_id")?,
            };
            let id: Uuid = row.try_get("id")?;
            let user: Uuid = row.try_get("user_id")?;
            let kind: String = row.try_get("kind")?;
            let context = scope.context(id, &format!("document/{user}/{kind}"));
            let plain = self
                .keys
                .open(&context, &row.try_get::<Vec<u8>, _>("content")?)?;
            let encrypted = self.keys.seal(&context, &plain)?;
            sqlx::query("UPDATE documents SET content=$4 WHERE organization_id=$1 AND workspace_id=$2 AND id=$3").bind(scope.organization).bind(scope.workspace).bind(id).bind(encrypted).execute(&mut *tx).await?;
        }
        let rows = sqlx::query("SELECT token_hash,csrf_cipher FROM sessions FOR UPDATE")
            .fetch_all(&mut *tx)
            .await?;
        for row in rows {
            let context: Vec<u8> = row.try_get("token_hash")?;
            let plain = self
                .keys
                .open(&context, &row.try_get::<Vec<u8>, _>("csrf_cipher")?)?;
            sqlx::query("UPDATE sessions SET csrf_cipher=$2 WHERE token_hash=$1")
                .bind(&context)
                .bind(self.keys.seal(&context, &plain)?)
                .execute(&mut *tx)
                .await?;
        }
        // OAuth states expire in five minutes; no login may be in flight during maintenance.
        sqlx::query("DELETE FROM oauth_states")
            .execute(&mut *tx)
            .await?;
        // Old HMAC keys remain necessary for no-replay tombstones and older backups.
        tx.commit().await?;
        Ok(())
    }
    pub async fn login_identity(
        &self,
        provider: &str,
        subject: &str,
        name: &str,
        bootstrap: &str,
        organization_name: &str,
    ) -> Result<Uuid> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(764202611)")
            .execute(&mut *tx)
            .await?;
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT user_id FROM provider_identities WHERE provider=$1 AND subject=$2",
        )
        .bind(provider)
        .bind(subject)
        .fetch_optional(&mut *tx)
        .await?;
        let user = existing.unwrap_or_else(Uuid::new_v4);
        if existing.is_none() {
            let name = name.chars().take(512).collect::<String>();
            sqlx::query("INSERT INTO users(id,display_name) VALUES($1,$2)")
                .bind(user)
                .bind(if name.trim().is_empty() {
                    "User"
                } else {
                    &name
                })
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO provider_identities(provider,subject,user_id) VALUES($1,$2,$3)",
            )
            .bind(provider)
            .bind(subject)
            .bind(user)
            .execute(&mut *tx)
            .await?;
        }
        let empty: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM organizations)")
            .fetch_one(&mut *tx)
            .await?;
        if empty && bootstrap == format!("{provider}:{subject}") {
            let organization = Uuid::new_v4();
            sqlx::query("INSERT INTO organizations(id,name) VALUES($1,$2)")
                .bind(organization)
                .bind(organization_name)
                .execute(&mut *tx)
                .await?;
            sqlx::query("INSERT INTO organization_members VALUES($1,$2,'Owner')")
                .bind(organization)
                .bind(user)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(user)
    }
    pub async fn prune(&self, audit_days: i32) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM oauth_states WHERE expires_at<now()")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM sessions WHERE expires_at<now()-interval '1 day'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM audit_events WHERE created_at<now()-make_interval(days => $1)")
            .bind(audit_days)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM mutations WHERE (organization_id,workspace_id,id) IN (SELECT organization_id,workspace_id,id FROM mutations WHERE created_at<now()-interval '30 days' AND state!='Applying' LIMIT 1000)").execute(&mut *tx).await?;
        // Idempotency tombstones survive restart for 30 days; clients never retry an expired key.
        sqlx::query("DELETE FROM executions WHERE created_at<now()-interval '30 days' AND state IN ('Completed','Failed','Cancelled','Interrupted')").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM documents WHERE kind='history' AND created_at_ms < (extract(epoch from now())*1000)::bigint-2592000000").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM documents d USING (SELECT organization_id,workspace_id,id,row_number() OVER(PARTITION BY organization_id,workspace_id,connection_id ORDER BY created_at_ms DESC,id DESC) AS n FROM documents WHERE kind='history') old WHERE old.organization_id=d.organization_id AND old.workspace_id=d.workspace_id AND old.id=d.id AND old.n>1000").execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
