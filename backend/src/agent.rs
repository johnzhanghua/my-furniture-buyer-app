//! The in-app shopping assistant.
//!
//! Backed by **Azure OpenAI** (`gpt-5-mini`) over raw HTTP — Rust has no
//! official client for it. Three things about that deployment shape this file:
//!
//! - It is a reasoning model: `max_tokens` is rejected with a 400, and
//!   `max_completion_tokens` covers reasoning *plus* the visible answer.
//! - Depth is tuned with `reasoning_effort`, not a token budget.
//! - Tool-call arguments arrive as a JSON **string**, so they need parsing
//!   before dispatch.
//!
//! # The judgement split
//!
//! The furniture shop's API can filter by exact category and nothing else. It
//! has no concept of "cheap", "blue", "small enough for a studio", or "goes
//! with oak". Rather than pretend otherwise, the tools return plain rows and
//! the system prompt makes the model responsible for that judgement: fetch a
//! category, then reason over the results. Every qualitative word in the user's
//! request is the model's job, not the API's.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::AgentConfig;
use crate::error::ApiError;
use crate::external_api::{to_cents, ExternalApiClient};
use crate::models::CatalogueProduct;
use crate::tools;

/// Ceiling on tool round trips, so a confused model can't loop forever.
const MAX_TURNS: usize = 8;

const SYSTEM_PROMPT: &str = "You are a shopping assistant for a furniture shop. The user is \
signed in and browsing the catalogue.

The shop's API is deliberately dumb: it can list products and filter by one exact category, and \
that is all. It cannot search by price, colour, size, material, style, room, or any free-text \
phrase. So when the user says something qualitative — \"cheap\", \"blue\", \"something for a \
small flat\", \"a nice desk\" — that judgement is YOURS to make. Fetch the relevant category \
with search_furniture_catalogue, then read the returned rows and decide yourself which items \
match. Never tell the user the shop cannot search for something; just do the filtering.

Earlier turns in this conversation are replayed to you, including the numbered \
list you last showed. Use them: \"the third one\", \"the cheaper of those\", \"that white one\" \
all refer to what you already showed. You have the item_ids, so act on them directly rather \
than saying you have lost the list or asking the user to repeat themselves.

