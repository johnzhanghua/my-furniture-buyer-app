use sqlx::SqlitePool;

use crate::config::Config;

/// Shared application state, handed to handlers via `web::Data<AppState>`.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Config,
}
