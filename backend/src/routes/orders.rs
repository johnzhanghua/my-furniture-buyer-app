use std::collections::HashMap;

use actix_web::{web, HttpResponse};
use sqlx::FromRow;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::models::{CreateOrderRequest, OrderItemResponse, OrderResponse, Product};
use crate::state::AppState;

#[derive(Debug, FromRow)]
struct OrderRow {
    id: String,
    total_cents: i64,
    status: String,
    created_at: String,
}

#[derive(Debug, FromRow)]
struct OrderItemRow {
    order_id: String,
    product_id: String,
    sku: String,
    name: String,
    quantity: i64,
    unit_price_cents: i64,
}

const ITEM_QUERY: &str =
    "SELECT oi.order_id, oi.product_id, p.sku, p.name, oi.quantity, oi.unit_price_cents
     FROM order_items oi
     JOIN products p ON p.id = oi.product_id
     WHERE oi.order_id IN (SELECT id FROM orders WHERE user_id = ?)";

/// Places an order. Budget and stock are both enforced here, inside one
/// transaction — this is the only place either rule is applied.
pub async fn create(
    state: web::Data<AppState>,
    user: AuthUser,
    body: web::Json<CreateOrderRequest>,
) -> Result<HttpResponse, ApiError> {
    let requested = body.into_inner().items;
    if requested.is_empty() {
        return Err(ApiError::BadRequest(
            "an order must contain at least one item".into(),
        ));
    }

    // Collapse repeated product lines so budget and stock checks see the real
    // totals rather than each line in isolation.
    let mut merged: Vec<(String, i64)> = Vec::new();
    for item in requested {
        if item.quantity <= 0 {
            return Err(ApiError::BadRequest(format!(
                "quantity for product {} must be greater than zero",
                item.product_id
            )));
        }
        match merged.iter_mut().find(|(id, _)| id == &item.product_id) {
            Some(entry) => entry.1 += item.quantity,
            None => merged.push((item.product_id, item.quantity)),
        }
    }

    let mut tx = state.pool.begin().await?;

    let budget_cents: i64 = sqlx::query_scalar("SELECT budget_cents FROM users WHERE id = ?")
        .bind(user.id.as_str())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ApiError::Unauthorized("account no longer exists".into()))?;

    let spent_cents: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(total_cents), 0) FROM orders
         WHERE user_id = ? AND status <> 'cancelled'",
    )
    .bind(user.id.as_str())
    .fetch_one(&mut *tx)
    .await?;

    let remaining_cents = budget_cents - spent_cents;

    // Price every line from the database; the client never supplies a price.
    let mut total_cents: i64 = 0;
    let mut lines: Vec<(Product, i64)> = Vec::with_capacity(merged.len());

    for (product_id, quantity) in merged {
        let product = sqlx::query_as::<_, Product>(
            "SELECT id, sku, name, description, category, price_cents, stock, image_url
             FROM products WHERE id = ?",
        )
        .bind(product_id.as_str())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("no product with id {product_id}")))?;

        if product.stock < quantity {
            return Err(ApiError::InsufficientStock {
                name: product.name,
                requested: quantity,
                available: product.stock,
            });
        }

        total_cents += product.price_cents * quantity;
        lines.push((product, quantity));
    }

    if total_cents > remaining_cents {
        return Err(ApiError::InsufficientBudget {
            total_cents,
            remaining_cents,
        });
    }

    let order_id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO orders (id, user_id, total_cents, status, created_at)
         VALUES (?, ?, ?, 'placed', ?)",
    )
    .bind(order_id.as_str())
    .bind(user.id.as_str())
    .bind(total_cents)
    .bind(created_at.as_str())
    .execute(&mut *tx)
    .await?;

    let mut items = Vec::with_capacity(lines.len());
    for (product, quantity) in lines {
        sqlx::query(
            "INSERT INTO order_items (id, order_id, product_id, quantity, unit_price_cents)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(order_id.as_str())
        .bind(product.id.as_str())
        .bind(quantity)
        .bind(product.price_cents)
        .execute(&mut *tx)
        .await?;

        // `stock >= ?` in the WHERE clause makes the decrement itself the
        // authority, so a concurrent order cannot drive stock negative.
        let updated =
            sqlx::query("UPDATE products SET stock = stock - ? WHERE id = ? AND stock >= ?")
                .bind(quantity)
                .bind(product.id.as_str())
                .bind(quantity)
                .execute(&mut *tx)
                .await?;

        if updated.rows_affected() == 0 {
            return Err(ApiError::InsufficientStock {
                name: product.name,
                requested: quantity,
                available: product.stock,
            });
        }

        items.push(OrderItemResponse {
            product_id: product.id,
            sku: product.sku,
            name: product.name,
            quantity,
            unit_price_cents: product.price_cents,
            line_total_cents: product.price_cents * quantity,
        });
    }

    tx.commit().await?;

    Ok(HttpResponse::Created().json(OrderResponse {
        id: order_id,
        total_cents,
        status: "placed".to_string(),
        created_at,
        items,
    }))
}

