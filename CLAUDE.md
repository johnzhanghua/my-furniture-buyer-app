# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this project is

A buyer-facing web app for a furniture shop, built for a one-day hackathon.

The core loop is deliberately small:

1. A buyer **logs in**.
2. They **browse a product catalogue** (search + filter by category).
3. They add items to a cart and **place an order against a spending budget**.

Every buyer has a fixed budget. An order is rejected if its total would push the
buyer over their remaining budget, or if any line item exceeds available stock.
Budget enforcement is the central business rule — it lives in the backend and is
only *mirrored* in the UI for feedback.

## Repository layout

```
my-furniture-buyer-app/
├── CLAUDE.md            ← this file
├── backend/             ← Rust API (Actix-web + SQLx + SQLite)
│   ├── migrations/      ← SQLx migrations, applied automatically on boot
│   └── src/
│       ├── main.rs      ← server bootstrap, CORS, route registration
│       ├── config.rs    ← env-driven configuration
│       ├── state.rs     ← AppState (pool + config), shared via web::Data
│       ├── db.rs        ← pool setup, migration run, demo-user seeding
│       ├── auth.rs      ← Argon2 hashing, JWT encode/decode, AuthUser extractor
│       ├── error.rs     ← ApiError + ResponseError impl (uniform JSON errors)
│       ├── models.rs    ← DB row structs and API DTOs
│       └── routes/      ← auth.rs, products.rs, orders.rs, me.rs
└── frontend/            ← React + TypeScript SPA (Vite)
    └── src/
        ├── api/         ← typed fetch client + shared API types
        ├── state/       ← AuthContext, CartContext
        ├── components/  ← Layout, ProtectedRoute, ProductCard
        ├── pages/       ← Login, Catalog, Cart, Orders
        └── lib/         ← formatting helpers
```

## Running it

Prerequisites: Rust (stable, via rustup) and Node 20+. **Neither is currently
installed on this machine** — install both before attempting to build.

```bash
# terminal 1 — API on http://127.0.0.1:8080
cd backend
cp .env.example .env
cargo run

# terminal 2 — UI on http://localhost:5173
cd frontend
cp .env.example .env
npm install
npm run dev
```

The SQLite file (`backend/furniture.db`) is created on first run, migrations are
applied, and a demo buyer is seeded:

- email `buyer@example.com`
- password `password123`
- budget `$5,000.00`

Vite proxies `/api` to the backend, so the browser sees a single origin in dev.

Useful commands:

```bash
cd backend  && cargo check && cargo clippy && cargo fmt
cd frontend && npm run typecheck && npm run build
```

## API surface

All routes are under `/api`. Authenticated routes require
`Authorization: Bearer <jwt>`.

| Method | Path                 | Auth | Purpose                                  |
| ------ | -------------------- | ---- | ---------------------------------------- |
| POST   | `/api/auth/register` | no   | Create a buyer, returns token + user      |
| POST   | `/api/auth/login`    | no   | Exchange credentials for a JWT            |
| GET    | `/api/me`            | yes  | Current user profile                      |
| GET    | `/api/me/budget`     | yes  | `{ budget, spent, remaining }` in cents   |
| GET    | `/api/products`      | no   | Catalogue; `?search=` and `?category=`    |
| GET    | `/api/products/{id}` | no   | Single product                            |
| POST   | `/api/orders`        | yes  | Place an order (budget + stock enforced)  |
| GET    | `/api/orders`        | yes  | Current user's orders, newest first       |
| GET    | `/api/orders/{id}`   | yes  | One order, scoped to the current user     |
| GET    | `/api/health`        | no   | Liveness probe                            |

Errors are always JSON: `{ "error": "<machine_code>", "message": "<human text>" }`.
Codes in use: `bad_request`, `unauthorized`, `invalid_credentials`, `not_found`,
`conflict`, `insufficient_budget`, `insufficient_stock`, `internal`.

## Conventions that matter

**Money is always integer cents (`i64` / `number`), never floats.** Field names
carry the unit — `price_cents`, `total_cents`, `remaining_cents`. Convert to a
display string only at the very edge, via `formatCents` in
`frontend/src/lib/format.ts`. Do not introduce a float or a decimal string for
money anywhere in the stack.

**IDs are UUID v4 stored as `TEXT`.** Timestamps are RFC 3339 strings in UTC,
also `TEXT`. This keeps SQLite type mapping trivial and makes lexicographic
sorting on `created_at` correct.

**SQLx runtime queries only — no `query!`/`query_as!` macros.** The compile-time
macros need a live `DATABASE_URL` at build time, which breaks offline builds and
CI. Use `sqlx::query_as::<_, T>(...)` with `#[derive(FromRow)]` and `.bind(...)`.

**Order placement runs in a single transaction** (`routes/orders.rs`). It reads
budget, sums prior non-cancelled orders, validates every line against stock and
price, inserts the order and its items, and decrements stock — then commits.
Any new invariant about ordering belongs inside that transaction, not in a
handler prelude and definitely not in the frontend.

**Never trust client-supplied prices.** The request body carries only
`product_id` and `quantity`; unit prices are re-read from the database and
snapshotted onto `order_items.unit_price_cents` so historical orders survive
price changes.

**Auth**: Argon2id password hashes, HS256 JWTs with a 24h expiry. The
`AuthUser` extractor (`auth.rs`) is the only way handlers should learn who the
caller is — never read a user id from a path or body parameter.

## Frontend notes

- Routing via `react-router-dom`; `ProtectedRoute` redirects unauthenticated
  users to `/login` and remembers the attempted path.
- Auth token is persisted in `localStorage` under `fb.token` and installed into
  the API client on boot. A `401` from any call clears it and bounces to login.
- Cart lives in `CartContext`, persisted to `localStorage` under `fb.cart`. It
  is purely client-side until `POST /api/orders`.
- No component library and no CSS framework — a single `index.css` with plain
  classes. Keep it that way unless the task explicitly asks otherwise; adding a
  design system mid-hackathon is a poor trade.
- State is React context + `fetch`. There is no TanStack Query / Redux, and
  adding one is a deliberate decision, not a drive-by refactor.

## Hackathon guardrails

This is a one-day build. When choosing between options, prefer the one that
keeps the demo loop working. Concretely:

- Don't swap SQLite for Postgres, add Docker, or introduce a message queue.
  SQLx keeps a Postgres migration cheap *later* if it's ever needed.
- Don't add a payment provider, shipping, or inventory reconciliation. Stock is
  a plain counter.
- SQLite is single-writer; concurrent checkout under load is out of scope.
- Auth is intentionally minimal: no refresh tokens, no password reset, no email
  verification. Say so rather than quietly building them.
- Secrets come from `.env` (`JWT_SECRET`). `.env` is gitignored; `.env.example`
  is committed and must stay in sync when a new variable is introduced.
