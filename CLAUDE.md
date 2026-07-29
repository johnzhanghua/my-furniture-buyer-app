# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this project is

A buyer-facing web app for a furniture shop, built for a one-day hackathon
(Day 1, Step 5: "Connect Your App to the API").

A buyer logs in, browses the shop's catalogue, and buys items against a real
balance. The loop is: **log in → browse → click Buy → balance goes down.**

## The most important thing to know

**The furniture shop API is the source of truth for catalogue, balance, and
orders. Our SQLite database holds login accounts and nothing else.**

There is no local product table in use, no local budget, no local order
history, and no local stock. Earlier versions of this app enforced a budget in
a local transaction; that is gone. If you find yourself writing money logic in
this codebase, stop — it belongs upstream.

Upstream: `https://day1.training.cognitivo.com.au`, authenticated with a single
`X-Api-Key` header. Full spec at `/openapi.json`, human docs at `/docs`.

### One upstream account, many local logins

The app holds one participant `user_id` and one API key. Every local login
shares that one upstream balance and order history — signing in as a different
local user does not give you a different wallet. This is a lab constraint, not
a design goal; don't build per-user budgets on top of it.

## Repository layout

```
backend/                  Rust API (Actix-web + SQLx + SQLite)
  migrations/             schema for local accounts; applied on boot
  src/
    main.rs               bootstrap, CORS, route registration
    config.rs             env-driven config, incl. upstream credentials
    state.rs              AppState: pool + config + upstream client
    db.rs                 pool setup, migrations, demo-account seeding
    auth.rs               Argon2 hashing, JWT, AuthUser extractor
    error.rs              ApiError + status/code mapping
    external_api.rs       ★ the furniture shop client — all upstream calls
    models.rs             local account types + upstream-backed DTOs
    routes/               auth.rs, me.rs, products.rs, orders.rs, upstream.rs
frontend/                 React + TypeScript SPA (Vite)
  src/api/                typed fetch client + shared types
  src/state/              AuthContext (session + balance)
  src/components/         Layout, ProtectedRoute, ProductCard
  src/pages/              Login, Catalog, Orders
```

## Running it

```bash
cd backend  && cp .env.example .env   # then add the real credentials
cargo run                             # http://127.0.0.1:8080

cd frontend && cp .env.example .env
npm install && npm run dev            # http://localhost:5173
```

Local demo login: `buyer@example.com` / `password123`. That is a *login*, not a
wallet — the balance shown comes from upstream.

Checks: `cargo check && cargo clippy && cargo fmt` · `npm run typecheck && npm run build`

## API surface

All routes under `/api`. Authenticated routes need `Authorization: Bearer <jwt>`.

| Method | Path | Auth | Backed by |
| --- | --- | --- | --- |
| POST | `/api/auth/register` | no | local DB |
| POST | `/api/auth/login` | no | local DB |
| GET | `/api/me` | yes | local DB (identity only) |
| GET | `/api/me/balance` | yes | upstream `GET /users/{user_id}` |
| GET | `/api/categories` | no | upstream `GET /catalogue/categories` |
| GET | `/api/products?search=&category=` | no | upstream `GET /catalogue/search-index` |
| GET | `/api/products/{item_id}` | no | upstream `GET /catalogue/{item_id}` |
| GET | `/api/products/{item_id}/image` | no | upstream `GET /catalogue/{item_id}/image` (JPEG bytes) |
| POST | `/api/orders` | yes | upstream `POST /orders` |
| GET | `/api/orders` | yes | upstream `GET /orders/{user_id}` |
| GET | `/api/upstream/status` | yes | diagnostic |
| GET | `/api/health` | no | liveness |

Errors are always `{ "error": "<code>", "message": "<text>" }`. Codes:
`bad_request`, `unauthorized`, `invalid_credentials`, `not_found`, `conflict`,
`insufficient_balance`, `upstream_error`, `internal`.

## The shopping assistant

`POST /api/assistant/ask` takes a plain-English request and returns a
recommendation. The text box lives at the top of the catalogue page
(`AssistantBox.tsx`). Backed by `backend/src/agent.rs` — Rust has no Anthropic
SDK, so it drives the Messages API over raw HTTP.

**The judgement split is the whole point.** The shop's API filters by exact
category and nothing else — no price, colour, size, or free-text search. So the
tools return plain rows and the *model* does the qualitative work: it fetches a
category, then decides what counts as "cheap", "white", or "right for a small
flat". The system prompt says this explicitly, including that "cheap" is
relative to what that category actually costs rather than a fixed threshold.
Don't try to push this reasoning into the API — it isn't there.

**The assistant cannot spend money. At all.** There is no tool that places an
order, and `tools::execute` has no branch that could — a hallucinated tool name
resolves to "no such tool". Buying works in two phases:

1. The model calls `propose_furniture_purchase`, which prices the item against
   the live balance and returns `pending_purchase` on the response. **Nothing
   is charged.** The UI shows item, quantity, unit price, total and balance
   after, and asks the user to reply.
