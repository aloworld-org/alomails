// A tiny async-load hook: runs a memoized loader, tracks loading/ready/error,
// cancels a superseded run, and exposes reload(). The caller passes a
// useCallback-stable loader so the effect key is explicit (no dep arrays).
import { useEffect, useState } from "react";

export interface AsyncState<T> {
  status: "loading" | "ready" | "error";
  data: T | null;
  error: string | null;
}

export interface Async<T> extends AsyncState<T> {
  reload: () => void;
}

export function useAsync<T>(load: () => Promise<T>): Async<T> {
  const [nonce, setNonce] = useState(0);
  const [state, setState] = useState<AsyncState<T>>({
    status: "loading",
    data: null,
    error: null,
  });

  useEffect(() => {
    let cancelled = false;
    setState({ status: "loading", data: null, error: null });
    load().then(
      (data) => {
        if (!cancelled) setState({ status: "ready", data, error: null });
      },
      (err: unknown) => {
        if (!cancelled) {
          const message = err instanceof Error ? err.message : "error";
          setState({ status: "error", data: null, error: message });
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, [load, nonce]);

  return { ...state, reload: () => setNonce((n) => n + 1) };
}
