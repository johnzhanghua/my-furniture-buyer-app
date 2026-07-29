# Architecture

How the furniture buyer's app is put together, and why. Requirement IDs (FR-…,
NFR-…, BR-…, LIM-…) refer to [requirements.md](requirements.md).

---

## 1. System context

```
┌──────────────┐   HTTPS/JSON     ┌──────────────────┐   SQL    ┌────────────┐
│   Browser    │  ─────────────▶  │   Rust API       │ ───────▶ │  SQLite    │
│  React SPA   │  ◀─────────────  │   Actix-web      │ ◀─────── │  file      │
└──────────────┘   Bearer JWT     └──────────────────┘          └────────────┘
   Vite :5173                          :8080                    furniture.db
```

Two deployable pieces and one file. There is no cache, queue, object store, or
third-party service — deliberately (NFR-5).

In development the Vite dev server proxies `/api` to `127.0.0.1:8080`, so the
browser sees one origin and no preflight. In any other topology the API's
`CORS_ALLOWED_ORIGINS` allowlist governs access (NFR-8).

### Trust boundary

The boundary is the HTTP edge. **Everything the browser sends is untrusted** —
including prices, budgets, and user identity. The SPA's budget arithmetic and
disabled buttons are conveniences for the person using it, not controls. Every
rule in BR-1…BR-6 is enforced again server-side, and the acceptance script
(requirements §5, steps 9–12) tests exactly that by bypassing the UI.

---

## 2. Backend

### 2.1 Module layout

`backend/src/`, in dependency order — nothing lower depends on anything higher:

| Module | Responsibility |
| --- | --- |
| [`config.rs`](backend/src/config.rs) | Reads every tunable from the environment with a dev default. The only place `std::env` is touched. |
| [`error.rs`](backend/src/error.rs) | `ApiError`, the one error type handlers return, and its `ResponseError` impl — the single place a status code and wire format are chosen. |
| [`models.rs`](backend/src/models.rs) | Database row structs (`FromRow`) and request/response DTOs. Rows and DTOs are separate types on purpose (see DD-8). |
| [`state.rs`](backend/src/state.rs) | `AppState { pool, config }`, shared via `web::Data`. |
| [`db.rs`](backend/src/db.rs) | Pool construction, PRAGMA setup, migration run, idempotent demo seed. |
| [`auth.rs`](backend/src/auth.rs) | Argon2id hashing, JWT issue/verify, and the `AuthUser` extractor. |
| [`routes/`](backend/src/routes/) | HTTP handlers, one module per resource, plus route registration in `mod.rs`. |
| [`main.rs`](backend/src/main.rs) | Composition root: load env → open DB → seed → build middleware → serve. |

There is no service layer. At this size it would be a pass-through; handlers own
their transactions directly. If a second caller of the ordering logic ever
appears, that is the moment to extract one — not before.

### 2.2 Authentication

```
POST /api/auth/login  ──▶ look up by lowercased email
                          Argon2id verify  ──▶ HS256 JWT { sub, email, exp: now+24h }

Any protected route  ──▶ AuthUser extractor
                          Authorization: Bearer <jwt>  ──▶ verify signature + exp
                                                       ──▶ AuthUser { id, email }
```

`AuthUser` implements Actix's `FromRequest`, so **adding the parameter to a
handler is what makes a route protected** — there is no separate middleware
registration to forget, and no route can accidentally read an identity from a
path or body parameter instead (BR-6).

Verification is a signature check only; it does not hit the database. That keeps
protected reads to one query, at the cost of LIM-4 (a deleted or altered account
stays usable until the token expires). Handlers that need live user data — `/me`,
`/me/budget`, and order placement — re-read the row and treat a missing user as
`401`.

**Failure parity (FR-1.4):** a missing user and a bad password both return
`ApiError::InvalidCredentials`. Note the asymmetry this creates — the no-user
branch skips Argon2 verification and so returns measurably faster. That timing
side channel is accepted here and would be closed with a dummy verify.

### 2.3 Order placement — the one piece of real logic

Everything else in the backend is CRUD. This is the part that carries the
product's meaning, and it lives in exactly one function:
[`routes/orders.rs::create`](backend/src/routes/orders.rs).

