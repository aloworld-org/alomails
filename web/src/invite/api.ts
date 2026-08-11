// The public half of a workspace invitation (migration 0209).
//
// Plain `fetch`, not the authorized client: the person holding the link has no
// account until they spend it, so there is no bearer token to send and no
// session to refresh. The token in the URL is the whole credential.
import { API_BASE } from "../platform/runtime";

/** Who an invitation is for. The server sends the address and nothing else. */
export interface Invitation {
  email: string;
}

/**
 * A refused invitation, carrying the server's own sentence.
 *
 * Unknown, spent and expired all arrive as the same `404` with the same words,
 * deliberately — telling them apart would let somebody with guessed tokens
 * learn which were ever issued.
 */
export class InviteError extends Error {
  readonly status: number;

  constructor(status: number, detail: string | null) {
    super(detail ?? `invitation request failed (${status})`);
    this.name = "InviteError";
    this.status = status;
  }
}

async function answer<T>(res: Response): Promise<T> {
  if (res.ok) return (await res.json()) as T;
  // The server authors these sentences to be read by the person who hit them,
  // so they are safe to show as they are.
  const detail = await res
    .json()
    .then((body: { detail?: unknown }) =>
      typeof body.detail === "string" ? body.detail : null,
    )
    .catch(() => null);
  throw new InviteError(res.status, detail);
}

/** Who this invitation is for, for the setup screen. */
export function invitation(token: string): Promise<Invitation> {
  return fetch(`${API_BASE}/jmap/invite/${encodeURIComponent(token)}`).then(
    answer<Invitation>,
  );
}

/**
 * Sets the password the invited person chose, records the recovery address
 * they named, and spends the link.
 *
 * The recovery address is required rather than optional: an account that can
 * be signed into and never recovered is the state this whole feature exists to
 * end, and acceptance is the only moment the person is present to name one.
 */
export function accept(
  token: string,
  password: string,
  recoveryEmail: string,
): Promise<Invitation> {
  return fetch(`${API_BASE}/jmap/invite/${encodeURIComponent(token)}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ password, recoveryEmail }),
  }).then(answer<Invitation>);
}
