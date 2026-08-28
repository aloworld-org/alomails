// Hands a file the server rendered to the browser's own "save" — the one
// path to a customer's PDF that needs no second way in.
//
// The document routes are authenticated with the session's bearer token, so a
// plain `<a href>` to them opens an anonymous tab and gets a `401`
// (`printSheet.ts` records the same reason). The file is fetched with the
// token and then offered under a short-lived `blob:` URL: the click is a
// download, not a navigation into the document, so the app's Content-Security-
// Policy — which keeps `blob:` out of frames — has nothing to say about it.

/** Offers `blob` to the user under `fileName`. */
export function saveFile(blob: Blob, fileName: string): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = fileName;
  link.rel = "noopener";
  document.body.appendChild(link);
  link.click();
  link.remove();
  // Revoked after the click has been handed off: revoking synchronously
  // cancels the download in some engines.
  window.setTimeout(() => URL.revokeObjectURL(url), 10_000);
}
