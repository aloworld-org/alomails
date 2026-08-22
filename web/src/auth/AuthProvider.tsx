// AuthProvider — the session authority for the whole app. It bootstraps a
// session from a stored refresh token on load, runs sign-in/sign-out, and
// exposes `authorizedFetch`: a fetch that attaches the bearer token, refreshes
// it transparently on expiry or a 401, and drops to signed-out (once) when the
// session can no longer be renewed. Every backend call in the app goes through
// it, so token handling lives in exactly one place.
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";

import { apiFetch } from "../platform/runtime";
import { login as oidcLogin, refresh as oidcRefresh, revoke } from "./oidcClient";
import {
  accessTokenStale,
  clearSession,
  getAccessToken,
  getRefreshToken,
  refreshTokenIsPersistent,
  setAccessToken,
  setRefreshToken,
} from "./session";
import type { Identity } from "./session";

type Status = "loading" | "anonymous" | "authenticated";

interface AuthContextValue {
  status: Status;
  identity: Identity | null;
  signIn: (username: string, password: string, otp?: string, remember?: boolean) => Promise<void>;
  signOut: () => Promise<void>;
  /** fetch() with bearer auth + transparent refresh. Throws on a dead session. */
  authorizedFetch: (input: string, init?: RequestInit) => Promise<Response>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<Status>("loading");
  const [identity, setIdentity] = useState<Identity | null>(null);
  // Dedupe concurrent refreshes so a burst of calls triggers one token renewal.
  const refreshInFlight = useRef<Promise<string | null> | null>(null);

  const applyResult = useCallback(
    (
      r: { identity: Identity; accessToken: string; refreshToken: string; expiresIn: number },
      persistent: boolean,
    ) => {
      setAccessToken(r.accessToken, r.expiresIn);
      setRefreshToken(r.refreshToken, persistent);
      if (r.identity.sub.length > 0) setIdentity(r.identity);
    },
    [],
  );

  // Renew the access token from the stored refresh token; null on failure.
  const renew = useCallback((): Promise<string | null> => {
    if (refreshInFlight.current !== null) return refreshInFlight.current;
    const rt = getRefreshToken();
    if (rt === null) return Promise.resolve(null);
    const p = (async () => {
      const result = await oidcRefresh(rt);
      if (result === null) {
        // A refresh token is single-use and server-revocable. Once rejected it
        // can never recover, so do not leave it behind to trigger another
        // failed /oauth/token request on every page load.
        clearSession();
        setIdentity(null);
        return null;
      }
      applyResult(result, refreshTokenIsPersistent());
      return result.accessToken;
    })().finally(() => {
      refreshInFlight.current = null;
    });
    refreshInFlight.current = p;
    return p;
  }, [applyResult]);

  const signOut = useCallback(async () => {
    const rt = getRefreshToken();
    clearSession();
    setIdentity(null);
    setStatus("anonymous");
    if (rt !== null) await revoke(rt);
  }, []);

  const signIn = useCallback(
    async (username: string, password: string, otp?: string, remember = false) => {
      const result = await oidcLogin(username, password, otp);
      applyResult(result, remember);
      setStatus("authenticated");
    },
    [applyResult],
  );

  const authorizedFetch = useCallback(
    async (input: string, init: RequestInit = {}): Promise<Response> => {
      let token = getAccessToken();
      if (accessTokenStale()) token = await renew();
      if (token === null) {
        await signOut();
        throw new Error("session expired");
      }
      const withAuth = (t: string): RequestInit => ({
        ...init,
        headers: { ...(init.headers ?? {}), authorization: `Bearer ${t}` },
      });
      let response = await apiFetch(input, withAuth(token));
      if (response.status === 401) {
        const fresh = await renew();
        if (fresh === null) {
          await signOut();
          throw new Error("session expired");
        }
        response = await apiFetch(input, withAuth(fresh));
      }
      return response;
    },
    [renew, signOut],
  );

  // Bootstrap: try to restore a session from a stored refresh token.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const token = await renew();
      if (cancelled) return;
      setStatus(token === null ? "anonymous" : "authenticated");
    })();
    return () => {
      cancelled = true;
    };
  }, [renew]);

  const value = useMemo<AuthContextValue>(
    () => ({ status, identity, signIn, signOut, authorizedFetch }),
    [status, identity, signIn, signOut, authorizedFetch],
  );

  return <AuthContext value={value}>{children}</AuthContext>;
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (ctx === null) throw new Error("useAuth must be used within <AuthProvider>");
  return ctx;
}
