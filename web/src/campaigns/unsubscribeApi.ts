// The public half of an unsubscribe link (alo Campaigns, ADR 0044 §3, wave
// C2s.2).
//
// Plain `fetch`, not the authorized client — the same call `web/src/invite`
// makes and for the same reason: the person holding this link has no account,
// no session and nothing to refresh. The token in the URL is the whole
// credential, and adding an `Authorization` header would only send somebody
// else's session to a page that is not theirs.
//
// This file decides nothing. Which choices exist comes from the server (a send
// that named no kind of mail has no narrower choice to offer), and both choices
// are irreversible once made, so the browser never guesses at one.
import { API_BASE } from "../platform/runtime";

/** How much of a workspace's mail the recipient wants to stop. */
export type UnsubscribeScope = "topic" | "all";

/** What the landing page draws itself from. */
export interface UnsubscribeLink {
  /**
   * The kind of mail this link came from, as the sender wrote it, or `null`
   * when the send did not name one — in which case the page offers stopping
   * all of it and nothing else, rather than a narrower button that would
   * decline a category no send matches.
   */
  topic: string | null;
  /** Whether this workspace has already been told to stop mailing them. */
  stopped: boolean;
  /** Whether they have already declined this kind of mail. */
  topicDeclined: boolean;
}

/**
 * A refused unsubscribe request, carrying the server's own sentence.
 *
 * An unknown token, a malformed one and one this deployment never minted all
 * arrive as the same `404` with the same words, deliberately — telling them
 * apart would let somebody with guessed tokens learn which addresses exist.
 */
export class UnsubscribeError extends Error {
  readonly status: number;

  constructor(status: number, detail: string | null) {
    super(detail ?? `unsubscribe request failed (${status})`);
    this.name = "UnsubscribeError";
    this.status = status;
  }
}

async function answer(res: Response): Promise<UnsubscribeLink> {
  if (res.ok) return (await res.json()) as UnsubscribeLink;
  // The server authors these sentences to be read by the person who hit them,
  // so they are shown as they are.
  const detail = await res
    .json()
    .then((body: { detail?: unknown }) =>
      typeof body.detail === "string" ? body.detail : null,
    )
    .catch(() => null);
  throw new UnsubscribeError(res.status, detail);
}

function url(token: string): string {
  return `${API_BASE}/jmap/campaign-unsubscribe/${encodeURIComponent(token)}`;
}

/**
 * What this link is for. **Reads only** — the server writes nothing on this
 * call, because every link-prefetching scanner between the sender and the
 * recipient fetches the URL before a human sees it (RFC 8058 requires the
 * acting request to be a POST for exactly that reason).
 */
export function unsubscribeLink(token: string): Promise<UnsubscribeLink> {
  return fetch(url(token)).then(answer);
}

/**
 * Stops this kind of mail, or all of it. One call per press, no confirmation
 * step: a maze on an unsubscribe is how somebody ends up pressing "spam"
 * instead.
 */
export function unsubscribe(
  token: string,
  scope: UnsubscribeScope,
): Promise<UnsubscribeLink> {
  return fetch(url(token), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ scope }),
  }).then(answer);
}
