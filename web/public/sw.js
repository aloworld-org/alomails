// The alo offline-shell service worker (served verbatim from public/).
//
// Scope, on purpose: it precaches ONE thing — the self-contained offline
// screen — and intercepts ONLY top-level navigations, network-first. While
// the network answers, every response (the app shell, OAuth redirects, API
// calls) passes through untouched and uncached; only when the network is
// unreachable does a navigation get the honest offline screen instead of the
// browser's error page. Nothing else is ever cached: a stale app shell served
// from cache is worse than no PWA, and mail data offline is a sync engine,
// not a cache (both cuts recorded in docs/autonomy/mail/STATE.md).
//
// Updates: any byte change here makes the browser install the new worker on
// the next visit; skipWaiting + clients.claim make it take over immediately,
// and activate drops every previous version's cache. Bump VERSION whenever
// offline.html changes — that page is precached under this version, so an
// unbumped edit would keep serving the old copy to existing installs.

const VERSION = "v1";
const CACHE_NAME = "alo-offline-" + VERSION;
const OFFLINE_URL = "/offline.html";

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE_NAME)
      // "reload" bypasses the HTTP cache so the precached copy is the
      // deployed one, not whatever the browser had lying around.
      .then((cache) => cache.add(new Request(OFFLINE_URL, { cache: "reload" })))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(
          keys
            .filter((key) => key.startsWith("alo-offline-") && key !== CACHE_NAME)
            .map((key) => caches.delete(key)),
        ),
      )
      .then(() => self.clients.claim()),
  );
});

// Web Push (mail M5.3). The payload is the JMAP StateChange object — type
// names, an account id, an opaque state string; the server never puts a
// subject line, a sender or a body in it, so the notification here is
// generic on purpose and the app fetches real data when opened. Shown only
// when no alo window is focused: someone looking at the app does not need
// the operating system to repeat it.
const PUSH_TEXT = {
  en: { mail: "New mail", other: "Something new happened", open: "Open alo" },
  fr: { mail: "Nouveau message", other: "Du nouveau", open: "Ouvrir alo" },
  nl: { mail: "Nieuwe e-mail", other: "Er is iets nieuws", open: "alo openen" },
  de: { mail: "Neue E-Mail", other: "Es gibt Neuigkeiten", open: "alo öffnen" },
};

function pushText() {
  const lang = (self.navigator && self.navigator.language) || "en";
  return PUSH_TEXT[lang.slice(0, 2)] || PUSH_TEXT.en;
}

self.addEventListener("push", (event) => {
  let changedTypes = [];
  try {
    const payload = event.data ? event.data.json() : null;
    for (const account of Object.values((payload && payload.changed) || {})) {
      changedTypes = changedTypes.concat(Object.keys(account));
    }
  } catch {
    // An unreadable payload still means "something changed".
  }
  event.waitUntil(
    self.clients
      .matchAll({ type: "window", includeUncontrolled: true })
      .then((windows) => {
        if (windows.some((w) => w.focused)) return undefined;
        const text = pushText();
        const title =
          changedTypes.includes("Email") || changedTypes.includes("Mailbox")
            ? text.mail
            : text.other;
        return self.registration.showNotification(title, {
          body: text.open,
          tag: "alo-state",
          icon: "/icons/mail-192.png",
          data: { url: "/" },
        });
      }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  event.waitUntil(
    self.clients
      .matchAll({ type: "window", includeUncontrolled: true })
      .then((windows) => {
        const existing = windows[0];
        if (existing) return existing.focus();
        return self.clients.openWindow(
          (event.notification.data && event.notification.data.url) || "/",
        );
      }),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  // Navigations only. API calls, asset loads, auth redirects and everything
  // else never enter this worker — they go straight to the network.
  if (request.method !== "GET" || request.mode !== "navigate") return;
  event.respondWith(
    fetch(request).catch(() =>
      caches.match(OFFLINE_URL).then(
        (cached) =>
          cached ??
          // The precache failed or was evicted: a plain answer beats a hang.
          new Response("offline", {
            status: 503,
            headers: { "Content-Type": "text/plain" },
          }),
      ),
    ),
  );
});
