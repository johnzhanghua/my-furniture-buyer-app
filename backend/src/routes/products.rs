use actix_web::{web, HttpResponse};

use crate::error::ApiError;
use crate::external_api::{to_cents, UpstreamProduct};
use crate::models::{CatalogueProduct, CatalogueQuery};
use crate::state::AppState;

/// Upstream caps `limit` at 1000 and the catalogue is 762 items, so one request
/// fetches everything. That is what makes local text search below viable — if
/// the catalogue outgrows the cap this needs real pagination.
const CATALOGUE_LIMIT: u32 = 1000;

impl From<UpstreamProduct> for CatalogueProduct {
    fn from(p: UpstreamProduct) -> Self {
        CatalogueProduct {
            item_id: p.item_id,
            product_name: p.product_name,
            price_cents: to_cents(p.price),
            category: p.category,
            width: p.width,
            height: p.height,
            depth: p.depth,
            colours: p.colours.unwrap_or_default(),
            // Upstream puts raw base64 image data in `image_url` on the detail
            // endpoint. Wrap it as a data URI so an <img src> actually works;
            // pass through anything that is already a real URL.
            image_url: match (p.image_url, p.image_mime_type) {
                (Some(data), _) if data.starts_with("http") || data.starts_with("data:") => {
                    Some(data)
                }
                (Some(data), Some(mime)) => Some(format!("data:{mime};base64,{data}")),
                (Some(data), None) => Some(format!("data:image/jpeg;base64,{data}")),
                (None, _) => None,
            },
            link: p.link,
        }
    }
}

pub async fn list(
    state: web::Data<AppState>,
    query: web::Query<CatalogueQuery>,
) -> Result<HttpResponse, ApiError> {
    let query = query.into_inner();
    let category = query
        .category
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    let search = query
        .search
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());

    // Category filtering is pushed upstream; free-text is not, because
    // search-index has no text parameter.
    let products = state
        .external_api
        .search_index(category, CATALOGUE_LIMIT, 0)
        .await?;

    let products: Vec<CatalogueProduct> = products
        .into_iter()
        .map(CatalogueProduct::from)
        .filter(|p| match &search {
            None => true,
            Some(term) => {
                p.product_name.to_lowercase().contains(term)
                    || p.item_id.to_lowercase().contains(term)
                    || p.category
                        .as_deref()
                        .is_some_and(|c| c.to_lowercase().contains(term))
            }
        })
        .collect();

    Ok(HttpResponse::Ok().json(products))
}

pub async fn categories(state: web::Data<AppState>) -> Result<HttpResponse, ApiError> {
    Ok(HttpResponse::Ok().json(state.external_api.categories().await?))
}

pub async fn detail(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let product = state.external_api.product(&path.into_inner()).await?;
    Ok(HttpResponse::Ok().json(CatalogueProduct::from(product)))
}

/// Streams a product photo. The browser points `<img src>` straight here, so
/// the key never leaves the server and the image is cached like any other
/// static asset.
pub async fn image(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let (bytes, content_type) = state.external_api.product_image(&path.into_inner()).await?;

    Ok(HttpResponse::Ok()
        .content_type(content_type)
        // Catalogue photos don't change; let the browser keep them for a day.
        .insert_header(("Cache-Control", "public, max-age=86400"))
        .body(bytes))
}
