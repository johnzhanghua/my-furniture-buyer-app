import type {
  ApiErrorCode,
  AuthResponse,
  Balance,
  Order,
  Product,
  User,
} from "./types";

const BASE_URL = import.meta.env.VITE_API_BASE_URL ?? "/api";
const TOKEN_KEY = "fb.token";

/** Thrown for every non-2xx response; `code` is the backend's machine code. */
export class ApiError extends Error {
  readonly code: ApiErrorCode | "network";
  readonly status: number;

  constructor(code: ApiErrorCode | "network", status: number, message: string) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.status = status;
  }
}

let token: string | null = localStorage.getItem(TOKEN_KEY);

export function setToken(next: string | null): void {
  token = next;
  if (next) {
    localStorage.setItem(TOKEN_KEY, next);
  } else {
    localStorage.removeItem(TOKEN_KEY);
  }
}

export function getToken(): string | null {
  return token;
}

/** Called when any request comes back 401, so the app can bounce to login. */
let onUnauthorized: () => void = () => {};

export function setUnauthorizedHandler(handler: () => void): void {
  onUnauthorized = handler;
}

/** Stable id for one buy intent, so a retry cannot charge twice. */
export function newIdempotencyKey(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  if (init.body !== undefined) {
    headers.set("Content-Type", "application/json");
  }
  if (token) {
    headers.set("Authorization", `Bearer ${token}`);
  }

  let response: Response;
  try {
    response = await fetch(`${BASE_URL}${path}`, { ...init, headers });
  } catch {
    throw new ApiError(
      "network",
      0,
      "Could not reach the server. Is the API running?",
    );
  }

  if (response.status === 204) {
    return undefined as T;
  }

  const payload = await response.json().catch(() => null);

  if (!response.ok) {
    const code = (payload?.error as ApiErrorCode) ?? "internal";
    const message = (payload?.message as string) ?? response.statusText;
    if (response.status === 401) {
      setToken(null);
      onUnauthorized();
    }
    throw new ApiError(code, response.status, message);
  }

  return payload as T;
}

export const api = {
  login: (email: string, password: string) =>
    request<AuthResponse>("/auth/login", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    }),

  register: (email: string, password: string, displayName?: string) =>
    request<AuthResponse>("/auth/register", {
      method: "POST",
      body: JSON.stringify({
        email,
        password,
        display_name: displayName || null,
      }),
    }),

  me: () => request<User>("/me"),

  balance: () => request<Balance>("/me/balance"),

  categories: () => request<string[]>("/categories"),

  products: (params: { search?: string; category?: string } = {}) => {
    const query = new URLSearchParams();
    if (params.search) query.set("search", params.search);
    if (params.category) query.set("category", params.category);
    const suffix = query.toString();
    return request<Product[]>(`/products${suffix ? `?${suffix}` : ""}`);
  },

  product: (itemId: string) =>
    request<Product>(`/products/${encodeURIComponent(itemId)}`),

  /** Places (and pays for) an order for a single product. */
  buy: (itemId: string, quantity: number, idempotencyKey: string) =>
    request<Order>("/orders", {
      method: "POST",
      body: JSON.stringify({
        item_id: itemId,
        quantity,
        idempotency_key: idempotencyKey,
      }),
    }),

  orders: () => request<Order[]>("/orders"),
};
