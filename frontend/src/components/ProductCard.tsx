import type { Product } from "../api/types";
import { formatCents } from "../lib/format";

interface Props {
  product: Product;
  inCart: number;
  affordable: boolean;
  onAdd: (product: Product) => void;
}

export function ProductCard({ product, inCart, affordable, onAdd }: Props) {
  const soldOut = product.stock === 0;
  const atStockLimit = inCart >= product.stock;

  return (
    <article className="card">
      <img
        className="card-image"
        src={product.image_url}
        alt=""
        loading="lazy"
      />

      <div className="card-body">
        <div className="card-heading">
          <h3>{product.name}</h3>
          <span className="price">{formatCents(product.price_cents)}</span>
        </div>

        <p className="muted card-description">{product.description}</p>

        <div className="card-meta">
          <span className="tag">{product.category}</span>
          <span className="muted">
            {soldOut ? "Out of stock" : `${product.stock} in stock`}
          </span>
        </div>

        <button
          type="button"
          className="primary"
          disabled={soldOut || atStockLimit || !affordable}
          onClick={() => onAdd(product)}
        >
          {soldOut
            ? "Out of stock"
            : atStockLimit
              ? "All stock in cart"
              : !affordable
                ? "Over budget"
                : inCart > 0
                  ? `In cart (${inCart}) — add another`
                  : "Add to cart"}
        </button>
      </div>
    </article>
  );
}
