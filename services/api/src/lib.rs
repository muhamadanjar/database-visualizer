//! Tenant-authorized web application layer. Customer SQL uses the shared Rust core.
pub mod auth;
pub mod config;
pub mod documents;
mod editing;
pub mod error;
pub mod keys;
mod productivity;
pub mod routes;
pub mod runtime;
pub mod store;
mod stream;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use error::{ApiError, Result, data};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub struct Limiter {
    buckets: Mutex<HashMap<String, (Instant, u32)>>,
}
impl Default for Limiter {
    fn default() -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
        }
    }
}
impl Limiter {
    pub fn check(&self, key: String, limit: u32) -> Result<()> {
        let mut buckets = self.buckets.lock().map_err(|_| ApiError::unavailable())?;
        if buckets.len() >= 4096 {
            buckets.retain(|_, (start, _)| start.elapsed() < Duration::from_secs(60));
        }
        if buckets.len() >= 4096 && !buckets.contains_key(&key) {
            return Err(ApiError::unavailable());
        }
        let bucket = buckets.entry(key).or_insert((Instant::now(), 0));
        if bucket.0.elapsed() >= Duration::from_secs(60) {
            *bucket = (Instant::now(), 0);
        }
        bucket.1 = bucket.1.saturating_add(1);
        if bucket.1 > limit {
            return Err(ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMITED",
                "Too many requests; wait before retrying",
            ));
        }
        Ok(())
    }
}
pub struct App {
    instance_guard: tokio::sync::Mutex<Option<sqlx::pool::PoolConnection<sqlx::Postgres>>>,
    pub config: Arc<config::Config>,
    pub store: Arc<store::Store>,
    pub runtime: Arc<runtime::Runtime>,
    pub http: reqwest::Client,
    pub limiter: Limiter,
    pub socket_capacity: Arc<tokio::sync::Semaphore>,
    pub shutdown: tokio_util::sync::CancellationToken,
}
impl App {
    pub async fn open(config: config::Config) -> Result<Arc<Self>> {
        let config = Arc::new(config);
        let store =
            Arc::new(store::Store::connect(&config.database_url, config.keys.clone()).await?);
        // Database lock also rejects replicas configured with different spool directories.
        let mut instance_guard = store.pool.acquire().await?;
        let available:bool=sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended(current_database() || '/' || current_schema() || '/studio-api',0))").fetch_one(&mut *instance_guard).await?;
        if !available {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "INSTANCE_IN_USE",
                "One API replica is supported",
            ));
        }
        // Acquire the process lock before touching any persisted live-execution state.
        let runtime = runtime::Runtime::open(store.clone(), config.clone())?;
        sqlx::query("UPDATE executions SET state='Interrupted',outcome='Unknown',finished_at=now() WHERE state IN ('Created','Running','Streaming')").execute(&store.pool).await?;
        sqlx::query(
            "UPDATE mutations SET state='Unknown',finished_at=now() WHERE state='Applying'",
        )
        .execute(&store.pool)
        .await?;
        let http = reqwest::Client::builder()
            .user_agent("Database-Studio/0.1")
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| ApiError::unavailable())?;
        Ok(Arc::new(Self {
            instance_guard: tokio::sync::Mutex::new(Some(instance_guard)),
            config,
            store,
            runtime,
            http,
            limiter: Limiter::default(),
            socket_capacity: Arc::new(tokio::sync::Semaphore::new(32)),
            shutdown: tokio_util::sync::CancellationToken::new(),
        }))
    }
    pub async fn close(&self) -> Result<()> {
        self.runtime.stop_admission();
        self.shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(25), self.runtime.shutdown())
            .await
            .map_err(|_| ApiError::unavailable())??;
        if let Some(mut connection) = self.instance_guard.lock().await.take() {
            // Never return a session-level advisory lock to the application pool.
            let _ = sqlx::query("SELECT pg_advisory_unlock_all()")
                .execute(&mut *connection)
                .await;
            connection.close().await?;
        }
        self.store.pool.close().await;
        Ok(())
    }
    pub fn background(self: &Arc<Self>) {
        let app = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            let mut count = 0u32;
            loop {
                tokio::select! {_=app.shutdown.cancelled()=>break,_=tick.tick()=>{
                    app.runtime.sweep().await;count=count.wrapping_add(1);
                    if count.is_multiple_of(60)&& app.store.prune(app.config.audit_days).await.is_err(){tracing::warn!(event="retention_unavailable");}
                }}
            }
        });
    }
}
async fn envelope(State(app): State<Arc<App>>, request: Request, next: Next) -> Response {
    if app.shutdown.is_cancelled() {
        return ApiError::unavailable().into_response();
    }
    let started = Instant::now();
    let route = request
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|path| path.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".into());
    let path = request.uri().path();
    let limit = if path.contains("/auth/") { 120 } else { 2400 };
    if let Err(error) = app.limiter.check(
        if path.contains("/auth/") {
            "auth-instance".into()
        } else {
            "http-instance".into()
        },
        limit,
    ) {
        return error.into_response();
    }
    let mut response = next.run(request).await;
    tracing::info!(
        event = "http_request",
        route,
        status = response.status().as_u16(),
        duration_ms = started.elapsed().as_millis() as u64
    );
    if (response.status().is_client_error() || response.status().is_server_error())
        && response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_none_or(|value| !value.starts_with("application/json"))
    {
        let status = response.status();
        response = ApiError::new(
            status,
            "HTTP_REQUEST_FAILED",
            "Request could not be completed",
        )
        .into_response();
    }
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        "nosniff".parse().expect("static header"),
    );
    response
}
async fn live() -> Json<Value> {
    data(json!({"status":"live"}))
}
async fn ready(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    if app.shutdown.is_cancelled() {
        return Err(ApiError::unavailable());
    }
    sqlx::query("SELECT 1").execute(&app.store.pool).await?;
    Ok(data(json!({"status":"ready"})))
}
async fn openapi() -> Json<Value> {
    Json(serde_json::from_str(include_str!("../openapi.json")).expect("checked OpenAPI artifact"))
}
pub fn router(app: Arc<App>) -> Router {
    let base = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}";
    let router = Router::new()
        .route(
            &format!("{base}/connections/{{connection_id}}/edits/snapshot"),
            post(editing::snapshot),
        )
        .route(
            &format!("{base}/connections/{{connection_id}}/edits/preview"),
            post(editing::preview),
        )
        .route(
            &format!("{base}/connections/{{connection_id}}/edits/{{tab_id}}"),
            axum::routing::delete(editing::discard),
        )
        .route(
            &format!("{base}/connections/{{connection_id}}/mutations"),
            post(editing::apply).get(editing::status),
        )
        .route(
            &format!("{base}/connections/{{connection_id}}/mutations/{{mutation_id}}"),
            get(editing::status).delete(editing::cancel),
        )
        .route(
            &format!("{base}/connections/{{connection_id}}/edits/readback"),
            post(editing::readback),
        )
        .route(
            &format!("{base}/connections/{{connection_id}}/documents/search"),
            post(productivity::search),
        )
        .route(
            &format!("{base}/connections/{{connection_id}}/documents/{{document_id}}/favorite"),
            axum::routing::put(productivity::favorite),
        )
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/api/v1/openapi.json", get(openapi))
        .route("/api/v1/auth/providers", get(auth::providers))
        .route("/api/v1/auth/me", get(auth::me))
        .route("/api/v1/auth/activity", post(auth::activity))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/auth/{provider}/login", get(auth::login))
        .route("/api/v1/auth/{provider}/callback", get(auth::callback))
        .route("/api/v1/organizations", get(routes::organizations))
        .route(
            "/api/v1/organizations/{organization_id}/audit",
            get(routes::organization_audit),
        )
        .route(
            "/api/v1/organizations/{organization_id}/members",
            get(routes::members).post(routes::add_member),
        )
        .route(
            "/api/v1/organizations/{organization_id}/members/{user_id}",
            axum::routing::delete(routes::remove_member),
        )
        .route(
            "/api/v1/organizations/{organization_id}/workspaces",
            get(routes::workspaces).post(routes::create_workspace),
        )
        .route(
            base,
            get(routes::workspace)
                .patch(routes::rename_workspace)
                .delete(routes::delete_workspace),
        )
        .route(
            &format!("{base}/members"),
            get(routes::members).post(routes::add_member),
        )
        .route(
            &format!("{base}/members/{{user_id}}"),
            axum::routing::delete(routes::remove_member),
        )
        .route(
            &format!("{base}/connections"),
            get(routes::connections).post(routes::save_connection),
        )
        .route(
            &format!("{base}/connections/test"),
            post(routes::test_connection),
        )
        .route(
            &format!("{base}/connections/{{connection_id}}"),
            put(routes::save_connection).delete(routes::delete_connection),
        )
        .route(
            &format!("{base}/connections/{{connection_id}}/metadata"),
            post(routes::metadata),
        )
        .route(
            &format!("{base}/connections/{{connection_id}}/prepare"),
            post(routes::prepare),
        )
        .route(
            &format!("{base}/connections/{{connection_id}}/sessions"),
            post(routes::connect),
        )
        .route(
            &format!("{base}/sessions/{{session_id}}"),
            get(routes::session).delete(routes::disconnect),
        )
        .route(
            &format!("{base}/sessions/{{session_id}}/lease"),
            post(routes::lease),
        )
        .route(
            &format!("{base}/sessions/{{session_id}}/auto-commit"),
            put(routes::auto_commit),
        )
        .route(
            &format!("{base}/sessions/{{session_id}}/executions"),
            post(routes::execute),
        )
        .route(
            &format!("{base}/sessions/{{session_id}}/table"),
            post(routes::table),
        )
        .route(&format!("{base}/executions"), get(routes::execution_key))
        .route(
            &format!("{base}/executions/{{execution_id}}"),
            get(routes::execution).delete(routes::close_execution),
        )
        .route(
            &format!("{base}/executions/{{execution_id}}/cancel"),
            post(routes::cancel),
        )
        .route(
            &format!("{base}/executions/{{execution_id}}/stream"),
            get(stream::upgrade),
        )
        .route(&format!("{base}/stream"), get(stream::upgrade))
        .route(
            &format!("{base}/executions/{{execution_id}}/results/{{result_id}}/transform"),
            post(routes::transform),
        )
        .route(
            &format!("{base}/connections/{{connection_id}}/documents"),
            get(documents::headers)
                .post(documents::write)
                .delete(documents::delete),
        )
        .route(
            &format!("{base}/connections/{{connection_id}}/documents/{{document_id}}"),
            get(documents::read)
                .patch(documents::rename)
                .delete(documents::delete),
        )
        .route(
            &format!("{base}/layout"),
            get(routes::layout).put(routes::save_layout),
        )
        .route(&format!("{base}/audit"), get(routes::audit))
        .route(&format!("{base}/metrics"), get(routes::metrics))
        .fallback(|| async { ApiError::missing() });
    #[cfg(feature = "test-provider")]
    let router = router.route("/api/v1/auth/test/login", post(auth::test_login));
    router
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(app.clone(), envelope))
        .with_state(app)
}
