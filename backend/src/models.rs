//! Local types.
//!
//! Since the furniture shop API became the source of truth, our database holds
//! login accounts and nothing else — there is no local product, order, or
//! balance type here. Everything money-shaped is an upstream-backed DTO with
//! amounts restated as integer cents (see `external_api::to_cents`).

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ---------------------------------------------------------------------------
// Local accounts
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub password_hash: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub created_at: String,
}

impl From<User> for UserResponse {
    fn from(u: User) -> Self {
        UserResponse {
            id: u.id,
            email: u.email,
            display_name: u.display_name,
            created_at: u.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserResponse,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

// ---------------------------------------------------------------------------
// Upstream-backed DTOs
//
// These mirror the furniture shop API but restate money as integer cents, so
// no float crosses into the rest of the app or out to the browser.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct CatalogueProduct {
    pub item_id: String,
    pub product_name: String,
    pub price_cents: i64,
    pub category: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub depth: Option<f64>,
    pub colours: Vec<String>,
    pub image_url: Option<String>,
    pub link: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BalanceResponse {
    pub user_id: String,
    pub name: String,
    pub balance_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct PlacedOrderLine {
    pub item_id: String,
    pub product_name: Option<String>,
    pub quantity: i64,
    pub unit_price_cents: i64,
    pub line_total_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct PlacedOrder {
    pub order_id: String,
    pub items: Vec<PlacedOrderLine>,
    pub total_cents: i64,
    /// Present on a freshly placed order; absent in history, which upstream
    /// does not report a running balance for.
    pub remaining_balance_cents: Option<i64>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BuyRequest {
    pub item_id: String,
    #[serde(default = "one")]
    pub quantity: i64,
    /// Supplied by the browser so a double-click, or a retry after a dropped
    /// connection, cannot charge twice.
    pub idempotency_key: Option<String>,
}

fn one() -> i64 {
    1
}

#[derive(Debug, Deserialize)]
pub struct CatalogueQuery {
    pub search: Option<String>,
    pub category: Option<String>,
}
