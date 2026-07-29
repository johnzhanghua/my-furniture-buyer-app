import { NavLink, Outlet, useNavigate } from "react-router-dom";

import { formatCents } from "../lib/format";
import { useAuth } from "../state/AuthContext";
import { useCart } from "../state/CartContext";

export function Layout() {
  const { user, budget, logout } = useAuth();
  const { itemCount, clear } = useCart();
  const navigate = useNavigate();

  const handleLogout = () => {
    clear();
    logout();
    navigate("/login", { replace: true });
  };

  const spentRatio =
    budget && budget.budget_cents > 0
      ? Math.min(budget.spent_cents / budget.budget_cents, 1)
      : 0;

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">Furniture&nbsp;Buyer</div>

        <nav className="nav">
          <NavLink to="/catalog">Catalogue</NavLink>
          <NavLink to="/cart">
            Cart{itemCount > 0 ? ` (${itemCount})` : ""}
          </NavLink>
          <NavLink to="/orders">Orders</NavLink>
        </nav>

        <div className="account">
          {budget && (
            <div className="budget" title="Remaining budget">
              <div className="budget-figures">
                <span className="budget-remaining">
                  {formatCents(budget.remaining_cents)}
                </span>
                <span className="muted">
                  {" "}
                  / {formatCents(budget.budget_cents)}
                </span>
              </div>
              <div className="budget-bar">
                <div
                  className="budget-bar-fill"
                  style={{ width: `${spentRatio * 100}%` }}
                />
              </div>
            </div>
          )}
          <span className="muted">{user?.display_name}</span>
          <button type="button" className="link-button" onClick={handleLogout}>
            Sign out
          </button>
        </div>
      </header>

      <main className="content">
        <Outlet />
      </main>
    </div>
  );
}