```
merge duplicate product lines                     ← FR-4.8, so BR-1/BR-2 see true totals
BEGIN
  read budget_cents        FROM users
  read SUM(total_cents)    FROM orders WHERE status <> 'cancelled'
  remaining = budget − spent
  for each line:
      read product (price + stock) FROM products
      if stock < quantity            → InsufficientStock  ⇢ rollback   ← BR-2
      total += price × quantity                                        ← BR-4
  if total > remaining               → InsufficientBudget ⇢ rollback   ← BR-1
  INSERT order
  for each line:
      INSERT order_item (snapshotting unit_price_cents)                ← BR-5
      UPDATE products SET stock = stock − q WHERE id = ? AND stock >= q
      if rows_affected = 0           → InsufficientStock  ⇢ rollback
COMMIT
```

Four things about this shape are load-bearing:

1. **One transaction.** Every early return drops the `Transaction`, which rolls
   back on drop. Atomic rejection (BR-3, FR-4.7) is therefore the default
   behaviour rather than something each error path has to remember.
2. **Duplicate lines are merged first.** Two lines of 3 chairs against 5 in
   stock must be read as 6, not as two independent 3s. Same for the budget.
3. **The decrement re-checks stock.** `WHERE ... AND stock >= ?` with a
   `rows_affected == 0` check makes the write itself the authority, so the
   read-then-write window cannot drive stock negative even if the isolation
   guarantee were weaker than SQLite's.
4. **Prices come from the database.** The request DTO has no price field at all
   (NFR-4) — the client cannot name its own price because there is nowhere to
   put one.

**Any new ordering invariant belongs inside this transaction**, not in a
validation prelude and never in the frontend.

### 2.4 Error model

`ApiError` maps to both a status and a stable machine code (NFR-2):

| Variant | Status | `error` code |
| --- | --- | --- |
| `BadRequest` | 400 | `bad_request` |
| `Unauthorized` / `InvalidCredentials` | 401 | `unauthorized` / `invalid_credentials` |
| `NotFound` | 404 | `not_found` |
| `Conflict` | 409 | `conflict` |
| `InsufficientBudget` | 422 | `insufficient_budget` |
| `InsufficientStock` | 422 | `insufficient_stock` |
| `Database` / `Internal` | 500 | `internal` |

Body is always `{ "error": "<code>", "message": "<human text>" }`.

422 rather than 400 for the two business-rule failures: the request was
syntactically valid and well-formed, it just isn't allowed. The distinction lets
the SPA show a budget message differently from a validation message without
parsing prose.

`Database` and `Internal` implement `Display` as the deliberately useless
`"something went wrong"`; the real cause is logged in `error_response` and never
crosses the wire (NFR-3).

### 2.5 Persistence

Opened with `create_if_missing`, `foreign_keys(true)`, and WAL journalling; five
pooled connections.

```
users ──< orders ──< order_items >── products
```

| Table | Notes |
| --- | --- |
| `users` | `budget_cents INTEGER`, unique lowercase `email`, Argon2 `password_hash`. |
| `products` | `price_cents`, `stock`, `category`, unique `sku`. |
| `orders` | `user_id`, `total_cents`, `status` (`placed`; `cancelled` reserved — LIM-2), `created_at`. |
| `order_items` | `quantity` and **`unit_price_cents`, a snapshot** — this column is why BR-5 holds. |

Indexes: `orders(user_id, created_at DESC)` serves the history query directly;
`order_items(order_id)` serves the item join; `products(category)` serves the
filter.

Conventions: IDs are UUID v4 as `TEXT`; timestamps are RFC 3339 UTC as `TEXT`,
which sorts lexicographically in the correct order, so `ORDER BY created_at DESC`
needs no date parsing.

Migrations in `backend/migrations/` run automatically on boot — schema first,
then the catalogue seed via `INSERT OR IGNORE`. The demo *user* is seeded from
Rust instead, because its password needs a real Argon2 hash rather than a
literal baked into SQL (NFR-6).

---

## 3. Frontend

### 3.1 Structure

