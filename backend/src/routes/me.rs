use actix_web::{web, HttpResponse};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::external_api::to_cents;
use crate::models::{BalanceResponse, User, UserResponse};
use crate::state::AppState;

/// The local login account. Identity is ours; money is not — see [`balance`].
pub async fn profile(state: web::Data<AppState>, user: AuthUser) -> Result<HttpResponse, ApiError> {
    let row = sqlx::query_as::<_, User>(
        "SELECT id, email, display_name, password_hash, created_at
         FROM users WHERE id = ?",
    )
    .bind(user.id.as_str())
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::Unauthorized("account no longer exists".into()))?;

    Ok(HttpResponse::Ok().json(UserResponse::from(row)))
}

/// The real balance, read from the furniture shop's ledger.
///
/// Note this is the *participant's* balance, shared by every local login —
/// there is one upstream account and one API key.
pub async fn balance(
    state: web::Data<AppState>,
    _user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    let upstream = state.external_api.balance().await?;

    Ok(HttpResponse::Ok().json(BalanceResponse {
        user_id: upstream.user_id,
        name: upstream.name,
        balance_cents: to_cents(upstream.balance),
    }))
}
