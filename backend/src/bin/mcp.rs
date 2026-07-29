//! MCP server exposing the furniture shop as four agent tools.
//!
//! Run over stdio: `cargo run --bin furniture-mcp`. Credentials come from the
//! same `.env` as the HTTP API, and every call goes through the shared
//! [`ExternalApiClient`] — so the agent path and the app path cannot drift.
//!
//! # Design notes
//!
//! Three things about this tool surface are deliberate:
//!
//! 1. **`search_furniture_catalogue` cannot fail silently.** Upstream returns
//!    an empty list for an unrecognised category — indistinguishable from
//!    "nothing in stock", and exactly the kind of non-error a model reports to
//!    the user as fact. Categories are validated here against the live list,
//!    and a miss returns an error naming every valid value, so a wrong guess
//!    is loud and self-correcting.
//! 2. **`get_furniture_product` strips the image.** Upstream puts ~60KB of
//!    base64 JPEG in `image_url`; left in, it would burn the model's context
//!    for no benefit.
//! 3. **`place_furniture_order` spends real money and cannot be undone** —
//!    there is no cancel or refund endpoint. That has to be in the tool
//!    description itself, not just the host's system prompt, because the
//!    description is what the model reads at the moment it decides to call.

use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::transport::stdio;
use rmcp::{schemars, tool, tool_handler, tool_router, ErrorData, ServerHandler, ServiceExt};
use serde_json::json;

use furniture_buyer_api::config::Config;
use furniture_buyer_api::external_api::ExternalApiClient;

/// Upstream's cap. The catalogue is 762 items, so this fetches everything.
const MAX_LIMIT: u32 = 1000;

