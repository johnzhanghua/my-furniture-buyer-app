use std::future::{ready, Ready};

use actix_web::{dev::Payload, web, FromRequest, HttpRequest};
use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Password hashing (Argon2id, default params)
// ---------------------------------------------------------------------------

pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::Internal(format!("failed to hash password: {e}")))
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, ApiError> {
    let parsed = PasswordHash::new(hash)
        .map_err(|e| ApiError::Internal(format!("stored password hash is malformed: {e}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

// ---------------------------------------------------------------------------
// JWT
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// User id.
    pub sub: String,
    pub email: String,
    /// Expiry, seconds since the Unix epoch.
    pub exp: i64,
}

pub fn issue_token(
    user_id: &str,
    email: &str,
    secret: &str,
    ttl_hours: i64,
) -> Result<String, ApiError> {
    let exp = (chrono::Utc::now() + chrono::Duration::hours(ttl_hours)).timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        exp,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(format!("failed to sign token: {e}")))
}

pub fn decode_token(token: &str, secret: &str) -> Result<Claims, ApiError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map(|data| data.claims)
    .map_err(|_| ApiError::Unauthorized("invalid or expired token".into()))
}

// ---------------------------------------------------------------------------
// Extractor
// ---------------------------------------------------------------------------

/// The authenticated caller. Adding this parameter to a handler is what makes a
/// route protected — handlers must never take a user id from the path or body.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: String,
}

impl FromRequest for AuthUser {
    type Error = ApiError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        ready(extract_auth_user(req))
    }
}

fn extract_auth_user(req: &HttpRequest) -> Result<AuthUser, ApiError> {
    let state = req
        .app_data::<web::Data<AppState>>()
        .ok_or_else(|| ApiError::Internal("app state is not configured".into()))?;

    let header = req
        .headers()
        .get(actix_web::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::Unauthorized("missing Authorization header".into()))?;

    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .ok_or_else(|| ApiError::Unauthorized("expected a Bearer token".into()))?
        .trim();

    let claims = decode_token(token, &state.config.jwt_secret)?;

    Ok(AuthUser { id: claims.sub })
}
