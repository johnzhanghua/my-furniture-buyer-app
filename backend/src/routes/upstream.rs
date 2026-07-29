use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ProbeQuery {
    /// Path to probe, relative to `EXTERNAL_API_BASE_URL`. Defaults to the
    /// base URL itself.
    pub path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpstreamStatus {
    pub configured: bool,
    pub base_url: String,
    /// Echoed so it is obvious which participant identity is in play; the API
    /// key is never included.
    pub user_id: String,
    pub key_header: String,
    pub user_header: String,
    pub probed_path: Option<String>,
    pub status_code: Option<u16>,
    pub detail: String,
}

/// Diagnostic: confirms the upstream credentials are loaded and, if a base URL
/// is set, reports what a credentialed GET actually returns.
///
/// Authenticated, because it discloses the configured base URL and participant
/// id. Intended as a hackathon debugging aid, not a public health check.
pub async fn status(
    state: web::Data<AppState>,
    _user: AuthUser,
    query: web::Query<ProbeQuery>,
) -> Result<HttpResponse, ApiError> {
    let cfg = &state.config.external_api;

    if !state.external_api.is_configured() {
        return Ok(HttpResponse::Ok().json(UpstreamStatus {
            configured: false,
            base_url: cfg.base_url.clone(),
            user_id: cfg.user_id.clone(),
            key_header: cfg.key_header.clone(),
            user_header: cfg.user_header.clone(),
            probed_path: None,
            status_code: None,
            detail: "set EXTERNAL_API_BASE_URL and EXTERNAL_API_KEY in backend/.env".into(),
        }));
    }

    let path = query.into_inner().path.unwrap_or_default();

    // A transport failure is reported in the body rather than propagated: the
    // point of this endpoint is to say *how* upstream is broken.
    let (status_code, detail) = match state.external_api.probe(&path).await {
        Ok(code) => (Some(code), format!("upstream responded with {code}")),
        Err(e) => (None, e.to_string()),
    };

    Ok(HttpResponse::Ok().json(UpstreamStatus {
        configured: true,
        base_url: cfg.base_url.clone(),
        user_id: cfg.user_id.clone(),
        key_header: cfg.key_header.clone(),
        user_header: cfg.user_header.clone(),
        probed_path: Some(path),
        status_code,
        detail,
    }))
}