// ---------------------------------------------------------------------------
// Tool parameters
//
// Field doc comments become the parameter descriptions in the JSON schema.
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    /// Exact category name to filter by. Must match one of the shop's
    /// categories exactly (case-insensitive); partial names like "Sofas" or
    /// "bed" match nothing. Omit to list the whole catalogue.
    pub category: Option<String>,
    /// Maximum number of products to return (1-1000). Defaults to all of them.
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ProductParams {
    /// The exact item_id from a search result, e.g. "00368814". This is a
    /// lookup, not a search — a product name will not work.
    pub item_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct OrderParams {
    /// The exact item_id of the product to buy, from a search result.
    pub item_id: String,
    /// How many units to buy. Defaults to 1.
    pub quantity: Option<i64>,
    /// A unique string identifying this purchase intent. Reusing the same key
    /// returns the original order instead of charging again, so a retry after
    /// a timeout is safe. Generate a fresh one for each genuinely new purchase.
    pub idempotency_key: Option<String>,
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct FurnitureShop {
    client: Arc<ExternalApiClient>,
    // Read by the code `#[tool_handler]` generates, which the dead-code pass
    // doesn't see through.
    #[allow(dead_code)]
    tool_router: rmcp::handler::server::router::tool::ToolRouter<Self>,
}

fn upstream_error(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

#[tool_router]
impl FurnitureShop {
    pub fn new(client: ExternalApiClient) -> Self {
        Self {
            client: Arc::new(client),
            tool_router: Self::tool_router(),
        }
    }

    /// Maps a caller-supplied category onto a real one, or explains what the
    /// valid options are. This is the guard that turns upstream's silent empty
    /// result into an actionable error.
    async fn resolve_category(&self, requested: &str) -> Result<String, ErrorData> {
        let categories = self.client.categories().await.map_err(upstream_error)?;

        let wanted = requested.trim().to_lowercase();
        match categories.iter().find(|c| c.to_lowercase() == wanted) {
            Some(matched) => Ok(matched.clone()),
            None => Err(ErrorData::invalid_params(
                format!(
                    "\"{requested}\" is not a category in this shop. Categories must match \
                     exactly (case-insensitive). Valid categories: {}. Retry with one of \
                     these, or omit the category to search the whole catalogue.",
                    categories.join(", ")
                ),
                None,
            )),
        }
    }

    #[tool(
        description = "Lists furniture from the shop's 762-item catalogue, optionally narrowed \
to one exact category, returning each item's ID, name, price, colours and dimensions so you \
can filter the results yourself. This endpoint CANNOT search by price, colour, style, keyword \
or any free-text description — to answer questions like \"a blue chair under 200\", list the \
category and filter the returned data. Valid categories, which must match exactly: \
Bar furniture, Beds, Bookcases & shelving units, Cabinets & cupboards, Café furniture, Chairs, \
Chests of drawers & drawer units, Children's furniture, Nursery furniture, Outdoor furniture, \
Room dividers, Sideboards buffets & console tables, Sofas & armchairs, TV & media furniture, \
Tables & desks, Trolleys, Wardrobes."
    )]
    pub async fn search_furniture_catalogue(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<String, ErrorData> {
        let category = match params.category.as_deref().map(str::trim) {
            Some(c) if !c.is_empty() => Some(self.resolve_category(c).await?),
            _ => None,
        };

        let limit = params.limit.unwrap_or(MAX_LIMIT).clamp(1, MAX_LIMIT);

        let products = self
            .client
            .search_index(category.as_deref(), limit, 0)
            .await
            .map_err(upstream_error)?;

        let items: Vec<_> = products
            .iter()
            .map(|p| {
                json!({
                    "item_id": p.item_id,
                    "product_name": p.product_name,
                    "price": p.price,
                    "category": p.category,
                    "colours": p.colours.clone().unwrap_or_default(),
                    "width_cm": p.width,
                    "height_cm": p.height,
                    "depth_cm": p.depth,
                })
            })
            .collect();

        Ok(json!({
            "count": items.len(),
            "filtered_by_category": category,
            "products": items,
        })
        .to_string())
    }

    #[tool(
        description = "Fetches full details of one furniture item by its exact item_id. Use this \
only after search_furniture_catalogue has given you an ID — it cannot find products by name. \
Search results already include price, category, colours and dimensions, so call this only when \
you need the product link or want to confirm a specific item still exists."
    )]
    pub async fn get_furniture_product(
        &self,
        Parameters(params): Parameters<ProductParams>,
    ) -> Result<String, ErrorData> {
        let p = self
            .client
            .product(params.item_id.trim())
            .await
            .map_err(upstream_error)?;

        // `image_url` holds ~60KB of base64 JPEG, not a URL. Report whether a
        // photo exists; never put the bytes in the model's context.
        Ok(json!({
            "item_id": p.item_id,
            "product_name": p.product_name,
            "price": p.price,
            "category": p.category,
            "colours": p.colours.unwrap_or_default(),
            "width_cm": p.width,
            "height_cm": p.height,
            "depth_cm": p.depth,
            "product_page": p.link,
            "has_image": p.image_url.is_some(),
        })
        .to_string())
    }

    #[tool(
        description = "Returns the current spending balance at the furniture shop. Call it before \
proposing a purchase and again afterwards, since every order debits the balance immediately. \
Takes no arguments — there is one account, fixed by the shop credentials."
    )]
    pub async fn check_shop_balance(&self) -> Result<String, ErrorData> {
        let user = self.client.balance().await.map_err(upstream_error)?;

        Ok(json!({
            "user_id": user.user_id,
            "account_name": user.name,
            "balance": user.balance,
        })
        .to_string())
    }

    #[tool(
        description = "Buys furniture: places an order for a quantity of one item and immediately \
debits the balance. THIS SPENDS REAL MONEY AND CANNOT BE UNDONE — the shop has no cancel, \
refund, or return endpoint. Confirm the exact item and its price with the user before calling. \
Fails without charging if the total exceeds the available balance, or if the item_id does not \
exist; an order is never partially filled. The shop tracks no stock levels, so any quantity the \
balance covers will succeed."
    )]
    pub async fn place_furniture_order(
        &self,
        Parameters(params): Parameters<OrderParams>,
    ) -> Result<String, ErrorData> {
        let quantity = params.quantity.unwrap_or(1);
        if quantity < 1 {
            return Err(ErrorData::invalid_params(
                "quantity must be at least 1".to_string(),
                None,
            ));
        }

        let item_id = params.item_id.trim().to_string();
        if item_id.is_empty() {
            return Err(ErrorData::invalid_params(
                "item_id is required".to_string(),
                None,
            ));
        }

        let result = self
            .client
            .place_order(&[(item_id, quantity)], params.idempotency_key.as_deref())
            .await
            .map_err(upstream_error)?;

        let lines: Vec<_> = result
            .items
            .iter()
            .map(|l| {
                json!({
                    "item_id": l.item_id,
                    "quantity": l.quantity,
                    "unit_price": l.unit_price,
                    "line_total": l.line_total,
                })
            })
            .collect();

        Ok(json!({
            "status": "purchased",
            "order_id": result.order_id,
            "items": lines,
            "total_price": result.total_price,
            "remaining_balance": result.remaining_balance,
        })
        .to_string())
    }
}

#[tool_handler]
impl ServerHandler for FurnitureShop {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        // ServerInfo is #[non_exhaustive] — build from Default and set fields.
        let mut info = rmcp::model::ServerInfo::default();
        info.capabilities = rmcp::model::ServerCapabilities::builder()
            .enable_tools()
            .build();
        info.instructions = Some(
            "Tools for a furniture shop: browse the catalogue, look up a product, check the \
                 spending balance, and buy. Search is category-and-filter, not free-text: there \
                 is no keyword, price, or colour search, so list a category and filter the \
                 results yourself. Placing an order spends real money and cannot be reversed — \
                 always confirm the item and price with the user first."
                .to_string(),
        );
        info
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    // stdout is the MCP transport — logs must go to stderr or they corrupt the
    // JSON-RPC stream.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .target(env_logger::Target::Stderr)
        .init();

    let config = Config::from_env();
    if !config.external_api.is_configured() {
        return Err(
            "furniture shop API not configured; set EXTERNAL_API_BASE_URL, \
             EXTERNAL_API_USER_ID and EXTERNAL_API_KEY in backend/.env"
                .into(),
        );
    }

    let client = ExternalApiClient::new(config.external_api.clone())?;
    let service = FurnitureShop::new(client).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
