import { useCallback, useEffect, useState } from "react";

import { ApiError, api, newIdempotencyKey } from "../api/client";
import type { Order, Product } from "../api/types";
import { ProductCard } from "../components/ProductCard";
import { formatCents } from "../lib/format";
import { useAuth } from "../state/AuthContext";

/**
 * Turns a failed buy into a sentence. The backend already maps upstream's
 * status codes to these codes, so this switch is the only place wording lives.
 */
function buyErrorMessage(error: unknown, balanceCents: number | null): string {
  if (!(error instanceof ApiError)) {
    return "Something went wrong placing that order. Please try again.";
  }
  switch (error.code) {
    case "insufficient_balance":
      return balanceCents === null
        ? "You don't have enough balance for that order."
        : `You don't have enough balance for that order — you have ${formatCents(balanceCents)}.`;
    case "not_found":
      return "This item is no longer available.";
    case "upstream_error":
      return "The furniture shop is unavailable right now. Please try again in a moment.";
    case "network":
      return "Could not reach the server. Check that the API is running.";
    default:
      return error.message;
  }
}

export function CatalogPage() {
  const { balance, refreshBalance, setBalanceCents } = useAuth();

  const [products, setProducts] = useState<Product[]>([]);
  const [categories, setCategories] = useState<string[]>([]);
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("");
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const [buyingId, setBuyingId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [buyError, setBuyError] = useState<string | null>(null);

  // Debounced so typing in the search box does not fire a request per keystroke.
  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(async () => {
      setLoading(true);
      try {
        const result = await api.products({
          search: search || undefined,
          category: category || undefined,
        });
        if (!cancelled) {
          setProducts(result);
          setLoadError(null);
        }
      } catch (err) {
        if (!cancelled) {
          setLoadError(
            err instanceof ApiError
              ? err.message
              : "Could not load the catalogue.",
          );
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    }, 250);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [search, category]);

  useEffect(() => {
    api
      .categories()
      .then(setCategories)
      .catch(() => setCategories([]));
  }, []);

  const handleBuy = useCallback(
    async (product: Product) => {
      // Guard against a second click landing before the button re-renders as
      // disabled. The idempotency key covers anything that slips past.
      if (buyingId !== null) return;

      setBuyingId(product.item_id);
      setNotice(null);
      setBuyError(null);

      try {
        const order: Order = await api.buy(
          product.item_id,
          1,
          newIdempotencyKey(),
        );

        setNotice(
          `Ordered ${product.product_name} for ${formatCents(order.total_cents)}.` +
            (order.remaining_balance_cents !== null
              ? ` Balance now ${formatCents(order.remaining_balance_cents)}.`
              : ""),
        );

        if (order.remaining_balance_cents !== null) {
          setBalanceCents(order.remaining_balance_cents);
        } else {
          await refreshBalance();
        }
      } catch (err) {
        setBuyError(buyErrorMessage(err, balance?.balance_cents ?? null));
        // The failure may have been a stale balance on our side; re-read it.
        await refreshBalance();
      } finally {
        setBuyingId(null);
      }
    },
    [buyingId, balance, refreshBalance, setBalanceCents],
  );

  const balanceCents = balance?.balance_cents ?? 0;

  return (
    <section>
      <div className="toolbar">
        <input
          type="search"
          placeholder="Search by name, category or item ID…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <select value={category} onChange={(e) => setCategory(e.target.value)}>
          <option value="">All categories</option>
          {categories.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
        <span className="muted">
          {balance ? `${formatCents(balanceCents)} to spend` : "balance —"}
        </span>
      </div>

      {notice && <p className="notice">{notice}</p>}
      {buyError && <p className="error">{buyError}</p>}
      {loadError && <p className="error">{loadError}</p>}

      {loading && products.length === 0 ? (
        <p className="muted">Loading catalogue…</p>
      ) : products.length === 0 ? (
        <p className="muted">No products match that search.</p>
      ) : (
        <div className="grid">
          {products.map((product) => (
            <ProductCard
              key={product.item_id}
              product={product}
              buying={buyingId === product.item_id}
              anyBuying={buyingId !== null}
              affordable={!balance || product.price_cents <= balanceCents}
              onBuy={handleBuy}
            />
          ))}
        </div>
      )}
    </section>
  );
}
