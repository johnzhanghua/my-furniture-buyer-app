//! Client for the hackathon's "Product Search / Order / Ledger API".
//!
//! Base URL `https://day1.training.cognitivo.com.au`, auth by a single
//! `X-Api-Key` header. The participant `user_id` is a path segment or body
//! field, never a header.
//!
//! # Upstream is the source of truth
//!
//! Catalogue, balance, and orders all live upstream. Our SQLite database keeps
//! only local login accounts. Nothing here writes to our database.
//!
//! # Money
//!
//! Upstream sends prices and balances as JSON floats (`398.0`, `5000.0`); this
//! app uses integer cents everywhere (see DD-1). Conversion happens here, at
//! the boundary, via [`to_cents`], and only ever inbound — order requests carry
//! `item_id` and `quantity` only, so no amount is ever sent back upstream and
//! there is no round-trip to drift.

use serde::{Deserialize, Serialize};

use crate::config::ExternalApiConfig;
use crate::error::ApiError;

/// Upstream floats → integer cents. `f64` represents every value the catalogue
/// uses exactly enough that rounding to the nearest cent is lossless here
/// (e.g. `19.99 * 100.0` is `1998.999…`, which rounds to `1999`).
pub fn to_cents(amount: f64) -> i64 {
    (amount * 100.0).round() as i64
}

// ---------------------------------------------------------------------------
// Upstream wire types (field names must match the API exactly)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct UpstreamProduct {
    pub item_id: String,
    pub product_name: String,
    pub price: f64,
    pub category: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub depth: Option<f64>,
    pub colours: Option<Vec<String>>,
    /// Misleadingly named upstream: `/catalogue/{item_id}` returns base64 image
    /// *data* here, not a URL. `search-index` leaves it null. See
    /// `CatalogueProduct::from`.
    pub image_url: Option<String>,
    pub image_mime_type: Option<String>,
    pub link: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpstreamUser {
    pub user_id: String,
    pub name: String,
    pub balance: f64,
}

#[derive(Debug, Serialize)]
struct OrderLineRequest<'a> {
    item_id: &'a str,
    quantity: i64,
}

#[derive(Debug, Serialize)]
struct OrderRequest<'a> {
    user_id: &'a str,
    items: Vec<OrderLineRequest<'a>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpstreamOrderLine {
    pub item_id: String,
    pub quantity: i64,
    pub unit_price: f64,
    pub line_total: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpstreamOrderResult {
    pub order_id: String,
    pub items: Vec<UpstreamOrderLine>,
    pub total_price: f64,
    pub remaining_balance: f64,
}

/// Order history lines use different field names from the order *result*
/// (`product_id`/`product_name` rather than `item_id`, no `line_total`).
#[derive(Debug, Clone, Deserialize)]
pub struct UpstreamHistoryItem {
    pub product_id: String,
    pub quantity: i64,
    pub unit_price: f64,
    pub product_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpstreamOrderRecord {
    pub order_id: String,
    pub items: Vec<UpstreamHistoryItem>,
    pub total_amount: f64,
    pub timestamp: Option<String>,
}

/// FastAPI's error envelope: `{"detail": "..."}`. `detail` is an array for
/// validation errors, hence the untyped value.
#[derive(Debug, Deserialize)]
struct UpstreamError {
    detail: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ExternalApiClient {
    http: reqwest::Client,
    config: ExternalApiConfig,
}

impl ExternalApiClient {
    pub fn new(config: ExternalApiConfig) -> Result<Self, ApiError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            // Don't follow redirects. An API that redirects is telling us
            // something — usually that auth failed and we're being sent to a
            // login page. Following it turns that signal into a misleading
            // `200 OK` carrying HTML.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| ApiError::Internal(format!("could not build HTTP client: {e}")))?;

        Ok(Self { http, config })
    }

    pub fn is_configured(&self) -> bool {
        self.config.is_configured()
    }

    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<reqwest::RequestBuilder, ApiError> {
        if !self.config.is_configured() {
            return Err(ApiError::Upstream(
                "the furniture shop API is not configured; set EXTERNAL_API_BASE_URL and EXTERNAL_API_KEY".into(),
            ));
        }

        let url = format!("{}/{}", self.config.base_url, path.trim_start_matches('/'));
        Ok(self
            .http
            .request(method, url)
            .header(&self.config.key_header, &self.config.api_key))
    }

