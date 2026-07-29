use actix_web::{web, HttpResponse};

use crate::error::ApiError;
use crate::models::{Product, ProductQuery};
use crate::state::AppState;

const PRODUCT_COLUMNS: &str = "id, sku, name, description, category, price_cents, stock, image_url";

pub async fn list(
    state: web::Data<AppState>,
    query: web::Query<ProductQuery>,
) -> Result<HttpResponse, ApiError> {
    let search = query
        .search
        .as_ref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());
    let category = query
        .category
        .as_ref()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty());

    let mut sql = format!("SELECT {PRODUCT_COLUMNS} FROM products WHERE 1 = 1");
    if search.is_some() {
        sql.push_str(" AND (LOWER(name) LIKE ? OR LOWER(description) LIKE ? OR LOWER(sku) LIKE ?)");
    }
    if category.is_some() {
        sql.push_str(" AND category = ?");
    }
    sql.push_str(" ORDER BY category ASC, name ASC");

    let mut stmt = sqlx::query_as::<_, Product>(&sql);
    if let Some(term) = &search {
        let pattern = format!("%{term}%");
        stmt = stmt
            .bind(pattern.clone())
            .bind(pattern.clone())
            .bind(pattern);
    }
    if let Some(category) = &category {
        stmt = stmt.bind(category.as_str());
    }

    let products = stmt.fetch_all(&state.pool).await?;
    Ok(HttpResponse::Ok().json(products))
}

pub async fn detail(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();

    let product = sqlx::query_as::<_, Product>(&format!(
        "SELECT {PRODUCT_COLUMNS} FROM products WHERE id = ?"
    ))
    .bind(id.as_str())
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("no product with id {id}")))?;

    Ok(HttpResponse::Ok().json(product))
}
