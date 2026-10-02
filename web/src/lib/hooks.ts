import { useCallback, useEffect, useRef, useState } from "react";

interface AsyncState<T> {
  data: T | null;
  error: unknown;
  loading: boolean;
}

/** Loads data on mount and whenever `deps` change; exposes a reload function. */
export function useAsync<T>(fn: () => Promise<T>, deps: unknown[]) {
  const [state, setState] = useState<AsyncState<T>>({ data: null, error: null, loading: true });
  const [version, setVersion] = useState(0);
  const fnRef = useRef(fn);
  useEffect(() => {
    fnRef.current = fn;
  });
  useEffect(() => {
    let cancelled = false;
    setState((s) => ({ ...s, loading: true }));
    fnRef.current().then(
      (data) => !cancelled && setState({ data, error: null, loading: false }),
      (error) => !cancelled && setState((s) => ({ ...s, error, loading: false })),
    );
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, version]);
  const reload = useCallback(() => setVersion((v) => v + 1), []);
  const setData = useCallback((data: T) => setState((s) => ({ ...s, data })), []);
  return { ...state, reload, setData };
}
