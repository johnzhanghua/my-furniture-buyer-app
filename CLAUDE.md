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
| POST | `/api/orders` | yes | upstream `POST /orders` |
| GET | `/api/orders` | yes | upstream `GET /orders/{user_id}` |
| GET | `/api/upstream/status` | yes | diagnostic |
| GET | `/api/health` | no | liveness |

Errors are always `{ "error": "<code>", "message": "<text>" }`. Codes:
`bad_request`, `unauthorized`, `invalid_credentials`, `not_found`, `conflict`,
`insufficient_balance`, `upstream_error`, `internal`.

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

**`image_url` from `/catalogue/{item_id}` is not a URL.** It is raw base64
image data, with the type in `image_mime_type`. `CatalogueProduct::from` wraps
it into a data URI. `search-index` returns null there, so listings have no
images.

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
