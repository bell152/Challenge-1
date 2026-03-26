pub mod seed;

use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use sqlx::{
    Sqlite, SqlitePool,
    migrate::MigrateDatabase,
    sqlite::SqlitePoolOptions,
};
use tracing::info;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub database_url: String,
    pub is_dev_mode: bool,
}

pub async fn prepare_database(
    config: &DatabaseConfig,
    run_dev_seed: bool,
) -> anyhow::Result<SqlitePool> {
    let created = ensure_database_exists(&config.database_url).await?;
    if created {
        info!("Database created");
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .with_context(|| {
            format!(
                "failed to connect sqlite database at {}",
                config.database_url
            )
        })?;

    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&pool)
        .await
        .context("failed to enable sqlite foreign keys")?;

    MIGRATOR
        .run(&pool)
        .await
        .context("failed to apply database migrations")?;
    info!("Migrations applied");

    if run_dev_seed && config.is_dev_mode {
        seed::run(&pool).await?;
        info!("Seed data applied");
    }

    Ok(pool)
}

async fn ensure_database_exists(database_url: &str) -> anyhow::Result<bool> {
    let Some((base_url, path)) = sqlite_url_and_path(database_url) else {
        return Ok(false);
    };

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create parent directory for sqlite database at {}",
                parent.display()
            )
        })?;
    }

    if path.exists() {
        return Ok(false);
    }

    let exists = Sqlite::database_exists(&base_url)
        .await
        .with_context(|| format!("failed to check sqlite database at {base_url}"))?;
    if !exists {
        Sqlite::create_database(&base_url)
            .await
            .with_context(|| format!("failed to create sqlite database at {base_url}"))?;
        return Ok(true);
    }

    Ok(false)
}

fn sqlite_url_and_path(database_url: &str) -> Option<(String, PathBuf)> {
    let base_url = database_url.split('?').next()?.to_string();
    let path = sqlite_path_from_url(&base_url)?;
    Some((base_url, path))
}

fn sqlite_path_from_url(database_url: &str) -> Option<PathBuf> {
    let raw = database_url
        .strip_prefix("sqlite://")
        .or_else(|| database_url.strip_prefix("sqlite:"))?;

    if raw.is_empty() || raw == ":memory:" {
        return None;
    }

    let path = Path::new(raw);
    if path.as_os_str().is_empty() {
        return None;
    }

    Some(path.to_path_buf())
}

pub fn parse_cli_command(args: &[String]) -> anyhow::Result<Option<CliCommand>> {
    match args.get(1).map(String::as_str) {
        None => Ok(None),
        Some("init-db") => Ok(Some(CliCommand::InitDb)),
        Some("seed") => Ok(Some(CliCommand::Seed)),
        Some(other) => Err(anyhow!(
            "unknown command '{other}', expected 'init-db' or 'seed'"
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliCommand {
    InitDb,
    Seed,
}
