# Requirements

Scope for the Day 1 hackathon build of the furniture buyer's app. Requirement
IDs are referenced from [architecture.md](architecture.md).

Status legend: **Built** — implemented in this repo (unverified, see
[Verification status](#verification-status)) · **Gap** — in scope but not
implemented · **Out** — explicitly excluded.

---

## 1. Product summary

A buyer employed by an organisation is given a fixed furnishing budget. They
sign in, browse the shop's catalogue, assemble a cart, and place an order. The
system must guarantee that a buyer can never commit more money than their
budget allows, and can never order stock that does not exist.

The three-step loop — **log in → browse → order against a budget** — is the
whole product. Everything else is supporting detail.

### Primary actor

**Buyer.** Authenticated, has one budget, sees only their own orders. There is
no admin, staff, or supplier actor in this build.

---

## 2. Functional requirements

### 2.1 Authentication and identity

| ID | Requirement | Status |
| --- | --- | --- |
| FR-1.1 | A buyer signs in with an email address and password and receives a session credential. | Built |
| FR-1.2 | A buyer can self-register; the account is created with the configured default budget (`DEFAULT_BUDGET_CENTS`, default $5,000.00). | Built |
| FR-1.3 | Passwords are never stored or logged in recoverable form. | Built (Argon2id) |
| FR-1.4 | A failed sign-in must not reveal whether the email address is registered. | Built (single `invalid_credentials` error for both cases) |
| FR-1.5 | Registration rejects passwords shorter than 8 characters and malformed email addresses. | Built |
| FR-1.6 | Email addresses are unique and case-insensitive (normalised to lowercase). | Built |
| FR-1.7 | A session expires after 24 hours; an expired credential is rejected. | Built (JWT `exp`) |
| FR-1.8 | Signing out ends the session on the client. | Built (client-side token discard — see LIM-4) |
| FR-1.9 | A returning buyer with a valid stored credential resumes their session without re-entering it. | Built |
| FR-1.10 | Password reset / change. | Out |
| FR-1.11 | Email verification, MFA, SSO. | Out |

### 2.2 Product catalogue

| ID | Requirement | Status |
| --- | --- | --- |
| FR-2.1 | The catalogue lists products with name, description, category, SKU, price, stock level, and an image. | Built |
| FR-2.2 | A buyer can free-text search across name, description, and SKU, case-insensitively. | Built |
| FR-2.3 | A buyer can filter by category. | Built |
| FR-2.4 | Search and filter combine (logical AND). | Built |
| FR-2.5 | The catalogue is readable without authentication; ordering is not. | Built (deliberate — see DD-6) |
| FR-2.6 | The shop is seeded with a representative catalogue for demo purposes. | Built (12 products, 5 categories) |
| FR-2.6a | **The catalogue carries furniture only.** Lighting fixtures, rugs, mirrors, and other decor or soft furnishings are out of scope. | Built (enforced by seed data, not by schema — see LIM-9) |
| FR-2.7 | Products display a per-item indication of whether they are affordable within the remaining budget. | Built |
| FR-2.8 | Catalogue pagination. | Gap (see LIM-6) |
| FR-2.9 | Product creation / editing by staff. | Out |

### 2.3 Cart

| ID | Requirement | Status |
| --- | --- | --- |
| FR-3.1 | A buyer can add a product to a cart, adjust its quantity, remove a line, and empty the cart. | Built |
| FR-3.2 | The cart persists across page reloads. | Built (`localStorage`) |
| FR-3.3 | Cart quantity for a product cannot exceed that product's stock. | Built (client-side clamp; authoritative check is FR-4.4) |
| FR-3.4 | The cart shows a running total and the budget position after the order. | Built |
| FR-3.5 | The cart is private to the browser and is not persisted server-side. | Built (deliberate — see DD-7) |
| FR-3.6 | Saved carts / wishlists shared across devices. | Out |

### 2.4 Ordering

| ID | Requirement | Status |
| --- | --- | --- |
| FR-4.1 | A buyer can place an order containing one or more line items. | Built |
| FR-4.2 | An empty order, or a line with quantity ≤ 0, is rejected. | Built |
| FR-4.3 | **An order whose total exceeds the buyer's remaining budget is rejected in full.** No partial fulfilment. | Built |
| FR-4.4 | **An order whose line exceeds available stock is rejected in full.** | Built |
| FR-4.5 | Placing an order decrements stock for every line. | Built |
| FR-4.6 | An order records the unit price at the time of purchase, so later price changes do not alter order history. | Built (`order_items.unit_price_cents`) |
| FR-4.7 | Rejection is atomic: a rejected order must leave no order row, no order items, and no stock change. | Built (single transaction) |
| FR-4.8 | Duplicate lines for the same product in one request are merged before budget and stock are evaluated. | Built |
| FR-4.9 | A buyer can view their own order history, newest first, with line detail. | Built |
| FR-4.10 | A buyer cannot read another buyer's order. | Built (queries scoped by `user_id`) |
| FR-4.11 | Order cancellation (and the budget refund it implies). | Gap (see LIM-2) |
| FR-4.12 | Payment capture, invoicing, delivery scheduling, returns. | Out |

### 2.5 Budget

| ID | Requirement | Status |
| --- | --- | --- |
| FR-5.1 | Each buyer has a budget expressed as a single monetary amount. | Built |
| FR-5.2 | Remaining budget = budget − sum of the buyer's non-cancelled order totals. | Built |
| FR-5.3 | The remaining budget is visible on every authenticated screen. | Built (header) |
| FR-5.4 | Budget enforcement is performed server-side; the client's check is advisory feedback only. | Built |
| FR-5.5 | The budget display updates immediately after an order is placed. | Built |
| FR-5.6 | Budget top-ups, per-category budgets, budget periods, approval workflows. | Out |

---

## 3. Non-functional requirements

| ID | Requirement | Status |
| --- | --- | --- |
| NFR-1 | **Money is never represented as a floating-point number.** All amounts are integer minor units (cents) end to end. | Built |
| NFR-2 | Every API error returns a machine-readable code and a human-readable message in a consistent JSON shape. | Built |
| NFR-3 | Internal faults (database, signing) return an opaque client message; detail is logged server-side only. | Built |
| NFR-4 | The client never supplies a price; the server re-prices every line from its own records. | Built |
| NFR-5 | A single `git clone` plus two documented commands starts the whole system, with no external service, container, or manual database setup. | Built |
| NFR-6 | The demo dataset is created automatically on first boot and is idempotent across restarts. | Built |
| NFR-7 | Secrets are supplied by environment variable, never committed. `.env.example` documents every variable. | Built |
| NFR-8 | The API and the UI can be developed and served from different origins. | Built (configurable CORS allowlist + dev proxy) |
| NFR-9 | Automated test coverage for the budget and stock rules. | **Gap — see LIM-1** |
| NFR-10 | Horizontal scalability, high availability, backups. | Out |
| NFR-11 | Accessibility audit (WCAG), internationalisation, mobile-first layout. | Out — the UI uses semantic markup and visible focus rings, but is unaudited and English/USD only |
| NFR-12 | Observability beyond request logging (metrics, tracing, alerting). | Out |

---

## 4. Business rules

These are the invariants the system exists to protect. They are stated here in
plain language; their single implementation site is given in
[architecture.md](architecture.md).

- **BR-1 — Budget ceiling.** The sum of a buyer's non-cancelled order totals may
  never exceed their budget. Checked against the *whole* order, not per line.
- **BR-2 — Stock floor.** A product's stock may never go negative.
- **BR-3 — All or nothing.** An order that violates BR-1 or BR-2 is rejected
  entirely; no partial order is ever created.
- **BR-4 — Server-priced.** The price charged for a line is the catalogue price
  held by the server at the moment the order is placed.
- **BR-5 — Immutable history.** A placed order's line prices and quantities do
  not change afterwards, regardless of later catalogue edits.
- **BR-6 — Tenancy.** A buyer may read only their own profile, budget, and
  orders.

---

## 5. Acceptance criteria (demo script)

The build is "done" for Day 1 when this sequence works end to end against a
freshly deleted database:

1. Start the API and the UI. The database file, schema, catalogue, and demo
   buyer are created automatically. → NFR-5, NFR-6
2. Open the app while signed out and request `/orders`; the app redirects to the
   sign-in screen. → FR-1.9, BR-6
3. Sign in as `buyer@example.com` / `password123`. The header shows
   `$5,000.00 / $5,000.00`. → FR-1.1, FR-5.3
4. Sign in with the wrong password and with an unregistered address; both
   produce the same message. → FR-1.4
5. Search `oak`; results narrow to matching items. Filter to `Chairs`; the
   category dropdown still lists every category. → FR-2.2, FR-2.3
6. Add a chair and a table to the cart. Reload the browser — the cart survives.
   → FR-3.1, FR-3.2
7. Place the order. It succeeds, the cart empties, the header budget drops by
   the order total, and the order appears in Orders with per-line prices.
   → FR-4.1, FR-4.5, FR-4.9, FR-5.5
8. Add items totalling more than the remaining budget. The Place order button
   disables and states the overage. → FR-5.4
9. Bypass the UI and `POST /api/orders` directly with the same over-budget
   payload. The API returns `422 insufficient_budget`, and no order or stock
   change is recorded. → BR-1, BR-3, FR-4.7
10. `POST /api/orders` with a quantity above stock. The API returns
    `422 insufficient_stock` and nothing is recorded. → BR-2, BR-3
11. `POST /api/orders` with no `Authorization` header, and again with a
    tampered token. Both return `401`. → FR-1.7
12. `GET /api/orders/{id}` using another buyer's order id returns `404`, not the
    order. → FR-4.10, BR-6

Steps 9–12 are the ones that matter — they are what distinguishes an enforced
budget from a disabled button.

---

## 6. Known limitations

Accepted for Day 1, listed so they are not mistaken for oversights.

- **LIM-1 — No automated tests.** The budget and stock rules are the core of the
  product and are currently verified only by hand (§5). This is the largest
  quality gap in the build and the first thing worth closing on Day 2.
- **LIM-2 — Order status is write-once.** Orders are always created as `placed`.
  The budget calculation already excludes `cancelled` orders, but nothing can
  produce that status, so FR-4.11 is unreachable until a cancel endpoint exists.
  Cancelling would also need to restore stock.
- **LIM-3 — Open registration.** Anyone reaching the API can create an account
  and grant themselves the default budget. Acceptable for a demo; unacceptable
  for anything real, where budgets would be issued by an administrator.
- **LIM-4 — Sessions cannot be revoked.** JWTs are stateless and valid until
  they expire. Signing out discards the token client-side; a copied token
  remains usable for up to 24 hours.
- **LIM-5 — Token is held in `localStorage`.** Convenient and survives reloads,
  but readable by any injected script. A cookie-based session would be the
  correct choice outside a hackathon.
- **LIM-6 — No pagination.** `/api/products` and `/api/orders` return everything.
  Fine at 12 products; not at 16,000.
- **LIM-7 — Single-writer database.** SQLite serialises writes. Concurrent
  checkout is correct but not fast, and is untested under load.
- **LIM-8 — Budget is recomputed by aggregation.** Remaining budget is a `SUM`
  over the orders table on every read rather than a stored balance. Simple and
  always consistent; it will need a materialised balance if order volume grows.
- **LIM-9 — "Furniture only" is a data convention, not a constraint.** FR-2.6a is
  enforced by what the seed migrations leave in the `products` table. Nothing in
  the schema stops a non-furniture `category` being inserted later. A `CHECK`
  constraint would need a SQLite table rebuild and would turn adding a
  legitimate new furniture category into a migration; not worth it at this size.

## Verification status

Partially verified, as of the last update to this document:

| Check | Status |
| --- | --- |
| `cargo check` (backend compiles) | Passes — one dead-code warning on `AuthUser::email` |
| `cargo fmt` | Applied |
| `npm run typecheck` | Passes |
| `npm run build` (production bundle) | Passes |
| `npm run dev` (dev server serves) | Passes |
| Catalogue restricted to furniture (FR-2.6a) | Verified against a copy of a live database: 16 products/7 categories → 12/5, and the delete guard leaves an already-ordered product in place |
| **Acceptance script (§5)** | **Not run** |
| **Budget and stock enforcement (BR-1…BR-3)** | **Not exercised at runtime** |

The two rules the product exists to enforce have been written and reviewed but
never executed. Until §5 steps 7–10 are run against a live API, treat BR-1
through BR-3 as *unproven*.