2. The user types **Yes** (or `y`, `ok`, `confirm`, …). The browser echoes the
   proposal back as `pending`, and `Agent::place_confirmed_order` places the
   order — plain Rust, reached only from an explicit affirmative.

`classify_reply` decides yes/no **deterministically**, before the model is
consulted. Whether "yes" means yes is not a judgement worth delegating to a
model when money is at stake. Anything that is neither a clear yes nor no is
treated as a fresh request and the proposal lapses — the safe direction to
fail. Keep that property.

The `idempotency_key` is minted server-side with the proposal and echoed back
with the confirmation, so a resent "yes" returns the original order rather than
charging twice. Totals are recomputed from the product's own price in cents,
never from the model's arithmetic.

The backend is stateless across turns: the browser carries the proposal, which
is no weaker than it sounds — the caller is already authenticated and could
call `POST /api/orders` directly.

### Conversation memory

Follow-ups like "the third one" or "buy the cheapest of those" work because the
browser replays prior turns in `history`. Two rules keep this cheap and correct:

- **Replay digests, never tool payloads.** Each assistant turn is rendered
  server-side into `transcript` — a *numbered* list of what was shown, with
  item_ids and prices, plus the summary. The numbering is what makes ordinals
  resolve. A full search result is ~30K tokens; a digest is a few hundred.
- **`MAX_HISTORY_TURNS` (12)** bounds growth on a long chat. "Start over"
  clears it.

Watch out when touching `read_proposal` / `remember_products`: a proposal
result also carries an `item_id`, but prices it under `unit_price` rather than
`price`. Without the guard in `remember_products` it lands in the product cache
as a **zero-priced phantom** and the confirmation panel shows a 0.00 total.
The proposal's own `unit_price` is authoritative — `seen` only enriches the
card, and may not hold the item at all when the model picks from an earlier
turn's list.

Because confirmation is mandatory, the prompt tells the model to *prefer
proposing over asking* — "buy the cheapest stool" should produce a proposal,
not a clarifying question, since the panel is where the user corrects it.
Keep that property if you touch the prompt: an assistant that asks first and
then still needs a confirmation click is two round trips for nothing.

**Provider: Azure OpenAI**, deployment `gpt-5-mini`, Chat Completions over raw
HTTP. Specifics worth keeping:

- URL is `{endpoint}/openai/deployments/{deployment}/chat/completions?api-version=…`
  and auth is an `api-key` header. The **deployment name**, not a `model`
  field, selects the model.
- It's a reasoning model: **`max_tokens` is rejected with a 400**. Use
  `max_completion_tokens`, which covers reasoning *plus* the visible answer —
  hence the headroom. Depth is tuned with `reasoning_effort`
  (`minimal`/`low`/`medium`/`high`), not a token budget.
- **Tool-call arguments arrive as a JSON string**, not an object — parse before
  dispatch. A malformed one goes back to the model rather than aborting.
- The assistant turn must be echoed back **with its `tool_calls`**, then one
  `{"role":"tool", "tool_call_id": …}` message per call. A tool result with no
  matching call is rejected.
- `finish_reason` is checked before reading content: `content_filter` and
  `refusal` leave it null, and `length` means the JSON answer is truncated —
  all three fail loudly rather than half-parsing.
- Structured output uses `response_format: {type: "json_schema", strict: true}`,
  which requires every property in `required` plus
  `additionalProperties: false`. The **tool** schemas are deliberately *not*
  strict — that would force `category` and `limit` to be mandatory and remove
  the "search everything" option.
- The prompt forbids attaching a currency symbol — upstream returns bare
  numbers and names no currency, and models otherwise invent one.

`MAX_TURNS` (8) bounds the tool loop. A failed tool result is handed back to
the model with `is_error: true` rather than aborting, so a bad category
self-corrects from the error message.

## MCP server — the shop as agent tools

`backend/src/bin/mcp.rs` exposes four furniture-shop operations as MCP tools
over stdio, built on `rmcp` 3. It shares the crate's library with the HTTP API,
so both drive the same `ExternalApiClient` and cannot drift.

```bash
cd backend && cargo build --bin furniture-mcp
```

Register it with an MCP client (absolute paths required; credentials come from
`backend/.env`):

```json
{
  "mcpServers": {
    "furniture-shop": {
      "command": "/absolute/path/to/backend/target/debug/furniture-mcp",
      "cwd": "/absolute/path/to/backend"
    }
  }
}
```

| Tool | Notes |
| --- | --- |
| `search_furniture_catalogue` | Exact category filter only — no text/price/colour search |
| `get_furniture_product` | Lookup by `item_id`; not a search |
| `check_shop_balance` | No parameters — one account, fixed by the API key |
| `place_furniture_order` | **Spends real money, irreversible** |

Three rules for anyone editing these tools:

