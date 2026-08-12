// Which stack am I looking at?
//
// Three of these run side by side during development — one per agent working
// on the repo, plus production — and in the browser they are pixel-identical.
// On 2026-08-12 that cost two separate debugging sessions: a correct password
// "stopped working" because the browser was on another checkout's dev server
// talking to another database, and a chat message "never arrived" because it
// had been sent to production while the screen showed localhost.
//
// Both times every layer was working correctly. The only thing that could have
// told anybody was the address bar, and nobody looks at the address bar when
// the app is on screen. So the app says it out loud instead.

/** What a dev build shows about where it is pointed, or `null` to show nothing. */
export interface StackLabel {
  /** Short, glanceable: "5174 → 8080". */
  text: string;
  /** The whole truth, for a tooltip. */
  detail: string;
}

/**
 * The label for a page served from `origin`, talking to `apiBase`.
 *
 * A pure function of two strings so it can be tested without a browser, and so
 * the decision about *what* to say is separable from *whether* to say it.
 *
 * Returns `null` for anything that is not a local development origin. A
 * deployed alo must never wear this badge: it would leak internal topology to
 * customers and, worse, train people to ignore it.
 */
export function stackLabel(origin: string, apiBase: string): StackLabel | null {
  let page: URL;
  let api: URL;
  try {
    page = new URL(origin);
    api = new URL(apiBase);
  } catch {
    return null;
  }
  const local = (host: string) =>
    host === "localhost" || host === "127.0.0.1" || host === "[::1]";
  if (!local(page.hostname)) return null;

  const pagePort =
    page.port === "" ? (page.protocol === "https:" ? "443" : "80") : page.port;
  // Same-origin is the ordinary browser case: the dev server proxies the API,
  // so the port that matters is the one Vite forwards to — which this cannot
  // see. Saying "5173 → itself" would be worse than useless, so the label
  // names the page and leaves the rest to the tooltip.
  const sameOrigin = page.origin === api.origin;
  const apiPort =
    api.port === "" ? (api.protocol === "https:" ? "443" : "80") : api.port;

  return {
    text: sameOrigin ? `dev :${pagePort}` : `dev :${pagePort} → :${apiPort}`,
    detail: sameOrigin
      ? `Page ${page.origin} · API proxied by this dev server. Check vite.config.ts if you are unsure which backend that is.`
      : `Page ${page.origin} · API ${api.origin}`,
  };
}
