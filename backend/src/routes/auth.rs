use actix_web::{web, HttpResponse};

use crate::auth::{hash_password, issue_token, verify_password};
use crate::error::ApiError;
use crate::models::{AuthResponse, LoginRequest, RegisterRequest, User, UserResponse};
use crate::state::AppState;

const USER_COLUMNS: &str = "id, email, display_name, password_hash, created_at";

pub async fn register(
    state: web::Data<AppState>,
    body: web::Json<RegisterRequest>,
) -> Result<HttpResponse, ApiError> {
    let body = body.into_inner();
    let email = body.email.trim().to_lowercase();

    if !email.contains('@') || email.len() < 3 {
        return Err(ApiError::BadRequest(
            "a valid email address is required".into(),
        ));
    }
    if body.password.len() < 8 {
        return Err(ApiError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }

    let taken: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
        .bind(email.as_str())
        .fetch_optional(&state.pool)
        .await?;
    if taken.is_some() {
        return Err(ApiError::Conflict(
            "an account with that email already exists".into(),
        ));
    }

    let display_name = body
        .display_name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| email.split('@').next().unwrap_or("Buyer").to_string());

    let id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();

    // No budget column is written: spending power comes from the furniture
    // shop's ledger, not from us.
    sqlx::query(
        "INSERT INTO users (id, email, display_name, password_hash, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id.as_str())
    .bind(email.as_str())
    .bind(display_name.as_str())
    .bind(hash_password(&body.password)?)
    .bind(created_at.as_str())
    .execute(&state.pool)
    .await?;

    let token = issue_token(
        &id,
        &email,
        &state.config.jwt_secret,
        state.config.jwt_ttl_hours,
    )?;

    Ok(HttpResponse::Created().json(AuthResponse {
        token,
        user: UserResponse {
            id,
            email,
            display_name,
            created_at,
        },
    }))
}

pub async fn login(
    state: web::Data<AppState>,
    body: web::Json<LoginRequest>,
) -> Result<HttpResponse, ApiError> {
    let body = body.into_inner();
    let email = body.email.trim().to_lowercase();

    let user =
        sqlx::query_as::<_, User>(&format!("SELECT {USER_COLUMNS} FROM users WHERE email = ?"))
            .bind(email.as_str())
            .fetch_optional(&state.pool)
            .await?
            // Same error for "no such user" and "wrong password" so the endpoint does
            // not leak which emails are registered.
            .ok_or(ApiError::InvalidCredentials)?;

    if !verify_password(&body.password, &user.password_hash)? {
        return Err(ApiError::InvalidCredentials);
    }

    let token = issue_token(
        &user.id,
        &user.email,
        &state.config.jwt_secret,
        state.config.jwt_ttl_hours,
    )?;

    Ok(HttpResponse::Ok().json(AuthResponse {
        token,
        user: user.into(),
    }))
}
