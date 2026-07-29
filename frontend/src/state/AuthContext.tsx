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
import type { Budget, User } from "../api/types";

interface AuthContextValue {
  user: User | null;
  budget: Budget | null;
  /** True until the initial token check resolves; gates route rendering. */
  loading: boolean;
  login: (email: string, password: string) => Promise<void>;
  register: (
    email: string,
    password: string,
    displayName?: string,
  ) => Promise<void>;
  logout: () => void;
  /** Re-reads the budget after an order is placed. */
  refreshBudget: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [budget, setBudget] = useState<Budget | null>(null);
  const [loading, setLoading] = useState(true);

  const logout = useCallback(() => {
    setToken(null);
    setUser(null);
    setBudget(null);
  }, []);

  const refreshBudget = useCallback(async () => {
    try {
      setBudget(await api.budget());
    } catch {
      // A stale budget is not worth blocking the UI over; the 401 path in the
      // API client already handles an expired session.
    }
  }, []);

  // Restore a persisted session on boot.
  useEffect(() => {
    setUnauthorizedHandler(() => {
      setUser(null);
      setBudget(null);
    });

    if (!getToken()) {
      setLoading(false);
      return;
    }

    let cancelled = false;
    (async () => {
      try {
        const [me, currentBudget] = await Promise.all([api.me(), api.budget()]);
        if (!cancelled) {
          setUser(me);
          setBudget(currentBudget);
        }
      } catch {
        if (!cancelled) {
          setToken(null);
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
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
      await refreshBudget();
    },
    [refreshBudget],
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
    () => ({ user, budget, loading, login, register, logout, refreshBudget }),
    [user, budget, loading, login, register, logout, refreshBudget],
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