pub async fn list(state: web::Data<AppState>, user: AuthUser) -> Result<HttpResponse, ApiError> {
    let orders = sqlx::query_as::<_, OrderRow>(
        "SELECT id, total_cents, status, created_at FROM orders
         WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(user.id.as_str())
    .fetch_all(&state.pool)
    .await?;

    // One extra query for all items, then group in memory — cheaper than a
    // query per order and simple enough at hackathon scale.
    let item_rows = sqlx::query_as::<_, OrderItemRow>(ITEM_QUERY)
        .bind(user.id.as_str())
        .fetch_all(&state.pool)
        .await?;

    let mut grouped: HashMap<String, Vec<OrderItemResponse>> = HashMap::new();
    for row in item_rows {
        grouped
            .entry(row.order_id)
            .or_default()
            .push(OrderItemResponse {
                product_id: row.product_id,
                sku: row.sku,
                name: row.name,
                quantity: row.quantity,
                unit_price_cents: row.unit_price_cents,
                line_total_cents: row.unit_price_cents * row.quantity,
            });
    }

    let response: Vec<OrderResponse> = orders
        .into_iter()
        .map(|o| OrderResponse {
            items: grouped.remove(&o.id).unwrap_or_default(),
            id: o.id,
            total_cents: o.total_cents,
            status: o.status,
            created_at: o.created_at,
        })
        .collect();

    Ok(HttpResponse::Ok().json(response))
}

pub async fn detail(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let order_id = path.into_inner();

    // Scoped by user_id: another buyer's order id reads as "not found".
    let order = sqlx::query_as::<_, OrderRow>(
        "SELECT id, total_cents, status, created_at FROM orders
         WHERE id = ? AND user_id = ?",
    )
    .bind(order_id.as_str())
    .bind(user.id.as_str())
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("no order with id {order_id}")))?;

    let item_rows = sqlx::query_as::<_, OrderItemRow>(
        "SELECT oi.order_id, oi.product_id, p.sku, p.name, oi.quantity, oi.unit_price_cents
         FROM order_items oi
         JOIN products p ON p.id = oi.product_id
         WHERE oi.order_id = ?",
    )
    .bind(order_id.as_str())
    .fetch_all(&state.pool)
    .await?;

    let items = item_rows
        .into_iter()
        .map(|row| OrderItemResponse {
            product_id: row.product_id,
            sku: row.sku,
            name: row.name,
            quantity: row.quantity,
            unit_price_cents: row.unit_price_cents,
            line_total_cents: row.unit_price_cents * row.quantity,
        })
        .collect();

    Ok(HttpResponse::Ok().json(OrderResponse {
        id: order.id,
        total_cents: order.total_cents,
        status: order.status,
        created_at: order.created_at,
        items,
    }))
}