    // -- endpoints ----------------------------------------------------------

    /// Lightweight catalogue listing. The guide is explicit that this is the
    /// endpoint to browse with — `/catalogue` returns embedded images and is
    /// far slower.
    pub async fn search_index(
        &self,
        category: Option<&str>,
        limit: u32,
        skip: u32,
    ) -> Result<Vec<UpstreamProduct>, ApiError> {
        let mut req = self
            .request(reqwest::Method::GET, "/catalogue/search-index")?
            .query(&[("limit", limit.to_string()), ("skip", skip.to_string())]);

        if let Some(category) = category {
            req = req.query(&[("category", category)]);
        }

        self.send(req, "/catalogue/search-index").await
    }

    pub async fn categories(&self) -> Result<Vec<String>, ApiError> {
        let req = self.request(reqwest::Method::GET, "/catalogue/categories")?;
        self.send(req, "/catalogue/categories").await
    }

    pub async fn product(&self, item_id: &str) -> Result<UpstreamProduct, ApiError> {
        let path = format!("/catalogue/{item_id}");
        let req = self.request(reqwest::Method::GET, &path)?;
        self.send(req, &path).await
    }

    pub async fn balance(&self) -> Result<UpstreamUser, ApiError> {
        let path = format!("/users/{}", self.config.user_id);
        let req = self.request(reqwest::Method::GET, &path)?;
        self.send(req, &path).await
    }

    pub async fn order_history(&self) -> Result<Vec<UpstreamOrderRecord>, ApiError> {
        let path = format!("/orders/{}", self.config.user_id);
        let req = self.request(reqwest::Method::GET, &path)?;
        self.send(req, &path).await
    }

    /// Places (and pays for) an order.
    ///
    /// `idempotency_key` makes a retry — or a double-clicked Buy button —
    /// return the original result instead of charging twice.
    pub async fn place_order(
        &self,
        items: &[(String, i64)],
        idempotency_key: Option<&str>,
    ) -> Result<UpstreamOrderResult, ApiError> {
        let body = OrderRequest {
            user_id: &self.config.user_id,
            items: items
                .iter()
                .map(|(item_id, quantity)| OrderLineRequest {
                    item_id,
                    quantity: *quantity,
                })
                .collect(),
        };

        let mut req = self.request(reqwest::Method::POST, "/orders")?.json(&body);
        if let Some(key) = idempotency_key {
            req = req.header("Idempotency-Key", key);
        }

        self.send(req, "/orders").await
    }

    // -- plumbing -----------------------------------------------------------

    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        req: reqwest::RequestBuilder,
        path: &str,
    ) -> Result<T, ApiError> {
        let response = req.send().await.map_err(|e| {
            log::error!("upstream {path} request failed: {e}");
            if e.is_timeout() {
                ApiError::Upstream("the furniture shop API timed out".into())
            } else {
                ApiError::Upstream("could not reach the furniture shop API".into())
            }
        })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Self::map_error(path, status, &body));
        }

        response.json::<T>().await.map_err(|e| {
            log::error!("upstream {path} returned an unreadable body: {e}");
            ApiError::Upstream("the furniture shop API returned an unexpected response".into())
        })
    }

    /// Translates upstream's status codes into ours. The mapping is the whole
    /// reason failures reach the user as sentences rather than stack traces.
    fn map_error(path: &str, status: reqwest::StatusCode, body: &str) -> ApiError {
        let detail = serde_json::from_str::<UpstreamError>(body)
            .ok()
            .and_then(|e| e.detail)
            .map(|d| match d {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            })
            .unwrap_or_else(|| body.to_string());

        log::error!("upstream {path} returned {status}: {detail}");

        match status.as_u16() {
            // Our API key is wrong or missing. That is our misconfiguration,
            // not something the end user can act on, so it is never surfaced
            // as an auth failure of *theirs*.
            401 | 403 => ApiError::Upstream(
                "the furniture shop rejected this app's API key; check EXTERNAL_API_KEY".into(),
            ),
            402 => ApiError::InsufficientBalance(detail),
            404 => ApiError::NotFound("this item is no longer available".into()),
            409 => ApiError::Conflict(detail),
            422 => ApiError::BadRequest(detail),
            429 => ApiError::Upstream("the furniture shop is rate limiting this app".into()),
            _ => ApiError::Upstream(format!("the furniture shop API returned {status}")),
        }
    }
}