Guidance:
- Pick the category that best fits the request and fetch it. If the request spans categories \
(\"something for a bedroom\"), fetch each relevant one.
- \"Cheap\" and \"expensive\" are relative to what that category actually costs. Look at the \
spread of prices you got back and judge against it, rather than applying a fixed threshold.
- Colour lives in each row's `colours` array. Treat near-matches sensibly — \"grey\" should \
consider \"dark grey\", and a request for \"neutral\" covers white, beige, grey and black.
- The shop reports prices as bare numbers and does not say which currency it uses. Write them \
as plain numbers (\"16.00\", \"1,299.00\") and never attach a currency symbol or code — you do \
not know the currency, and guessing one is a factual error.
- If nothing genuinely matches, say so plainly and offer the closest alternatives. Do not \
invent products, prices, or item_ids — every fact you state must come from a tool result.
- Check the balance when the user asks about affordability or mentions a budget.

Your final answer is structured, not prose. Fill in:
- `recommendations`: usually 2-6 specific items, best first. Each needs the exact `item_id` from \
a tool result and one short sentence saying why that item fits this request. The app renders \
these as product cards with pictures and prices, so do NOT repeat the name or price in your \
text — say only why it fits (\"the deepest seat in this price range\", \"folds flat for a small \
room\"). Leave the list empty if genuinely nothing matches.
- `summary`: one or two sentences framing the picks — the judgement you applied, such as what \
counts as cheap in this category. Do not list the items again; the cards do that.";

const PURCHASING: &str = "\n\nBuying: you have no tool that spends money. When the user asks to \
buy something, call propose_furniture_purchase — that prices it and shows them a confirmation \
panel, and nothing is charged until they press Confirm.

Because the confirmation panel is where the user checks your choice, prefer proposing your best \
interpretation over asking them to narrow it down. \"Buy the cheapest stool\" should produce a \
proposal for the stool you judge cheapest, not a question about which one they meant — they can \
read the panel and cancel. Ask only when you genuinely cannot tell which item is meant and the \
candidates differ a lot in price or kind. Default to a quantity of 1 unless they say otherwise.

In your summary, say what they are about to buy and the total, then ask them to reply \"Yes\" \
to confirm (or \"No\" to cancel) — nothing is charged until they do. Never propose something \
the user only asked you to find.";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// How many prior turns to replay. Each is a short digest, not raw tool output,
/// so this is cheap — but it still bounds context growth on a long chat.
const MAX_HISTORY_TURNS: usize = 12;

/// One earlier turn, replayed so the model can resolve "the third one" or
/// "the cheaper of those two".
#[derive(Debug, Deserialize)]
pub struct HistoryTurn {
    /// `user` or `assistant`; anything else is ignored.
    pub role: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct AskRequest {
    pub message: String,
    /// Prior turns, oldest first. The browser holds these — the backend stays
    /// stateless, and the digests carry no tool payloads.
    #[serde(default)]
    pub history: Vec<HistoryTurn>,
    /// Echoed back by the browser when a proposal is on screen, so the next
    /// message can be read as a yes/no about it. Keeps the backend stateless.
    #[serde(default)]
    pub pending: Option<PendingRef>,
}

/// The minimum needed to place a proposed order.
#[derive(Debug, Deserialize)]
pub struct PendingRef {
    pub item_id: String,
    pub quantity: i64,
    /// Minted with the proposal, so a resent "yes" cannot charge twice.
    pub idempotency_key: String,
}

/// An order that was actually placed, after the user confirmed.
#[derive(Debug, Serialize)]
pub struct ConfirmedOrder {
    pub order_id: String,
    pub product_name: String,
    pub quantity: i64,
    pub total_cents: i64,
    pub remaining_balance_cents: i64,
}

/// How a reply to a pending proposal was read.
enum Reply {
    Yes,
    No,
    Unrelated,
}

/// Classifies a reply to a proposal.
///
/// Deliberately deterministic and narrow: whether "yes" means yes is not a
/// judgement call worth delegating to a model when money is at stake. Anything
/// that isn't a clear yes or no is treated as a new request, which drops the
/// proposal — the safe direction to fail.
fn classify_reply(message: &str) -> Reply {
    let cleaned: String = message
        .trim()
        .to_lowercase()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_string();

    match cleaned.as_str() {
        "y" | "yes" | "yeah" | "yep" | "yup" | "ok" | "okay" | "confirm" | "confirmed"
        | "do it" | "go ahead" | "sure" | "buy it" | "place the order" | "please do" => Reply::Yes,
        "n" | "no" | "nope" | "nah" | "cancel" | "stop" | "don t" | "dont" | "no thanks"
        | "never mind" | "nevermind" => Reply::No,
        _ => Reply::Unrelated,
    }
}

/// A purchase the assistant wants to make, priced and waiting on the user.
///
/// **Nothing has been charged when this is returned.** The order is placed by
/// `POST /api/orders` — the same endpoint the catalogue's Buy button uses —
/// and only when the user clicks Confirm.
#[derive(Debug, Serialize)]
pub struct PendingPurchase {
    #[serde(flatten)]
    pub product: CatalogueProduct,
    pub quantity: i64,
    pub total_cents: i64,
    pub balance_cents: i64,
    pub balance_after_cents: i64,
    pub affordable: bool,
    /// Minted server-side with the proposal, so a double-clicked Confirm
    /// returns the original order instead of charging twice.
    pub idempotency_key: String,
}

#[derive(Debug, Serialize)]
pub struct ToolStep {
    pub tool: String,
    pub input: Value,
    /// Short summary — the full payload can be tens of KB, and this is only
    /// for showing the user what the assistant did.
    pub summary: String,
    pub is_error: bool,
}

#[derive(Debug, Serialize)]
pub struct Recommendation {
    /// The product itself, in the same shape the catalogue grid renders — so
    /// the UI shows a real card, image and Buy button, not a line of text.
    #[serde(flatten)]
    pub product: CatalogueProduct,
    /// The model's one-line justification for this pick.
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct AskResponse {
    pub summary: String,
    pub recommendations: Vec<Recommendation>,
    /// Set when the assistant wants to buy something. Nothing is charged until
    /// the user replies "yes".
    pub pending_purchase: Option<PendingPurchase>,
    /// Set only on the turn where a confirmed order actually went through.
    pub order_placed: Option<ConfirmedOrder>,
    /// Digest of this turn, to be replayed as the assistant's `history` entry
    /// on the next request. Rendered here so the format stays server-side.
    pub transcript: String,
    pub steps: Vec<ToolStep>,
    pub model: String,
}

/// Schema for the model's final answer. Structured outputs require every
/// property listed in `required` and `additionalProperties: false`.
fn answer_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "summary": {
                "type": "string",
                "description": "One or two sentences framing the picks and the judgement applied."
            },
            "recommendations": {
                "type": "array",
                "description": "Recommended products, best first. Empty if nothing matches.",
                "items": {
                    "type": "object",
                    "properties": {
                        "item_id": {
                            "type": "string",
                            "description": "Exact item_id from a tool result."
                        },
                        "reason": {
                            "type": "string",
                            "description": "One short sentence on why this item fits. Do not \
    repeat the name or price — the card shows those."
                        }
                    },
                    "required": ["item_id", "reason"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["summary", "recommendations"],
        "additionalProperties": false
    })
}

#[derive(Debug, Deserialize)]
struct ModelAnswer {
    summary: String,
    recommendations: Vec<ModelPick>,
}

#[derive(Debug, Deserialize)]
struct ModelPick {
    item_id: String,
    reason: String,
}

// ---------------------------------------------------------------------------
// Azure OpenAI wire types (only the fields we read)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    model: String,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChatMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ToolCall>,
    /// Populated when the model declines rather than answers.
    #[serde(default)]
    refusal: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolCall {
    id: String,
    function: ToolCallFunction,
}

#[derive(Debug, Deserialize)]
struct ToolCallFunction {
    name: String,
    /// A JSON *string*, not an object — parsed before dispatch.
    #[serde(default)]
    arguments: String,
}

#[derive(Clone)]
pub struct Agent {
    http: reqwest::Client,
    config: AgentConfig,
}

impl Agent {
    pub fn new(config: AgentConfig) -> Result<Self, ApiError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| ApiError::Internal(format!("could not build agent HTTP client: {e}")))?;

        Ok(Self { http, config })
    }

    pub fn is_configured(&self) -> bool {
        !self.config.api_key.is_empty()
    }

    /// Runs the tool loop until the model produces a final answer.
    pub async fn ask(
        &self,
        shop: &ExternalApiClient,
        request: AskRequest,
    ) -> Result<AskResponse, ApiError> {
        if !self.is_configured() {
            return Err(ApiError::Upstream(
                "the assistant is not configured; set AZURE_OPENAI_API_KEY in backend/.env".into(),
            ));
        }

        let message = request.message.trim().to_string();
        if message.is_empty() {
            return Err(ApiError::BadRequest("message is required".into()));
        }

        // A reply to a proposal is handled here, deterministically, before the
        // model is involved at all. The model never decides to spend money —
        // it only ever proposes, and this branch is the only code that buys.
        if let Some(pending) = &request.pending {
            let cancelled = "No problem — I haven't bought anything. Ask me for something else \
                             whenever you're ready.";
            match classify_reply(&message) {
                Reply::Yes => return self.place_confirmed_order(shop, pending).await,
                Reply::No => {
                    return Ok(AskResponse {
                        summary: cancelled.to_string(),
                        recommendations: Vec::new(),
                        pending_purchase: None,
                        order_placed: None,
                        transcript: "The user declined the proposed purchase; nothing was bought."
                            .to_string(),
                        steps: Vec::new(),
                        model: self.config.deployment.clone(),
                    })
                }
                // Neither a yes nor a no: treat it as a fresh request and let
                // the proposal lapse rather than guessing at consent.
                Reply::Unrelated => {}
            }
        }

        let system = format!("{SYSTEM_PROMPT}{PURCHASING}");
        let tool_defs = tools::definitions();

        // The system prompt is the first message here, not a separate field.
        let mut messages = vec![json!({ "role": "system", "content": system })];

        // Replay the conversation so ordinals and pronouns resolve — "the third
        // one", "the cheaper of those". Only the most recent turns, and only
        // digests: raw tool output would be tens of KB per turn.
        let history = request.history;
        let skip = history.len().saturating_sub(MAX_HISTORY_TURNS);
        for turn in history.into_iter().skip(skip) {
            let role = match turn.role.as_str() {
                "user" => "user",
                "assistant" => "assistant",
                _ => continue,
            };
            messages.push(json!({ "role": role, "content": turn.text }));
        }

        messages.push(json!({ "role": "user", "content": message.as_str() }));
        let mut steps: Vec<ToolStep> = Vec::new();
        let mut model_used = self.config.deployment.clone();
        // Every product the model saw, so recommendations resolve to full cards
        // without re-fetching. Populated from the tool results it already read.
        let mut seen: HashMap<String, CatalogueProduct> = HashMap::new();
        // Last proposal, if the model asked to buy something.
        let mut pending: Option<PendingPurchase> = None;

        for _ in 0..MAX_TURNS {
            let response = self.send(&tool_defs, &messages).await?;
            if !response.model.is_empty() {
                model_used = response.model.clone();
            }

            let choice = response
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| ApiError::Upstream("the assistant returned no answer".into()))?;

            // Check for a decline before reading content: on a refusal or a
            // content-filter stop, `content` is null.
            if let Some(refusal) = choice.message.refusal {
                log::warn!("assistant refused: {refusal}");
                return Err(ApiError::Upstream(
                    "the assistant declined to answer that request".into(),
                ));
            }
            if choice.finish_reason.as_deref() == Some("content_filter") {
                return Err(ApiError::Upstream(
                    "that request was blocked by the content filter".into(),
                ));
            }
            // Reasoning plus the answer overran the budget — the JSON answer
            // would be truncated, so fail loudly rather than half-parse it.
            if choice.finish_reason.as_deref() == Some("length") {
                return Err(ApiError::Upstream(
                    "the assistant ran out of room before finishing; try a narrower request".into(),
                ));
            }

            if choice.message.tool_calls.is_empty() {
                return self
                    .finish(
                        shop,
                        choice.message.content.as_deref().unwrap_or_default(),
                        &mut seen,
                        pending,
                        steps,
                        model_used,
                    )
                    .await;
            }

            // Echo the assistant turn back, including every tool_call — the API
            // rejects the next request if a tool result has no matching call.
            messages.push(json!({
                "role": "assistant",
                "content": choice.message.content,
                "tool_calls": choice.message.tool_calls.iter().map(|c| json!({
                    "id": c.id,
                    "type": "function",
                    "function": { "name": c.function.name, "arguments": c.function.arguments },
                })).collect::<Vec<_>>(),
            }));

            for call in &choice.message.tool_calls {
                // Arguments arrive as a JSON string; a malformed one is handed
                // back to the model rather than aborting the whole request.
                let content = match serde_json::from_str::<Value>(&call.function.arguments) {
                    Ok(input) => {
                        // A failed tool goes back to the model as its result —
                        // a bad category is recoverable, and the message says how.
                        let (output, is_error) =
                            match tools::execute(shop, &call.function.name, &input).await {
                                Ok(output) => {
                                    remember_products(&output, &mut seen);
                                    if call.function.name == tools::PROPOSE_TOOL {
                                        pending = read_proposal(&output, &seen);
                                    }
                                    (output, false)
                                }
                                Err(message) => (message, true),
                            };

                        steps.push(ToolStep {
                            tool: call.function.name.clone(),
                            input,
                            summary: summarise(&call.function.name, &output, is_error),
                            is_error,
                        });
                        output
                    }
                    Err(e) => format!("arguments were not valid JSON: {e}"),
                };

                // One `tool` message per call, each keyed by its call id.
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call.id,
                    "content": content,
                }));
            }
        }

        Err(ApiError::Upstream(format!(
            "the assistant did not finish within {MAX_TURNS} steps"
        )))
    }

    /// Places an order the user just confirmed by replying "yes".
    ///
    /// The only code path in the assistant that spends money, and it is plain
    /// Rust — reached from an explicit affirmative against a specific proposal,
    /// never from a model decision.
    async fn place_confirmed_order(
        &self,
        shop: &ExternalApiClient,
        pending: &PendingRef,
    ) -> Result<AskResponse, ApiError> {
        let result = shop
            .place_order(
                &[(pending.item_id.clone(), pending.quantity)],
                Some(pending.idempotency_key.as_str()),
            )
            .await?;

        let total_cents = to_cents(result.total_price);
        let remaining_balance_cents = to_cents(result.remaining_balance);

        // The order response names items by id only, so read the name back for
        // a confirmation the user can actually recognise.
        let product_name = match shop.product(&pending.item_id).await {
            Ok(p) => p.product_name,
            Err(_) => pending.item_id.clone(),
        };

        Ok(AskResponse {
            summary: format!(
                "Done — I've ordered {} × {}. That came to {:.2}, leaving {:.2}.",
                pending.quantity, product_name, result.total_price, result.remaining_balance,
            ),
            recommendations: Vec::new(),
            pending_purchase: None,
            transcript: format!(
                "The user confirmed, and the order went through: {} × {} ({}) for {:.2}. \
                 Their balance is now {:.2}.",
                pending.quantity,
                product_name,
                pending.item_id,
                result.total_price,
                result.remaining_balance,
            ),
            order_placed: Some(ConfirmedOrder {
                order_id: result.order_id,
                product_name,
                quantity: pending.quantity,
                total_cents,
                remaining_balance_cents,
            }),
            steps: vec![ToolStep {
                tool: "place_order".to_string(),
                input: json!({ "item_id": pending.item_id, "quantity": pending.quantity }),
                summary: format!("Placed the order — {:.2} charged", result.total_price),
                is_error: false,
            }],
            model: self.config.deployment.clone(),
        })
    }

    /// Turns the model's structured answer into product cards.
    ///
    /// Picks are resolved from what the model already saw, so no extra upstream
    /// calls in the normal case. An id it invented resolves to nothing and is
    /// dropped rather than rendered as a broken card.
    async fn finish(
        &self,
        shop: &ExternalApiClient,
        text: &str,
        seen: &mut HashMap<String, CatalogueProduct>,
        pending_purchase: Option<PendingPurchase>,
        steps: Vec<ToolStep>,
        model: String,
    ) -> Result<AskResponse, ApiError> {
        let answer: ModelAnswer = serde_json::from_str(text).map_err(|e| {
            log::error!("assistant returned unparseable JSON: {e}; body: {text}");
            ApiError::Upstream("the assistant returned an unexpected response".into())
        })?;

        let mut recommendations = Vec::new();
        for pick in answer.recommendations {
            let product = match seen.get(&pick.item_id) {
                Some(p) => p.clone(),
                // Not in anything it read — look it up before trusting it.
                None => match shop.product(&pick.item_id).await {
                    Ok(p) => {
                        let converted = CatalogueProduct::from(p);
                        seen.insert(pick.item_id.clone(), converted.clone());
                        converted
                    }
                    Err(e) => {
                        log::warn!("assistant recommended unknown item {}: {e}", pick.item_id);
                        continue;
                    }
                },
            };

            recommendations.push(Recommendation {
                product,
                reason: pick.reason,
            });
        }

        let transcript = render_transcript(&answer.summary, &recommendations, &pending_purchase);

        Ok(AskResponse {
            summary: answer.summary,
            recommendations,
            pending_purchase,
            order_placed: None,
            transcript,
            steps,
            model,
        })
    }

    async fn send(
        &self,
        tool_defs: &[Value],
        messages: &[Value],
    ) -> Result<ChatResponse, ApiError> {
        let body = json!({
            // `max_tokens` is rejected with a 400 on this reasoning model. This
            // budget covers reasoning *and* the visible answer, hence the
            // headroom over the answer's actual length.
            "max_completion_tokens": self.config.max_completion_tokens,
            "reasoning_effort": self.config.reasoning_effort,
            // Constrains only the final answer; tool-call turns are unaffected.
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "furniture_recommendations",
                    "strict": true,
                    "schema": answer_schema(),
                }
            },
            "tools": tool_defs,
            "messages": messages,
        });

        let url = format!(
            "{}/openai/deployments/{}/chat/completions?api-version={}",
            self.config.endpoint.trim_end_matches('/'),
            self.config.deployment,
            self.config.api_version,
        );

        let response = self
            .http
            .post(url)
            .header("api-key", &self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                log::error!("assistant request failed: {e}");
                if e.is_timeout() {
                    ApiError::Upstream("the assistant timed out".into())
                } else {
                    ApiError::Upstream("could not reach the assistant".into())
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            log::error!("assistant returned {status}: {body}");
            return Err(match status.as_u16() {
                401 | 403 => ApiError::Upstream(
                    "the assistant rejected this app's API key; check AZURE_OPENAI_API_KEY".into(),
                ),
                404 => ApiError::Upstream(
                    "no such deployment; check AZURE_OPENAI_DEPLOYMENT and AZURE_OPENAI_ENDPOINT"
                        .into(),
                ),
                429 => {
                    ApiError::Upstream("the assistant is rate limited; try again shortly".into())
                }
                _ => ApiError::Upstream(format!("the assistant returned {status}")),
            });
        }

        response.json::<ChatResponse>().await.map_err(|e| {
            log::error!("assistant returned an unreadable body: {e}");
            ApiError::Upstream("the assistant returned an unexpected response".into())
        })
    }
}

