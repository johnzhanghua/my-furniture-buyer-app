use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::auth::hash_password;
use crate::error::ApiError;

pub const DEMO_EMAIL: &str = "buyer@example.com";
pub const DEMO_PASSWORD: &str = "password123";

/// Opens (creating if needed) the SQLite database and applies all migrations.
pub async fn init_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        // WAL keeps reads from blocking the single writer.
        .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| sqlx::Error::Migrate(Box::new(e)))?;

    Ok(pool)
}

/// Inserts the demo buyer if it is not already present. Idempotent, so it is
/// safe to run on every boot.
pub async fn seed_demo_user(pool: &SqlitePool, budget_cents: i64) -> Result<(), ApiError> {
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
        .bind(DEMO_EMAIL)
        .fetch_optional(pool)
        .await?;

    if existing.is_some() {
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO users (id, email, display_name, password_hash, budget_cents, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(DEMO_EMAIL)
    .bind("Demo Buyer")
    .bind(hash_password(DEMO_PASSWORD)?)
    .bind(budget_cents)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;

    log::info!("seeded demo buyer {DEMO_EMAIL} / {DEMO_PASSWORD}");
    Ok(())
}
