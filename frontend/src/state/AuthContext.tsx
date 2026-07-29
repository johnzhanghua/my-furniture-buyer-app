import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

import { api, getToken, setToken, setUnauthorizedHandler } from "../api/client";
import type { Balance, User } from "../api/types";

interface AuthContextValue {
  user: User | null;
  /** Real balance from the furniture shop ledger; null until loaded. */
  balance: Balance | null;
  /** True until the initial token check resolves; gates route rendering. */
  loading: boolean;
  login: (email: string, password: string) => Promise<void>;
  register: (
    email: string,
    password: string,
    displayName?: string,
  ) => Promise<void>;
  logout: () => void;
  refreshBalance: () => Promise<void>;
  /** Applied straight from an order response, avoiding a second round trip. */
  setBalanceCents: (cents: number) => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [balance, setBalance] = useState<Balance | null>(null);
  const [loading, setLoading] = useState(true);

  const logout = useCallback(() => {
    setToken(null);
    setUser(null);
    setBalance(null);
  }, []);

  const refreshBalance = useCallback(async () => {
    try {
      setBalance(await api.balance());
    } catch {
      // A stale balance is not worth blocking the UI over; the 401 path in the
      // API client already handles an expired session.
    }
  }, []);

  const setBalanceCents = useCallback((cents: number) => {
    setBalance((current) =>
      current ? { ...current, balance_cents: cents } : current,
    );
  }, []);

  // Restore a persisted session on boot.
  useEffect(() => {
    setUnauthorizedHandler(() => {
      setUser(null);
      setBalance(null);
    });

    if (!getToken()) {
      setLoading(false);
      return;
    }

    let cancelled = false;
    (async () => {
      try {
        const me = await api.me();
        if (!cancelled) setUser(me);
        // Balance is fetched separately: an upstream outage must not log the
        // user out of an otherwise working app.
        try {
          const current = await api.balance();
          if (!cancelled) setBalance(current);
        } catch {
          /* leave balance null; the header shows a dash */
        }
      } catch {
        if (!cancelled) setToken(null);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  const adopt = useCallback(
    async (auth: { token: string; user: User }) => {
      setToken(auth.token);
      setUser(auth.user);
      await refreshBalance();
    },
    [refreshBalance],
  );

  const login = useCallback(
    async (email: string, password: string) => {
      await adopt(await api.login(email, password));
    },
    [adopt],
  );

  const register = useCallback(
    async (email: string, password: string, displayName?: string) => {
      await adopt(await api.register(email, password, displayName));
    },
    [adopt],
  );

  const value = useMemo(
    () => ({
      user,
      balance,
      loading,
      login,
      register,
      logout,
      refreshBalance,
      setBalanceCents,
    }),
    [
      user,
      balance,
      loading,
      login,
      register,
      logout,
      refreshBalance,
      setBalanceCents,
    ],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error("useAuth must be used inside an AuthProvider");
  }
  return ctx;
}
