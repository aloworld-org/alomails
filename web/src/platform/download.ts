// Saving a server-rendered export to the user's machine (alo Billing, ADR 0035,
// wave B1.20; alo CRM, B2.08).
//
// It lives in `platform` because it is a browser mechanism, not a module's
// rule: the VAT summary and the pipeline report save a file the same way, and
// the second caller is what moved it out of billing rather than copying it.
//
// The export itself comes from the server, already complete: this file's whole
// job is to put the text the API answered into a file the browser saves,
// without leaving the app.
//
// **Fetched, then saved from memory — never linked.** The report routes are
// authenticated with the session's bearer token, so a plain `<a href>` would
// open an anonymous request and download a `401` page named `vat.csv`. The
// caller fetches the text (which is where a failure is reported, in the
// server's own words) and hands it here.
//
// The object URL is revoked on the next task rather than immediately: Safari
// cancels a download whose URL is revoked in the same tick as the click.

/** How long the object URL is kept alive after the click. Long enough for
 *  every engine to have started the download, short enough that the bytes —
 *  a tenant's turnover — are not held in memory for the session. */
const REVOKE_MS = 1_000;

/**
 * Saves `text` as a file called `fileName`.
 *
 * `mediaType` is the type the blob is labelled with; it decides what the
 * browser (and the operating system, once saved) thinks the file is.
 */
export function saveTextFile(text: string, fileName: string, mediaType: string): void {
  const url = URL.createObjectURL(new Blob([text], { type: mediaType }));
  // Keep the implementation that created this object URL. Test environments
  // and embedded webviews can replace the global URL object before the delayed
  // cleanup runs; looking it up again then leaks the URL or throws.
  const revokeObjectURL = URL.revokeObjectURL.bind(URL);
  const link = document.createElement("a");
  link.href = url;
  link.download = fileName;
  link.rel = "noopener";
  link.click();
  window.setTimeout(() => revokeObjectURL(url), REVOKE_MS);
}