**Never let a tool fail silently.** Upstream returns `[]` for an unrecognised
category, which a model reports to the user as "the shop has none". Categories
are validated in `resolve_category` against the live list, and a miss returns an
error naming every valid value — a wrong guess becomes loud and self-correcting.
Preserve that property in any new filter.

**Never put the product image in a tool result.** `get_furniture_product`
returns `has_image: true`, not the ~60KB base64 blob upstream sends. The whole
response is ~260 characters; passing the raw field through would be ~84KB of
context per product.

**Consequences belong in the tool description, not the host's system prompt.**
`place_furniture_order` states that it spends real money and cannot be undone,
because the description is what the model reads at the moment it decides to
call. Same for the "no free-text search" limitation on the catalogue tool.

`stdout` is the JSON-RPC transport — logs go to **stderr** (`main` pins
`env_logger` to `Target::Stderr`). A stray `println!` corrupts the stream.

## Conventions that matter

**Money is integer cents everywhere in this codebase.** Upstream speaks
floating-point (`398.0`, `5000.0`). Conversion happens in exactly one place —
`external_api::to_cents` — at the boundary, and only inbound. Order requests
carry `item_id` and `quantity` only, so no amount is ever sent back upstream
and there is no round trip to drift. Never let an `f64` past `external_api.rs`,
and never introduce a float or decimal string for money in the frontend.

**Browse with `search-index`, not `/catalogue`.** The guide is explicit: the
plain catalogue endpoint embeds images and is much slower. `search-index`
supports `category`, `limit`, `skip` — but **no text search**, so free-text
filtering happens in `routes/products.rs` after fetching. That is only viable
because the catalogue is 762 items and upstream's `limit` cap is 1000. If it
outgrows that, this needs real pagination.

**Product photos come from a proxy route, not from the listing.**
`search-index` returns `image_url: null` for all 762 products — the field
exists but is never populated, since that endpoint is the deliberately
lightweight one. So the grid cannot render images from the listing payload.

Instead `GET /api/products/{item_id}/image` proxies upstream
`GET /catalogue/{item_id}/image`, which serves real JPEG bytes (1400×1400,
~45 KB). The browser points `<img src>` straight at it, with `loading="lazy"`
so only visible cards fetch. Upstream currently serves that endpoint
*unauthenticated*, but going through our backend keeps the browser on one
origin and means nothing breaks if that changes. Responses carry
`Cache-Control: public, max-age=86400`.

Separately, `GET /catalogue/{item_id}` (the JSON detail endpoint) puts raw
base64 image data in `image_url` — despite the name, it is not a URL.
`CatalogueProduct::from` wraps it into a data URI so it is at least usable.

**Every upstream failure maps through `external_api::map_error`.** That is the
single place upstream status codes become ours: `402` → `insufficient_balance`,
`404` → "this item is no longer available", `401/403` → `upstream_error`
(our key is wrong — never surfaced as the *user's* auth problem). Add new
mappings there, not in handlers.

**The upstream client does not follow redirects.** A `3xx` from an API usually
means auth failed and you're being sent to a login page; following it disguises
that as `200 OK` carrying HTML.

**Ordering is idempotent by key.** The browser generates an idempotency key per
buy intent and the backend forwards it as `Idempotency-Key`; upstream returns
the original result rather than charging twice. The Buy button is also disabled
while its request is in flight. Both halves matter — keep them.

**Auth**: Argon2id hashes, HS256 JWTs, 24h expiry. The `AuthUser` extractor is
the only way a handler learns who is calling.

## Frontend notes

- Routing via `react-router-dom`; `ProtectedRoute` redirects to `/login`.
- Token in `localStorage` under `fb.token`; a `401` from any call clears it.
- **There is no cart.** Step 5 specifies a per-product Buy button, so ordering
  is one product at a time. The backend's order endpoint takes a single item;
  upstream itself accepts multiple lines if a cart is ever reinstated.
- Balance lives in `AuthContext`. A successful order returns
  `remaining_balance_cents`, which is applied directly rather than triggering
  another round trip.
- Buy failures are phrased in `buyErrorMessage` in `CatalogPage.tsx` — the one
  place user-facing error wording lives. It switches on the error *code*, never
  on message text.
- No component library, no CSS framework, no data-fetching library. Keep it
  that way unless asked.

## Local database

Only `users` is read or written. The `products`, `orders`, and `order_items`
tables still exist from the pre-integration design and are **dead** — nothing
queries them, and migration `…0002` still seeds a local catalogue that nothing
reads. Left in place rather than dropped destructively; remove them with a new
migration if the clutter becomes a problem. Never edit an applied migration —
SQLx checksums them on boot.

## Hackathon guardrails

- Don't reintroduce local budget or stock enforcement.
- Don't add Postgres, Docker, a queue, or a payment provider.
- Don't guess upstream endpoints — read `/openapi.json`.
- Secrets come from `.env` (gitignored). `.env.example` is committed and must
  stay in sync when a variable is added.
