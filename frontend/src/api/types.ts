// Mirrors the DTOs in backend/src/models.rs. Money is always integer cents.

export interface User {
  id: string;
  email: string;
  display_name: string;
  budget_cents: number;
  created_at: string;
}

export interface AuthResponse {
  token: string;
  user: User;
}

export interface Budget {
  budget_cents: number;
  spent_cents: number;
  remaining_cents: number;
}

export interface Product {
  id: string;
  sku: string;
  name: string;
  description: string;
  category: string;
  price_cents: number;
  stock: number;
  image_url: string;
}

export interface OrderItem {
  product_id: string;
  sku: string;
  name: string;
  quantity: number;
  unit_price_cents: number;
  line_total_cents: number;
}

export interface Order {
  id: string;
  total_cents: number;
  status: string;
  created_at: string;
  items: OrderItem[];
}

/** Matches the `{ error, message }` body every failing endpoint returns. */
export type ApiErrorCode =
  | "bad_request"
  | "unauthorized"
  | "invalid_credentials"
  | "not_found"
  | "conflict"
  | "insufficient_budget"
  | "insufficient_stock"
  | "internal";
