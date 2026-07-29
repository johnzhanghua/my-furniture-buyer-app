use sqlx::SqlitePool;

use crate::config::Config;
use crate::external_api::ExternalApiClient;

/// Shared application state, handed to handlers via `web::Data<AppState>`.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Config,
    pub external_api: ExternalApiClient,
}
