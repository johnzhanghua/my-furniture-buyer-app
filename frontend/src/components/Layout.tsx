import { NavLink, Outlet, useNavigate } from "react-router-dom";

import { formatCents } from "../lib/format";
import { useAuth } from "../state/AuthContext";

export function Layout() {
  const { user, balance, logout } = useAuth();
  const navigate = useNavigate();

  const handleLogout = () => {
    logout();
    navigate("/login", { replace: true });
  };

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">Furniture&nbsp;Buyer</div>

        <nav className="nav">
          <NavLink to="/catalog">Catalogue</NavLink>
          <NavLink to="/orders">Orders</NavLink>
        </nav>

        <div className="account">
          <div className="budget" title="Balance at the furniture shop">
            <div className="budget-figures">
              <span className="budget-remaining">
                {balance ? formatCents(balance.balance_cents) : "—"}
              </span>
              <span className="muted"> balance</span>
            </div>
            {balance && <div className="muted">{balance.user_id}</div>}
          </div>
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
