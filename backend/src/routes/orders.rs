//! Ordering, delegated to the furniture shop API.
//!
//! Budget and stock are no longer enforced here: upstream owns the ledger and
//! debits the balance as part of placing the order. This module's whole job is
//! to forward the request with our API key, convert money to cents, and let
//! `external_api::map_error` turn upstream's failures into ones the UI can
//! phrase sensibly.

use actix_web::{web, HttpResponse};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::external_api::to_cents;
use crate::models::{BuyRequest, PlacedOrder, PlacedOrderLine};
use crate::state::AppState;

/// `POST /api/orders` — buy one product.
///
/// The browser supplies `idempotency_key`; upstream then returns the original
/// result for a repeated key rather than charging again. That, plus the button
/// being disabled while the request is in flight, is what makes the
/// double-click case safe.
pub async fn create(
    state: web::Data<AppState>,
    _user: AuthUser,
    body: web::Json<BuyRequest>,
) -> Result<HttpResponse, ApiError> {
    let body = body.into_inner();

    if body.quantity < 1 {
        return Err(ApiError::BadRequest(
            "quantity must be at least 1".to_string(),
        ));
    }
    if body.item_id.trim().is_empty() {
        return Err(ApiError::BadRequest("item_id is required".to_string()));
    }

    let result = state
        .external_api
        .place_order(
            &[(body.item_id, body.quantity)],
            body.idempotency_key.as_deref(),
        )
        .await?;

    let items = result
        .items
        .into_iter()
        .map(|line| PlacedOrderLine {
            item_id: line.item_id,
            product_name: None,
            quantity: line.quantity,
            unit_price_cents: to_cents(line.unit_price),
            line_total_cents: to_cents(line.line_total),
        })
        .collect();

    Ok(HttpResponse::Created().json(PlacedOrder {
        order_id: result.order_id,
        items,
        total_cents: to_cents(result.total_price),
        remaining_balance_cents: Some(to_cents(result.remaining_balance)),
        timestamp: None,
    }))
}

/// `GET /api/orders` — the participant's order history, newest first.
pub async fn list(state: web::Data<AppState>, _user: AuthUser) -> Result<HttpResponse, ApiError> {
    let mut records = state.external_api.order_history().await?;

    // Upstream does not promise an order; sort newest-first ourselves. RFC 3339
    // timestamps sort correctly as strings, and missing ones sink to the end.
    records.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    let orders: Vec<PlacedOrder> = records
        .into_iter()
        .map(|record| PlacedOrder {
            order_id: record.order_id,
            items: record
                .items
                .into_iter()
                .map(|item| PlacedOrderLine {
                    quantity: item.quantity,
                    unit_price_cents: to_cents(item.unit_price),
                    line_total_cents: to_cents(item.unit_price) * item.quantity,
                    item_id: item.product_id,
                    product_name: item.product_name,
                })
                .collect(),
            total_cents: to_cents(record.total_amount),
            remaining_balance_cents: None,
            timestamp: record.timestamp,
        })
        .collect();

    Ok(HttpResponse::Ok().json(orders))
}
