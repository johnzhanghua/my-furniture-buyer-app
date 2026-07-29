use actix_web::{web, HttpResponse};

use crate::agent::AskRequest;
use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

/// `POST /api/assistant/ask` — plain-English request in, recommendation out.
///
/// Authenticated: the assistant reads the shop balance and (when the caller
/// opts in) can spend money, so it is not open to anonymous callers.
pub async fn ask(
    state: web::Data<AppState>,
    _user: AuthUser,
    body: web::Json<AskRequest>,
) -> Result<HttpResponse, ApiError> {
    let answer = state
        .agent
        .ask(&state.external_api, body.into_inner())
        .await?;

    Ok(HttpResponse::Ok().json(answer))
}
