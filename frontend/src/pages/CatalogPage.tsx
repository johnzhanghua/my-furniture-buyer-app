import { useEffect, useMemo, useState } from "react";

import { ApiError, api } from "../api/client";
import type { Product } from "../api/types";
import { ProductCard } from "../components/ProductCard";
import { formatCents } from "../lib/format";
import { useAuth } from "../state/AuthContext";
import { useCart } from "../state/CartContext";

export function CatalogPage() {
  const { budget } = useAuth();
  const { lines, totalCents, add } = useCart();

  const [products, setProducts] = useState<Product[]>([]);
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

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
          setError(null);
        }
      } catch (err) {
        if (!cancelled) {
          setError(
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

  // Categories come from the unfiltered result set, so the dropdown does not
  // collapse to a single option once a filter is applied.
  const [categories, setCategories] = useState<string[]>([]);
  useEffect(() => {
    api
      .products()
      .then((all) =>
        setCategories([...new Set(all.map((p) => p.category))].sort()),
      )
      .catch(() => setCategories([]));
  }, []);

  const quantities = useMemo(() => {
    const map = new Map<string, number>();
    for (const line of lines) map.set(line.product.id, line.quantity);
    return map;
  }, [lines]);

  const headroomCents = (budget?.remaining_cents ?? 0) - totalCents;

  return (
    <section>
      <div className="toolbar">
        <input
          type="search"
          placeholder="Search by name, description or SKU…"
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
          {formatCents(Math.max(headroomCents, 0))} left to spend
        </span>
      </div>

      {error && <p className="error">{error}</p>}

      {loading && products.length === 0 ? (
        <p className="muted">Loading catalogue…</p>
      ) : products.length === 0 ? (
        <p className="muted">No products match that search.</p>
      ) : (
        <div className="grid">
          {products.map((product) => (
            <ProductCard
              key={product.id}
              product={product}
              inCart={quantities.get(product.id) ?? 0}
              affordable={product.price_cents <= headroomCents}
              onAdd={add}
            />
          ))}
        </div>
      )}
    </section>
  );
}
