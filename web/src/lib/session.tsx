import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { api, ApiError, setCsrfToken } from "./api";
import { initI18n } from "./i18n";
import type { Instance, Me, Role } from "./types";

interface SessionValue {
  instance: Instance;
  me: Me | null;
  refresh: () => Promise<void>;
  hasRole: (r: Role) => boolean;
}

const SessionContext = createContext<SessionValue | null>(null);

export function useSession(): SessionValue {
  const ctx = useContext(SessionContext);
  if (!ctx) throw new Error("useSession outside SessionProvider");
  return ctx;
}

async function loadMe(): Promise<Me | null> {
  try {
    const me = await api.get<Me>("/api/me");
    setCsrfToken(me.csrf_token);
    return me;
  } catch (e) {
    if (e instanceof ApiError && e.status === 401) return null;
    throw e;
  }
}

export function SessionProvider({ children, fallback }: { children: ReactNode; fallback: ReactNode }) {
  const [instance, setInstance] = useState<Instance | null>(null);
  const [me, setMe] = useState<Me | null>(null);

  useEffect(() => {
    (async () => {
      const inst = await api.get<Instance>("/api/instance");
      await initI18n(inst.locale, { instance: inst.name, product: inst.product_name });
      document.title = inst.name;
      if (inst.favicon) {
        const link = document.createElement("link");
        link.rel = "icon";
        link.href = inst.favicon;
        document.head.appendChild(link);
      }
      setMe(await loadMe());
      setInstance(inst);
    })();
  }, []);

  const refresh = useCallback(async () => setMe(await loadMe()), []);
  const hasRole = useCallback((r: Role) => !!me?.roles.includes(r), [me]);

  if (!instance) return <>{fallback}</>;
  return <SessionContext.Provider value={{ instance, me, refresh, hasRole }}>{children}</SessionContext.Provider>;
}
