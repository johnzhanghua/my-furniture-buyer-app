import type { Product } from "../api/types";
import { formatCents } from "../lib/format";

interface Props {
  product: Product;
  /** True while this product's own buy request is in flight. */
  buying: boolean;
  /** True while any buy is in flight — blocks starting a second one. */
  anyBuying: boolean;
  affordable: boolean;
  onBuy: (product: Product) => void;
}

export function ProductCard({
  product,
  buying,
  anyBuying,
  affordable,
  onBuy,
}: Props) {
  // Disabled while in flight: the first half of double-click protection. The
  // second half is the idempotency key sent with the request.
  const disabled = anyBuying || !affordable;

  return (
    <article className="card">
      <div className="card-body">
        <div className="card-heading">
          <h3>{product.product_name}</h3>
          <span className="price">{formatCents(product.price_cents)}</span>
        </div>

        <div className="card-meta">
          {product.category && <span className="tag">{product.category}</span>}
          {product.colours.length > 0 && (
            <span className="muted">{product.colours.join(", ")}</span>
          )}
        </div>

        <div className="muted card-description">
          {product.item_id}
          {product.width && product.height
            ? ` · ${product.width}×${product.height} cm`
            : ""}
        </div>

        <button
          type="button"
          className="primary"
          disabled={disabled}
          onClick={() => onBuy(product)}
        >
          {buying
            ? "Placing order…"
            : !affordable
              ? "Over balance"
              : `Buy for ${formatCents(product.price_cents)}`}
        </button>
      </div>
    </article>
  );
}
