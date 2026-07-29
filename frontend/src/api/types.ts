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
