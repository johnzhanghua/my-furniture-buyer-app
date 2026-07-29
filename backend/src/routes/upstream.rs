use actix_web::{web, HttpResponse};
use serde::Serialize;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::external_api::to_cents;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct UpstreamStatus {
    pub configured: bool,
    pub base_url: String,
    /// Echoed so it is obvious which participant identity is in play. The API
    /// key is never included.
    pub user_id: String,
    pub key_header: String,
    pub reachable: bool,
    pub balance_cents: Option<i64>,
    pub detail: String,
}

/// Diagnostic: confirms the upstream credentials work, by doing the cheapest
/// authenticated call there is — reading the balance.
///
/// Authenticated, because it discloses the configured base URL and participant
/// id. A hackathon debugging aid, not a public health check.
pub async fn status(state: web::Data<AppState>, _user: AuthUser) -> Result<HttpResponse, ApiError> {
    let cfg = &state.config.external_api;

    if !state.external_api.is_configured() {
        return Ok(HttpResponse::Ok().json(UpstreamStatus {
            configured: false,
            base_url: cfg.base_url.clone(),
            user_id: cfg.user_id.clone(),
            key_header: cfg.key_header.clone(),
            reachable: false,
            balance_cents: None,
            detail: "set EXTERNAL_API_BASE_URL, EXTERNAL_API_USER_ID and EXTERNAL_API_KEY in backend/.env".into(),
        }));
    }

    // Reported in the body rather than propagated: the point of this endpoint
    // is to say *how* upstream is broken.
    let (reachable, balance_cents, detail) = match state.external_api.balance().await {
        Ok(user) => (
            true,
            Some(to_cents(user.balance)),
            format!("authenticated as {}", user.name),
        ),
        Err(e) => (false, None, e.to_string()),
    };

    Ok(HttpResponse::Ok().json(UpstreamStatus {
        configured: true,
        base_url: cfg.base_url.clone(),
        user_id: cfg.user_id.clone(),
        key_header: cfg.key_header.clone(),
        reachable,
        balance_cents,
        detail,
    }))
}
