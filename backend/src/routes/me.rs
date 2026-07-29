use actix_web::{web, HttpResponse};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::models::{BudgetResponse, User, UserResponse};
use crate::state::AppState;

pub async fn profile(state: web::Data<AppState>, user: AuthUser) -> Result<HttpResponse, ApiError> {
    let row = sqlx::query_as::<_, User>(
        "SELECT id, email, display_name, password_hash, budget_cents, created_at
         FROM users WHERE id = ?",
    )
    .bind(user.id.as_str())
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::Unauthorized("account no longer exists".into()))?;

    Ok(HttpResponse::Ok().json(UserResponse::from(row)))
}

pub async fn budget(state: web::Data<AppState>, user: AuthUser) -> Result<HttpResponse, ApiError> {
    let budget_cents: i64 = sqlx::query_scalar("SELECT budget_cents FROM users WHERE id = ?")
        .bind(user.id.as_str())
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::Unauthorized("account no longer exists".into()))?;

    let spent_cents: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(total_cents), 0) FROM orders
         WHERE user_id = ? AND status <> 'cancelled'",
    )
    .bind(user.id.as_str())
    .fetch_one(&state.pool)
    .await?;

    Ok(HttpResponse::Ok().json(BudgetResponse {
        budget_cents,
        spent_cents,
        remaining_cents: budget_cents - spent_cents,
    }))
}