```
main.tsx        BrowserRouter → AuthProvider → CartProvider → App
App.tsx         routes; everything but /login sits behind ProtectedRoute
api/client.ts   typed fetch wrapper, token handling, ApiError
state/          AuthContext (session + budget) · CartContext (cart)
components/     Layout (nav + budget bar) · ProtectedRoute · ProductCard
pages/          Login · Catalog · Cart · Orders
lib/format.ts   formatCents / formatDate
```

### 3.2 API client

One `request<T>` helper owns the cross-cutting concerns so no component deals
with them: base URL, JSON headers, bearer injection, `ApiError` construction
from the `{ error, message }` body, and a network-failure case that says "is the
API running?" rather than surfacing a raw `TypeError`.

A `401` from *any* call clears the stored token and fires the unauthorized
handler that `AuthProvider` registers, which drops the user out of state; the
next render sends `ProtectedRoute` to `/login`. Session expiry is therefore
handled in one place regardless of which request discovered it.

`api` is a flat object of named methods — the only module that knows endpoint
paths exist.

### 3.3 State

Two contexts, no state library (DD-9).

**`AuthContext`** holds `user`, `budget`, and `loading`. On boot, if a token is
present it fetches `/me` and `/me/budget` in parallel and discards the token if
either fails. `loading` gates route rendering so a reload on `/orders` doesn't
flash the login screen before the session resolves. `refreshBudget()` is called
after checkout (FR-5.5); it swallows its own errors, since a stale budget
figure is not worth blocking the UI over.

**`CartContext`** holds lines of `{ product, quantity }`, mirrored to
`localStorage` on every change (FR-3.2). Quantities are clamped to
`product.stock` on both `add` and `setQuantity`, and setting zero removes the
line. The cached `Product` is a display snapshot only — see DD-7.

### 3.4 The budget, shown three ways

The same figure appears with three different jobs, which is worth being explicit
about because only one of them is a control:

| Where | Purpose |
| --- | --- |
| Header bar (`Layout`) | Ambient awareness — remaining vs total, with a fill bar. |
| Per-product `affordable` flag (`Catalog`) | Prevents adding what cannot be paid for; uses remaining **minus the current cart**, so the cart's own contents count. |
| Checkout panel (`Cart`) | Shows the post-order position and disables the button when over. |

All three are advisory. The API is asked, and may still say no — in which case
its `message` is surfaced verbatim rather than being re-derived client-side,
because the server's arithmetic is the one that counts (FR-5.4).

---

## 4. Request flows

**Sign in**

```
LoginPage → AuthContext.login → POST /api/auth/login
  → setToken(localStorage) → setUser → GET /api/me/budget
  → navigate to the remembered destination (or /catalog)
```

**Browse**

```
CatalogPage → 250 ms debounce on search/category
  → GET /api/products?search=&category=
  (separate unfiltered fetch on mount populates the category dropdown, so it
   doesn't collapse to one option once a filter is applied)
```

**Checkout**

```
CartPage → POST /api/orders { items: [{ product_id, quantity }] }
  ├─ 201 → clear cart → refreshBudget → navigate /orders
  └─ 422 → show the server's message; cart is left untouched for editing
```

The request carries only ids and quantities — the smallest payload that can
express the intent, and one that makes NFR-4 structural rather than a rule
someone has to follow.

---

## 5. Design decisions

**DD-1 — Money as integer cents everywhere (NFR-1).** `i64` in Rust, `number` in
TypeScript, `INTEGER` in SQLite. Field names carry the unit (`price_cents`,
`remaining_cents`) so a bare `price` is visibly wrong in review. Conversion to a
display string happens in exactly one function, `formatCents`. Binary floats
cannot represent `0.10`; a budget app that drifts by a cent is a broken budget
app. `i64` cents overflows past $92 quadrillion, which is sufficient.

**DD-2 — SQLite over Postgres.** No container, no daemon, no connection string
to coordinate; deleting one file resets the world. Costs: single writer (LIM-7),
weak column typing. SQLx speaks both, and the schema uses nothing exotic, so a
port stays cheap if it is ever needed.

