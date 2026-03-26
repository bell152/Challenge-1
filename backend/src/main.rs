mod access;
mod auth;
mod audit;
mod db;
mod error;
mod passkey;
mod portal;
mod rate_limit;
mod routes;

use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use axum::{Json, extract::State, http::StatusCode};
use auth::OtpChallengeEntry;
use db::{CliCommand, DatabaseConfig, parse_cli_command, prepare_database};
use moka::future::Cache;
use passkey::PasskeyChallengeEntry;
use rate_limit::RateLimitEntry;
use serde::Serialize;
use sqlx::SqlitePool;
use time::OffsetDateTime;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    db: SqlitePool,
    bootstrap_cache: Cache<String, String>,
    otp_cache: Cache<String, OtpChallengeEntry>,
    passkey_cache: Cache<String, PasskeyChallengeEntry>,
    rate_limit_cache: Cache<String, RateLimitEntry>,
    jwt_secret: String,
    is_dev_mode: bool,
}

pub type SharedState = Arc<AppState>;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    database: &'static str,
    cache: &'static str,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let config = load_database_config();
    match parse_cli_command(&std::env::args().collect::<Vec<_>>())? {
        Some(CliCommand::InitDb) => {
            prepare_database(&config, true).await?;
            info!("database initialization completed");
            return Ok(());
        }
        Some(CliCommand::Seed) => {
            let db = prepare_database(&config, false).await?;
            db::seed::run(&db).await?;
            info!("seed completed");
            return Ok(());
        }
        None => {}
    }

    let state = Arc::new(build_state(&config).await?);
    let app = routes::build_router(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());
    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));

    info!("backend listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("failed to bind backend listener")?;

    axum::serve(listener, app)
        .await
        .context("backend server exited unexpectedly")?;

    Ok(())
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "multi_subject_auth_backend=debug,tower_http=info".to_string()),
        )
        .init();
}

fn load_database_config() -> DatabaseConfig {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://data/app.db".to_string());
    let app_env = std::env::var("APP_ENV").unwrap_or_else(|_| "development".to_string());
    let is_dev_mode = !matches!(app_env.to_ascii_lowercase().as_str(), "production" | "prod");

    DatabaseConfig {
        database_url,
        is_dev_mode,
    }
}

async fn build_state(config: &DatabaseConfig) -> anyhow::Result<AppState> {
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "dev-stage-2-secret-change-me".to_string());
    let db = prepare_database(config, true).await?;
    sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&db)
        .await
        .context("failed to ping sqlite connection")?;

    let bootstrap_cache = Cache::builder().max_capacity(1_000).build();
    let otp_cache = Cache::builder().max_capacity(10_000).build();
    let passkey_cache = Cache::builder().max_capacity(10_000).build();
    let rate_limit_cache = Cache::builder().max_capacity(10_000).build();
    bootstrap_cache
        .insert("service".to_string(), "multi-subject-auth-backend".to_string())
        .await;
    bootstrap_cache
        .insert(
            "booted_at".to_string(),
            OffsetDateTime::now_utc().unix_timestamp().to_string(),
        )
        .await;

    Ok(AppState {
        db,
        bootstrap_cache,
        otp_cache,
        passkey_cache,
        rate_limit_cache,
        jwt_secret,
        is_dev_mode: config.is_dev_mode,
    })
}

async fn health(State(state): State<SharedState>) -> (StatusCode, Json<HealthResponse>) {
    let database_ok = sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.db)
        .await
        .is_ok();

    let cache_ok = state.bootstrap_cache.get("service").await.is_some();

    let response = HealthResponse {
        status: "ok",
        service: "multi-subject-auth-backend",
        database: if database_ok { "connected" } else { "unavailable" },
        cache: if cache_ok { "ready" } else { "unavailable" },
    };

    (StatusCode::OK, Json(response))
}
