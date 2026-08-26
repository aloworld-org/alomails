// The OIDC/OAuth client for the first-party web app. `alo-identity` treats
// this app as a PUBLIC client (no secret) and requires PKCE S256. Unusually
// for OIDC, its /oauth/authorize accepts the credentials as a POST form (the
// IdP renders no login page of its own), so THIS app owns the login UI and
// posts here; on success the server 302-redirects to our redirect URI with a
// code, which we exchange for tokens. Endpoints resolve against `API_BASE`
// (same-origin in the browser; the hosted server in the desktop app).
import { API_BASE, apiFetch } from "../platform/runtime";
import { challengeFor, createState, createVerifier } from "./pkce";
import { decodeIdentity } from "./session";
import type { Identity } from "./session";

const config = {
  clientId: "web",
  scope: "openid email profile",
  authorizeEndpoint: `${API_BASE}/oauth/authorize`,
  tokenEndpoint: `${API_BASE}/oauth/token`,
  revokeEndpoint: `${API_BASE}/oauth/revoke`,
  get redirectUri(): string {
    return `${API_BASE}/auth/callback`;
  },
};

export type AuthErrorKind =
  | "bad_credentials"
  | "second_factor"
  | "rate_limited"
  | "network"
  | "generic";

export class AuthError extends Error {
  readonly kind: AuthErrorKind;
  constructor(kind: AuthErrorKind, message?: string) {
    super(message ?? kind);
    this.name = "AuthError";
    this.kind = kind;
  }
}

interface TokenResponse {
  access_token: string;
  id_token?: string;
  refresh_token: string;
  expires_in: number;
}

export interface LoginResult {
  identity: Identity;
  accessToken: string;
  refreshToken: string;
  expiresIn: number;
}

/** Run the authorization-code + PKCE flow with the user's credentials. */
export async function login(
  username: string,
  password: string,
  otp?: string,
): Promise<LoginResult> {
  const verifier = createVerifier();
  const challenge = await challengeFor(verifier);
  const state = createState();

  const body = new URLSearchParams({
    response_type: "code",
    client_id: config.clientId,
    redirect_uri: config.redirectUri,
    scope: config.scope,
    state,
    nonce: createState(),
    code_challenge: challenge,
    code_challenge_method: "S256",
    username,
    password,
  });
  if (otp !== undefined && otp.length > 0) body.set("otp", otp);

  let response: Response;
  try {
    response = await apiFetch(config.authorizeEndpoint, {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body,
      redirect: "follow",
    });
  } catch {
    throw new AuthError("network");
  }

  if (response.status === 401) {
    const description = await errorDescription(response);
    if (/second factor|otp|2fa/i.test(description)) throw new AuthError("second_factor");
    throw new AuthError("bad_credentials");
  }
  if (response.status === 429) throw new AuthError("rate_limited");
  // A stopped or unhealthy backend is not a credential failure. Vite's dev
  // proxy historically surfaced a refused upstream as 500, so classify every
  // server-side failure as availability and give the login page its specific
  // connection message instead of the generic sign-in error.
  if (response.status >= 500) throw new AuthError("network");

  // Success followed the 302 to our redirect URI; the code (or an error) is in
  // the final URL. The redirect target is served as the SPA shell (200).
  const finalUrl = new URL(response.url);
  const params = finalUrl.searchParams;
  const returnedError = params.get("error");
  if (returnedError !== null) {
    if (returnedError === "access_denied") throw new AuthError("bad_credentials");
    throw new AuthError("generic", returnedError);
  }
  const code = params.get("code");
  if (code === null) throw new AuthError("generic", "no authorization code returned");
  if (params.get("state") !== state) throw new AuthError("generic", "state mismatch");

  const tokens = await exchangeCode(code, verifier);
  const identity =
    tokens.id_token !== undefined ? decodeIdentity(tokens.id_token) : null;
  if (identity === null) throw new AuthError("generic", "no identity in token");

  return {
    identity,
    accessToken: tokens.access_token,
    refreshToken: tokens.refresh_token,
    expiresIn: tokens.expires_in,
  };
}

async function exchangeCode(code: string, verifier: string): Promise<TokenResponse> {
  const body = new URLSearchParams({
    grant_type: "authorization_code",
    code,
    client_id: config.clientId,
    redirect_uri: config.redirectUri,
    code_verifier: verifier,
  });
  const response = await fetchToken(body);
  if (!response.ok) throw new AuthError("generic", "token exchange failed");
  return (await response.json()) as TokenResponse;
}

/** Exchange a refresh token for a fresh access token (rotates the refresh
 * token server-side). Returns null on any failure so the caller re-logs in. */
export async function refresh(refreshToken: string): Promise<LoginResult | null> {
  const body = new URLSearchParams({
    grant_type: "refresh_token",
    refresh_token: refreshToken,
    client_id: config.clientId,
    scope: config.scope,
  });
  let response: Response;
  try {
    response = await fetchToken(body);
  } catch {
    return null;
  }
  if (!response.ok) return null;
  const tokens = (await response.json()) as TokenResponse;
  const identity =
    tokens.id_token !== undefined ? decodeIdentity(tokens.id_token) : null;
  return {
    identity: identity ?? { sub: "", email: "", name: "" },
    accessToken: tokens.access_token,
    refreshToken: tokens.refresh_token,
    expiresIn: tokens.expires_in,
  };
}

/** Best-effort revoke of the refresh token on sign-out. */
export async function revoke(refreshToken: string): Promise<void> {
  try {
    await apiFetch(config.revokeEndpoint, {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        token: refreshToken,
        token_type_hint: "refresh_token",
        client_id: config.clientId,
      }),
    });
  } catch {
    // Sign-out proceeds locally regardless; the token also expires server-side.
  }
}

function fetchToken(body: URLSearchParams): Promise<Response> {
  return apiFetch(config.tokenEndpoint, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body,
  });
}

async function errorDescription(response: Response): Promise<string> {
  try {
    const data = (await response.json()) as { error_description?: string; error?: string };
    return data.error_description ?? data.error ?? "";
  } catch {
    return "";
  }
}