/// Harvests products out of a tool result so recommendations can be resolved
/// later without another upstream round trip.
fn remember_products(content: &str, seen: &mut HashMap<String, CatalogueProduct>) {
    let Ok(parsed) = serde_json::from_str::<Value>(content) else {
        return;
    };

    // `search` returns {products: [...]}; `get_product` returns a bare object.
    let rows: Vec<&Value> = match parsed.get("products").and_then(Value::as_array) {
        Some(list) => list.iter().collect(),
        None if parsed.get("item_id").is_some() => vec![&parsed],
        None => return,
    };

    for row in rows {
        let Some(item_id) = row.get("item_id").and_then(Value::as_str) else {
            continue;
        };
        // A proposal result also carries an item_id but prices it under
        // `unit_price`. Without this guard it lands here as a zero-priced
        // phantom and shadows the real product.
        if row.get("price").and_then(Value::as_f64).is_none() {
            continue;
        }

        seen.entry(item_id.to_string())
            .or_insert_with(|| CatalogueProduct {
                item_id: item_id.to_string(),
                product_name: row
                    .get("product_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                price_cents: row
                    .get("price")
                    .and_then(Value::as_f64)
                    .map(to_cents)
                    .unwrap_or_default(),
                category: row
                    .get("category")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                width: row.get("width_cm").and_then(Value::as_f64),
                height: row.get("height_cm").and_then(Value::as_f64),
                depth: row.get("depth_cm").and_then(Value::as_f64),
                colours: row
                    .get("colours")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                // The card loads the photo from /api/products/{id}/image, so
                // there is nothing to carry here.
                image_url: None,
                link: row
                    .get("product_page")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
    }
}

/// Renders a turn into the digest replayed on the next request.
///
/// **Numbered on purpose** — this is what lets "the third one" resolve. Prices
/// and item_ids are included so a follow-up can act on a specific pick without
/// re-searching the catalogue.
fn render_transcript(
    summary: &str,
    recommendations: &[Recommendation],
    pending: &Option<PendingPurchase>,
) -> String {
    let mut out = String::new();

    if !recommendations.is_empty() {
        out.push_str("I showed the user this numbered list:\n");
        for (i, rec) in recommendations.iter().enumerate() {
            out.push_str(&format!(
                "{}. {} — item_id {}, price {:.2}\n",
                i + 1,
                rec.product.product_name,
                rec.product.item_id,
                rec.product.price_cents as f64 / 100.0,
            ));
        }
    }

    if let Some(p) = pending {
        out.push_str(&format!(
            "I proposed buying {} × {} (item_id {}) for a total of {:.2}, awaiting their yes/no.\n",
            p.quantity,
            p.product.product_name,
            p.product.item_id,
            p.total_cents as f64 / 100.0,
        ));
    }

    out.push_str("I said: ");
    out.push_str(summary);
    out
}

/// Reads a proposal out of the propose tool's result.
///
/// Money is converted from upstream's floats to cents here, so the browser only
/// ever sees integers and the confirm panel can't disagree with the card price.
fn read_proposal(
    content: &str,
    seen: &HashMap<String, CatalogueProduct>,
) -> Option<PendingPurchase> {
    let parsed: Value = serde_json::from_str(content).ok()?;
    let item_id = parsed.get("item_id")?.as_str()?;

    // The proposal's own unit price is authoritative: it came from a live
    // product fetch inside the tool. `seen` only enriches the card (category,
    // colours, dimensions) and may not hold this item at all if the model
    // proposed something from an earlier turn's list.
    let unit_price_cents = to_cents(parsed.get("unit_price")?.as_f64()?);
    let mut product = seen
        .get(item_id)
        .cloned()
        .unwrap_or_else(|| CatalogueProduct {
            item_id: item_id.to_string(),
            product_name: parsed
                .get("product_name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            price_cents: unit_price_cents,
            category: None,
            width: None,
            height: None,
            depth: None,
            colours: Vec::new(),
            image_url: None,
            link: None,
        });
    product.price_cents = unit_price_cents;

    let quantity = parsed.get("quantity").and_then(Value::as_i64).unwrap_or(1);
    let balance_cents = to_cents(parsed.get("balance")?.as_f64()?);
    // Derived from the unit price rather than the model's arithmetic.
    let total_cents = unit_price_cents * quantity;

    Some(PendingPurchase {
        quantity,
        total_cents,
        balance_cents,
        balance_after_cents: balance_cents - total_cents,
        affordable: total_cents <= balance_cents,
        idempotency_key: parsed
            .get("idempotency_key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        product,
    })
}

/// One human-readable line per tool call, for the "what it did" trace.
fn summarise(tool: &str, content: &str, is_error: bool) -> String {
    if is_error {
        return content.chars().take(160).collect();
    }

    let parsed: Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(_) => return format!("{tool} completed"),
    };

    match tool {
        tools::SEARCH_TOOL => {
            let count = parsed.get("count").and_then(Value::as_u64).unwrap_or(0);
            match parsed.get("filtered_by_category").and_then(Value::as_str) {
                Some(c) => format!("Looked through {count} items in {c}"),
                None => format!("Looked through all {count} items in the catalogue"),
            }
        }
        tools::PRODUCT_TOOL => format!(
            "Checked {}",
            parsed
                .get("product_name")
                .and_then(Value::as_str)
                .unwrap_or("a product")
        ),
        tools::BALANCE_TOOL => format!(
            "Checked the balance: {}",
            parsed
                .get("balance")
                .map(Value::to_string)
                .unwrap_or_default()
        ),
        tools::PROPOSE_TOOL => format!(
            "Priced up {} — waiting for you to confirm",
            parsed
                .get("product_name")
                .and_then(Value::as_str)
                .unwrap_or("a purchase")
        ),
        other => format!("{other} completed"),
    }
}