**DD-3 — SQLx runtime queries, not the `query!` macros.** The compile-time
macros need a live `DATABASE_URL` at build time, which breaks offline builds and
CI bootstrap. The cost is real and worth naming: **column/struct mismatches
become runtime errors instead of compile errors**, which is exactly the class of
bug NFR-9's absent tests would otherwise catch. Column lists are held in
`const`s per module to limit the drift.

**DD-4 — JWT over server-side sessions.** Stateless, no session store, trivial
to test with `curl`. Buys LIM-4 (no revocation) and LIM-5 (`localStorage`
exposure). For a 24-hour demo the trade is fine; for anything real, an
httpOnly-cookie session with server-side revocation is the correct design.

**DD-5 — Argon2id, despite the demo scope.** Password hashing is the one thing
where a shortcut taken on Day 1 becomes a genuine liability the moment real
credentials touch the system. Default parameters, per-password salt.

**DD-6 — The catalogue is public (FR-2.5).** A shop's products are not secret,
and it keeps the browse path free of auth complexity. Orders, budget, and
profile all require a token.

**DD-7 — The cart is client-side only.** No server round-trip per quantity
change, no abandoned-cart rows, no cart-vs-order sync bug. The consequence:
cached prices and stock go stale, and the app leans on the checkout call to
correct them. That is precisely why BR-4 re-prices server-side.

**DD-8 — Row structs are separate from response DTOs.** `User` carries
`password_hash`; `UserResponse` cannot. Keeping them distinct means a secret
cannot reach a JSON body by someone adding a `Serialize` derive to a row struct.

**DD-9 — React Context and `fetch`, no data-fetching library.** Four screens and
six endpoints do not amortise the concept count of TanStack Query or Redux. The
gap this leaves is visible in `CatalogPage`: hand-rolled debounce, cancellation
flags, and no caching between navigations. Introducing one is a deliberate
decision to take at a specific moment, not a drive-by refactor.

**DD-10 — Hand-written CSS, no component library.** One `index.css` of plain
classes, CSS custom properties for the palette. Adding a design system mid-build
costs more than the four screens are worth.

---

## 6. Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `DATABASE_URL` | `sqlite://furniture.db` | Database file; created on first boot. |
| `HOST` / `PORT` | `127.0.0.1` / `8080` | API bind address. |
| `JWT_SECRET` | dev placeholder | HS256 signing key. **Must be replaced outside local dev.** |
| `JWT_TTL_HOURS` | `24` | Token lifetime. |
| `CORS_ALLOWED_ORIGINS` | Vite dev origins | Comma-separated allowlist. |
| `DEFAULT_BUDGET_CENTS` | `500000` | Budget granted at registration (FR-1.2). |
| `RUST_LOG` | `info` | Log filter. |
| `VITE_API_BASE_URL` | `/api` | Frontend API base; `/api` uses the dev proxy. |

`.env` is gitignored; `.env.example` is committed and must gain an entry
whenever a variable is added (NFR-7).

---

## 7. Where to change what

| Task | Place |
| --- | --- |
| Add or alter a business rule about ordering | The transaction in `routes/orders.rs::create` — nowhere else |
| Add an endpoint | `routes/<resource>.rs` + register in `routes/mod.rs` |
| Add a failure mode | An `ApiError` variant + its status and code in `error.rs`; mirror in `api/types.ts` |
| Change the schema | A new file in `migrations/` — never edit an applied migration |
| Change a money format | `lib/format.ts` |
| Add a config knob | `config.rs` **and** `.env.example` |

---

## 8. What this architecture does not do

Beyond the requirement-level gaps (LIM-1…LIM-8), the structure itself assumes:

- **One process.** In-process state is limited to the pool and config, so
  running two instances against one SQLite file would work but is untested and
  pointless given the single-writer constraint.
- **No background work.** Everything happens inside a request. There is no
  scheduler, no outbox, no retry.
- **No audit trail.** Order rows record what was bought, not who changed a
  budget or a price, because nothing can change them yet.
- **No rate limiting.** Login and registration are both unthrottled (compounding
  LIM-3).

None of these are hard to add later; all of them would be wrong to add now.
