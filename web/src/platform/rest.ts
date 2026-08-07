// What every alo REST surface (`/billing`, `/crm`, …) fails with, in one place.
//
// The server answers a refusal with a `Problem` whose `detail` is its own
// sentence — authored to name the rule that was broken and never to echo stored
// data, so it is safe to put in front of a user. `status` lets a caller tell
// "you typed something impossible" (422) from "that record is gone" (404)
// without parsing prose.
//
// This module holds the failure shape only, deliberately: the request plumbing
// of each surface differs (which routes are text, which carry a `?lang=`), but
// the way a failure reaches a user must not, or two modules end up reporting
// the same server sentence two different ways.

/** A failed request to an alo REST surface. */
export class RestError extends Error {
  readonly status: number;
  readonly detail: string | null;

  constructor(status: number, detail: string | null, name = "RestError") {
    super(detail ?? `request failed (${status})`);
    this.name = name;
    this.status = status;
    this.detail = detail;
  }
}

/**
 * What to show a user about a failed request: the server's own sentence when it
 * sent one, and `fallback` otherwise (a dropped connection, or a failure whose
 * reason is not the user's business).
 */
export function restMessage(error: unknown, fallback: string): string {
  return error instanceof RestError && error.detail !== null ? error.detail : fallback;
}

/** Reads the `Problem` detail out of a failed response, or `null` when the body
 *  is not one (a proxy's HTML error page, an empty 502). Never throws. */
export async function problemDetail(res: Response): Promise<string | null> {
  const problem = (await res.json().catch(() => ({}))) as { detail?: unknown };
  return typeof problem.detail === "string" ? problem.detail : null;
}
