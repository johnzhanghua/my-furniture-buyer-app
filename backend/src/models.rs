use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ---------------------------------------------------------------------------
// Database rows
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub password_hash: String,
    pub budget_cents: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Product {
    pub id: String,
    pub sku: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub price_cents: i64,
    pub stock: i64,
    pub image_url: String,
}

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub budget_cents: i64,
    pub created_at: String,
}

impl From<User> for UserResponse {
    fn from(u: User) -> Self {
        UserResponse {
            id: u.id,
            email: u.email,
            display_name: u.display_name,
            budget_cents: u.budget_cents,
            created_at: u.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserResponse,
}

#[derive(Debug, Serialize)]
pub struct BudgetResponse {
    pub budget_cents: i64,
    pub spent_cents: i64,
    pub remaining_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct OrderItemResponse {
    pub product_id: String,
    pub sku: String,
    pub name: String,
    pub quantity: i64,
    pub unit_price_cents: i64,
    pub line_total_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct OrderResponse {
    pub id: String,
    pub total_cents: i64,
    pub status: String,
    pub created_at: String,
    pub items: Vec<OrderItemResponse>,
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

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

/// Note the absence of a price field: unit prices are always re-read from the
/// database so a client cannot dictate what it pays.
#[derive(Debug, Deserialize)]
pub struct OrderItemInput {
    pub product_id: String,
    pub quantity: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateOrderRequest {
    pub items: Vec<OrderItemInput>,
}

#[derive(Debug, Deserialize)]
pub struct ProductQuery {
    pub search: Option<String>,
    pub category: Option<String>,
}
