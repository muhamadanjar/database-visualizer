use sqlx::ConnectOptions;
use std::str::FromStr;
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().json().with_target(false).init();
    if let Err(error) = run().await {
        tracing::error!(code = error.code, message = error.message);
        std::process::exit(1);
    }
}
async fn run() -> studio_api::error::Result<()> {
    let command = std::env::args().nth(1);
    if matches!(
        command.as_deref(),
        Some("migrate" | "provision-organization" | "rotate-key")
    ) {
        let url = zeroize::Zeroizing::new(
            std::env::var("STUDIO_APPLICATION_DATABASE_URL")
                .map_err(|_| studio_api::error::ApiError::unavailable())?,
        );
        let options = sqlx::postgres::PgConnectOptions::from_str(&url)
            .map_err(|_| studio_api::error::ApiError::invalid())?
            .disable_statement_logging();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        match command.as_deref() {
            Some("migrate") => {
                studio_api::store::Store::migrate(&pool).await?;
                tracing::info!(event = "migration_completed", version = 2);
            }
            Some("provision-organization") => {
                let owner = std::env::var("STUDIO_PROVISION_OWNER")
                    .map_err(|_| studio_api::error::ApiError::invalid())?;
                let (provider, subject) = owner
                    .split_once(':')
                    .ok_or_else(studio_api::error::ApiError::invalid)?;
                let name = std::env::var("STUDIO_PROVISION_NAME")
                    .map_err(|_| studio_api::error::ApiError::invalid())?;
                let id =
                    studio_api::store::Store::provision(&pool, provider, subject, &name).await?;
                tracing::info!(event="organization_provisioned",organization_id=%id);
            }
            Some("rotate-key") => {
                let path = std::env::var("STUDIO_KEY_FILE")
                    .map_err(|_| studio_api::error::ApiError::invalid())?;
                let keys = std::sync::Arc::new(studio_api::keys::KeyRing::load(
                    std::path::Path::new(&path),
                )?);
                let store = studio_api::store::Store::connect(&url, keys).await?;
                store.reencrypt().await?;
                store.pool.close().await;
                tracing::info!(event = "key_rotation_completed");
            }
            _ => unreachable!(),
        }
        pool.close().await;
        return Ok(());
    }
    let config = studio_api::config::Config::load().await?;
    let bind = config.bind;
    let app = studio_api::App::open(config).await?;
    app.background();
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|_| studio_api::error::ApiError::unavailable())?;
    tracing::info!(event="api_ready",address=%bind);
    let stopping = app.clone();
    let server =
        axum::serve(listener, studio_api::router(app.clone())).with_graceful_shutdown(async move {
            #[cfg(unix)]
            {
                let mut terminate =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("SIGTERM registration");
                tokio::select! { _=tokio::signal::ctrl_c()=>{}, _=terminate.recv()=>{} }
            }
            #[cfg(not(unix))]
            {
                let _ = tokio::signal::ctrl_c().await;
            }
            stopping.runtime.stop_admission();
            stopping.shutdown.cancel();
            if tokio::time::timeout(
                std::time::Duration::from_secs(25),
                stopping.runtime.shutdown(),
            )
            .await
            .is_err()
            {
                tracing::warn!(event = "shutdown_cleanup_timeout");
            }
        });
    server
        .await
        .map_err(|_| studio_api::error::ApiError::unavailable())?;
    app.close().await?;
    Ok(())
}
