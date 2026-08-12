// The public signup API (ADR 0018). Unauthenticated — a person has no
// credentials yet — so these are plain fetches to the API (`apiFetch`/`API_BASE`
// resolve same-origin in the browser and the hosted server in the desktop app),
// not the bearer-authenticated JMAP client. Errors carry the server's `detail`
// string where present so the page can show a specific reason.
import { API_BASE, apiFetch } from "../platform/runtime";

/** A signup request failed; `message` is safe to show the user. */
export class SignupError extends Error {}

async function post<T>(path: string, body: unknown): Promise<T> {
  let res: Response;
  try {
    res = await apiFetch(`${API_BASE}/api${path}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  } catch {
    throw new SignupError("network");
  }
  const data = (await res.json().catch(() => ({}))) as { detail?: string } & Record<string, unknown>;
  if (!res.ok) {
    throw new SignupError(data.detail ?? `error ${res.status}`);
  }
  return data as T;
}

/** The domains open to personal signup. Empty means signup is disabled. */
export async function signupDomains(): Promise<string[]> {
  try {
    const res = await apiFetch(`${API_BASE}/api/signup/domains`);
    if (!res.ok) return [];
    const data = (await res.json()) as { domains?: string[] };
    return data.domains ?? [];
  } catch {
    return [];
  }
}

/** Whether `address` can be claimed, with a machine reason. */
export function signupAvailable(
  address: string,
): Promise<{ available: boolean; reason: string }> {
  return post("/signup/available", { address });
}

/** Reserve the address and email a verification code to the recovery mailbox. */
export function signupBegin(address: string, recoveryEmail: string): Promise<{ status: string }> {
  return post("/signup/begin", { address, recoveryEmail });
}

/** Verify the code and provision the account. */
export function signupVerify(
  address: string,
  code: string,
  password: string,
): Promise<{ accountId: string; email: string }> {
  return post("/signup/verify", { address, code, password });
}

/** Request a password-reset code to the account's recovery mailbox. Always
 *  resolves the same way — the server never reveals whether the account (or a
 *  recovery mailbox for it) exists, so the page must not either. */
export function resetRequest(address: string): Promise<{ status: string }> {
  return post("/reset/request", { address });
}

/** Verify the reset code and set the new password. */
export function resetVerify(
  address: string,
  code: string,
  password: string,
): Promise<{ status: string }> {
  return post("/reset/verify", { address, code, password });
}
