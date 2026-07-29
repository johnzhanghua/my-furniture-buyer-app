import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

import type { Product } from "../api/types";

export interface CartLine {
  product: Product;
  quantity: number;
}

interface CartContextValue {
  lines: CartLine[];
  totalCents: number;
  itemCount: number;
  add: (product: Product, quantity?: number) => void;
  setQuantity: (productId: string, quantity: number) => void;
  remove: (productId: string) => void;
  clear: () => void;
}

const CART_KEY = "fb.cart";
const CartContext = createContext<CartContextValue | null>(null);

function readStoredCart(): CartLine[] {
  try {
    const raw = localStorage.getItem(CART_KEY);
    return raw ? (JSON.parse(raw) as CartLine[]) : [];
  } catch {
    return [];
  }
}

/**
 * The cart is purely client-side. Prices held here are for display only — the
 * backend re-prices every line at checkout.
 */
export function CartProvider({ children }: { children: ReactNode }) {
  const [lines, setLines] = useState<CartLine[]>(readStoredCart);

  useEffect(() => {
    localStorage.setItem(CART_KEY, JSON.stringify(lines));
  }, [lines]);

  const add = useCallback((product: Product, quantity = 1) => {
    setLines((current) => {
      const existing = current.find((l) => l.product.id === product.id);
      if (!existing) {
        return [...current, { product, quantity }];
      }
      return current.map((l) =>
        l.product.id === product.id
          ? { ...l, quantity: Math.min(l.quantity + quantity, product.stock) }
          : l,
      );
    });
  }, []);

  const setQuantity = useCallback((productId: string, quantity: number) => {
    setLines((current) =>
      current.flatMap((l) => {
        if (l.product.id !== productId) return [l];
        const clamped = Math.min(Math.max(quantity, 0), l.product.stock);
        return clamped === 0 ? [] : [{ ...l, quantity: clamped }];
      }),
    );
  }, []);

  const remove = useCallback((productId: string) => {
    setLines((current) => current.filter((l) => l.product.id !== productId));
  }, []);

  const clear = useCallback(() => setLines([]), []);

  const value = useMemo(() => {
    const totalCents = lines.reduce(
      (sum, l) => sum + l.product.price_cents * l.quantity,
      0,
    );
    const itemCount = lines.reduce((sum, l) => sum + l.quantity, 0);
    return { lines, totalCents, itemCount, add, setQuantity, remove, clear };
  }, [lines, add, setQuantity, remove, clear]);

  return <CartContext.Provider value={value}>{children}</CartContext.Provider>;
}

export function useCart(): CartContextValue {
  const ctx = useContext(CartContext);
  if (!ctx) {
    throw new Error("useCart must be used inside a CartProvider");
  }
  return ctx;
}
