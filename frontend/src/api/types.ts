// Mirrors the DTOs in backend/src/models.rs.
//
// Money is always integer cents. The furniture shop API speaks floats; the Rust
// backend converts at its boundary so no float reaches the browser.

/** A local login account. Carries no money — see `Balance`. */
export interface User {
  id: string;
  email: string;
  display_name: string;
  created_at: string;
}

export interface AuthResponse {
  token: string;
  user: User;
}

/**
 * The participant's real balance from the furniture shop's ledger. Shared by
 * every local login: there is one upstream account and one API key.
 */
export interface Balance {
  user_id: string;
  name: string;
  balance_cents: number;
}

export interface Product {
  item_id: string;
  product_name: string;
  price_cents: number;
  category: string | null;
  width: number | null;
  height: number | null;
  depth: number | null;
  colours: string[];
  /** Null in listings; a data URI on the single-product endpoint. */
  image_url: string | null;
  link: string | null;
}

export interface OrderLine {
  item_id: string;
  product_name: string | null;
  quantity: number;
  unit_price_cents: number;
  line_total_cents: number;
}

export interface Order {
  order_id: string;
  items: OrderLine[];
  total_cents: number;
  /** Present on a freshly placed order; absent in history. */
  remaining_balance_cents: number | null;
  timestamp: string | null;
}

/** One tool call the assistant made, for the "what it did" trace. */
export interface AssistantStep {
  tool: string;
  input: Record<string, unknown>;
  summary: string;
  is_error: boolean;
}

/**
 * A recommended product. The backend flattens the full catalogue record into
 * this, so it renders with the same card as the grid — image, price and Buy
 * button included — rather than as a line of text.
 */
export type Recommendation = Product & { reason: string };

/**
 * A purchase the assistant wants to make. **Nothing has been charged yet** —
 * the order is placed by the normal buy endpoint, and only when the user
 * clicks Confirm.
 */
export type PendingPurchase = Product & {
  quantity: number;
  total_cents: number;
  balance_cents: number;
  balance_after_cents: number;
  affordable: boolean;
  /** Minted with the proposal, so a double-clicked Confirm can't charge twice. */
  idempotency_key: string;
};

/** An order that actually went through, after the user replied "yes". */
export interface ConfirmedOrder {
  order_id: string;
  product_name: string;
  quantity: number;
  total_cents: number;
  remaining_balance_cents: number;
}

/** One earlier turn, replayed so the assistant can resolve "the third one". */
export interface HistoryTurn {
  role: "user" | "assistant";
  text: string;
}

export interface AssistantAnswer {
  summary: string;
  recommendations: Recommendation[];
  pending_purchase: PendingPurchase | null;
  order_placed: ConfirmedOrder | null;
  /** Digest of this turn — send it back as the assistant's history entry. */
  transcript: string;
  steps: AssistantStep[];
  model: string;
}

/** Matches the `{ error, message }` body every failing endpoint returns. */
export type ApiErrorCode =
  | "bad_request"
  | "unauthorized"
  | "invalid_credentials"
  | "not_found"
  | "conflict"
  | "insufficient_balance"
  | "upstream_error"
  | "internal";
