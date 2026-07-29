import { useEffect, useState } from "react";
import { Link } from "react-router-dom";

import { ApiError, api } from "../api/client";
import type { Order } from "../api/types";
import { formatCents, formatDate } from "../lib/format";

export function OrdersPage() {
  const [orders, setOrders] = useState<Order[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    api
      .orders()
      .then((result) => {
        if (!cancelled) setOrders(result);
      })
      .catch((err) => {
        if (!cancelled) {
          setError(
            err instanceof ApiError
              ? err.message
              : "Could not load your orders.",
          );
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  if (loading) {
    return <p className="muted">Loading orders…</p>;
  }
  if (error) {
    return <p className="error">{error}</p>;
  }
  if (orders.length === 0) {
    return (
      <section className="panel">
        <h1>No orders yet</h1>
        <p className="muted">
          <Link to="/catalog">Browse the catalogue</Link> to place your first
          order.
        </p>
      </section>
    );
  }

  return (
    <section>
      <h1>Orders</h1>

      {orders.map((order) => (
        <article key={order.id} className="panel order">
          <header className="order-header">
            <div>
              <strong>{formatDate(order.created_at)}</strong>
              <div className="muted">#{order.id.slice(0, 8)}</div>
            </div>
            <div className="order-total">
              <span className="tag">{order.status}</span>
              <strong>{formatCents(order.total_cents)}</strong>
            </div>
          </header>

          <table className="table">
            <tbody>
              {order.items.map((item) => (
                <tr key={item.product_id}>
                  <td>
                    {item.name}
                    <div className="muted">{item.sku}</div>
                  </td>
                  <td className="muted">
                    {item.quantity} × {formatCents(item.unit_price_cents)}
                  </td>
                  <td className="numeric">
                    {formatCents(item.line_total_cents)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </article>
      ))}
    </section>
  );
}
