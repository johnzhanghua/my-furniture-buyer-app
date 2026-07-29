import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";

import { ApiError, api } from "../api/client";
import { formatCents } from "../lib/format";
import { useAuth } from "../state/AuthContext";
import { useCart } from "../state/CartContext";

export function CartPage() {
  const { budget, refreshBudget } = useAuth();
  const { lines, totalCents, setQuantity, remove, clear } = useCart();
  const navigate = useNavigate();

  const [error, setError] = useState<string | null>(null);
  const [placing, setPlacing] = useState(false);

  const remainingCents = budget?.remaining_cents ?? 0;
  const overBudget = totalCents > remainingCents;

  const handleCheckout = async () => {
    setError(null);
    setPlacing(true);
    try {
      await api.placeOrder(
        lines.map((l) => ({ product_id: l.product.id, quantity: l.quantity })),
      );
      clear();
      await refreshBudget();
      navigate("/orders");
    } catch (err) {
      // The backend is the authority on budget and stock — surface its message
      // verbatim rather than second-guessing it here.
      setError(
        err instanceof ApiError ? err.message : "Could not place the order.",
      );
    } finally {
      setPlacing(false);
    }
  };

  if (lines.length === 0) {
    return (
      <section className="panel">
        <h1>Your cart is empty</h1>
        <p className="muted">
          <Link to="/catalog">Browse the catalogue</Link> to add furniture.
        </p>
      </section>
    );
  }

  return (
    <section>
      <h1>Cart</h1>

      <table className="table">
        <thead>
          <tr>
            <th>Item</th>
            <th>Unit price</th>
            <th>Quantity</th>
            <th className="numeric">Line total</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {lines.map(({ product, quantity }) => (
            <tr key={product.id}>
              <td>
                <strong>{product.name}</strong>
                <div className="muted">{product.sku}</div>
              </td>
              <td>{formatCents(product.price_cents)}</td>
              <td>
                <input
                  type="number"
                  min={1}
                  max={product.stock}
                  value={quantity}
                  className="qty"
                  onChange={(e) =>
                    setQuantity(product.id, Number(e.target.value) || 0)
                  }
                />
                <div className="muted">{product.stock} available</div>
              </td>
              <td className="numeric">
                {formatCents(product.price_cents * quantity)}
              </td>
              <td>
                <button
                  type="button"
                  className="link-button"
                  onClick={() => remove(product.id)}
                >
                  Remove
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      <div className="panel checkout">
        <div className="checkout-row">
          <span>Order total</span>
          <strong>{formatCents(totalCents)}</strong>
        </div>
        <div className="checkout-row">
          <span>Budget remaining</span>
          <strong>{formatCents(remainingCents)}</strong>
        </div>
        <div className="checkout-row">
          <span>After this order</span>
          <strong className={overBudget ? "error" : undefined}>
            {formatCents(remainingCents - totalCents)}
          </strong>
        </div>

        {overBudget && (
          <p className="error">
            This order exceeds your remaining budget by{" "}
            {formatCents(totalCents - remainingCents)}.
          </p>
        )}
        {error && <p className="error">{error}</p>}

        <div className="checkout-actions">
          <button type="button" className="link-button" onClick={clear}>
            Empty cart
          </button>
          <button
            type="button"
            className="primary"
            disabled={placing || overBudget}
            onClick={handleCheckout}
          >
            {placing ? "Placing order…" : "Place order"}
          </button>
        </div>
      </div>
    </section>
  );
}
