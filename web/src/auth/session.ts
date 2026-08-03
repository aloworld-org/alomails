// Session storage and identity decoding.
//
// Token storage (v1, see docs/design/web-shell.md): the access token lives in
// memory only; the refresh token in sessionStorage so a page reload keeps the
// session without persisting to disk across tab-close. The server issues
// opaque, revocable access tokens and rotates refresh tokens with replay-chain
// revocation, so a stolen refresh token is detected on reuse. The named
// hardening step is a backend-for-frontend holding tokens in an httpOnly
// cookie (out of JS reach) — recorded, not yet built.

const REFRESH_KEY = "alo.rt";

export interface Identity {
  /** OIDC subject — the stable user id. */
  sub: string;
  email: string;
  /** Display name; falls back to the email local-part. */
  name: string;
}

export interface Tokens {
  accessToken: string;
  refreshToken: string;
  /** Epoch millis when the access token expires. */
  expiresAt: number;
}

// Access token: memory only.
let accessToken: string | null = null;
let accessExpiresAt = 0;

export function setAccessToken(token: string, expiresInSeconds: number): void {
  accessToken = token;
  accessExpiresAt = Date.now() + expiresInSeconds * 1000;
}

export function getAccessToken(): string | null {
  return accessToken;
}

/** True when the access token is missing or within 30s of expiry. */
export function accessTokenStale(): boolean {
  return accessToken === null || Date.now() > accessExpiresAt - 30_000;
}

// Refresh token: sessionStorage by default; localStorage when the user chose
// "Remember me" (survives browser restart). Only one of the two ever holds it.
export function setRefreshToken(token: string, persistent = false): void {
  try {
    const [store, other] = persistent
      ? [localStorage, sessionStorage]
      : [sessionStorage, localStorage];
    store.setItem(REFRESH_KEY, token);
    other.removeItem(REFRESH_KEY);
  } catch {
    // Storage disabled (private mode quota) — the session simply won't survive
    // reload; the app still works for the active tab via the in-memory token.
  }
}

export function getRefreshToken(): string | null {
  try {
    return localStorage.getItem(REFRESH_KEY) ?? sessionStorage.getItem(REFRESH_KEY);
  } catch {
    return null;
  }
}

/** Whether the stored refresh token is the persistent ("remember me") one, so
 * a token renewal keeps it in the same place. */
export function refreshTokenIsPersistent(): boolean {
  try {
    return localStorage.getItem(REFRESH_KEY) !== null;
  } catch {
    return false;
  }
}

export function clearSession(): void {
  accessToken = null;
  accessExpiresAt = 0;
  try {
    sessionStorage.removeItem(REFRESH_KEY);
    localStorage.removeItem(REFRESH_KEY);
  } catch {
    // ignore
  }
}

/** Decode a JWT payload without verifying its signature.
 *
 * The ID token is received directly from our own token endpoint over TLS
 * (same origin), so per OIDC it may be consumed without client-side signature
 * validation; the opaque access token — which the server validates on every
 * call — is what actually authorizes requests. We only read identity claims
 * here. */
export function decodeIdentity(idToken: string): Identity | null {
  const parts = idToken.split(".");
  if (parts.length !== 3) return null;
  try {
    const json = atob(parts[1]!.replace(/-/g, "+").replace(/_/g, "/"));
    const claims = JSON.parse(json) as Record<string, unknown>;
    const sub = typeof claims.sub === "string" ? claims.sub : null;
    if (sub === null) return null;
    const email = typeof claims.email === "string" ? claims.email : "";
    const name =
      typeof claims.name === "string" && claims.name.length > 0
        ? claims.name
        : email.split("@")[0] || email;
    return { sub, email, name };
  } catch {
    return null;
  }
}
