// Putting a server-rendered billing document in front of the browser's print
// dialog (alo Billing, ADR 0035, wave B1.16).
//
// The page itself comes from the server, already complete and already laid out
// for A4 (`docs/design/billing.md`): this file's whole job is to hand it to the
// browser without leaving the app.
//
// **A hidden `srcdoc` iframe, not a new tab and not a blob URL.**
//
//   - A new tab would need a URL, and the print route is authenticated with the
//     session's bearer token — a plain link opens an anonymous tab and gets a
//     `401`. Printing a document is not a reason to invent a second way in.
//   - A `blob:` URL is blocked by our own Content-Security-Policy: the app is
//     served with `default-src 'self'` (and `frame-src 'self'` on the workspace
//     surface), and `blob:` is not `'self'`. `srcdoc` inherits the parent's
//     policy instead of being matched against it, and the document's inline
//     stylesheet is allowed by the `style-src 'unsafe-inline'` already in that
//     policy.
//   - Printing the app's own window would print the app around the document.

/** How long a frame may sit in the page after the dialog is dismissed.
 *  Firefox and Safari return from `print()` immediately, so removal cannot
 *  simply follow the call; `afterprint` is the signal, and this is the
 *  backstop for a browser that never fires it — or never fires `load`, which
 *  is why the timer is armed before the frame is mounted rather than inside
 *  the load handler. A customer's address, totals and our own bank account do
 *  not sit in the DOM for the rest of the session because an event was missed.
 */
const CLEANUP_MS = 60_000;

/**
 * Hands one already-rendered HTML document to the browser's print dialog.
 *
 * Returns as soon as the dialog has been asked for; no browser reports whether
 * anything was actually printed, so there is nothing to await and nothing to
 * throw — a caller's error handling belongs on the *fetch* that produced the
 * HTML.
 *
 * The frame is **sandboxed without `allow-scripts`**, so no script in the
 * document can run at all. That, not the response's own headers, is what makes
 * the page inert here: a `srcdoc` document is same-origin with the app and the
 * XHR response's `Content-Security-Policy` never applies to it — it inherits
 * the app's policy instead (`deploy/production/Caddyfile`). `allow-same-origin`
 * is kept so this window can reach `contentWindow`, and `allow-modals` because
 * the print dialog is one.
 */
export function printSheet(html: string): void {
  const frame = document.createElement("iframe");
  // Off-screen rather than `display: none`: a frame that is not laid out has
  // no document to print in some engines.
  frame.setAttribute("aria-hidden", "true");
  frame.setAttribute("sandbox", "allow-same-origin allow-modals");
  frame.style.position = "fixed";
  frame.style.right = "0";
  frame.style.bottom = "0";
  frame.style.width = "0";
  frame.style.height = "0";
  frame.style.border = "0";
  frame.style.visibility = "hidden";

  let done = false;
  const remove = () => {
    if (done) return;
    done = true;
    frame.remove();
  };

  // Armed before the frame is mounted, so a `load` that never comes cannot
  // leave the document in the page.
  window.setTimeout(remove, CLEANUP_MS);

  frame.addEventListener("load", () => {
    const view = frame.contentWindow;
    if (view === null) {
      remove();
      return;
    }
    view.addEventListener("afterprint", remove);
    view.focus();
    view.print();
  });

  frame.srcdoc = html;
  document.body.appendChild(frame);
}
