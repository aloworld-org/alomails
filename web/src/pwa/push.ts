// The browser half of Web Push (mail M5.3): permission, subscription and
// the wire shape the server stores. The service worker (public/sw.js) shows
// the notifications; the settings screen calls these to opt a device in or
// out. Kept apart from any UI so the permission flow is testable with a
// mocked navigator — the real browser prompt is the owner's manual check.

/** The W3C `PushSubscription.toJSON()` shape the server stores per device. */
export interface WirePushSubscription {
  endpoint: string;
  keys: { p256dh: string; auth: string };
}

/** Decodes the server's base64url VAPID public key into the byte array
 * `pushManager.subscribe` wants as `applicationServerKey`. */
export function applicationServerKey(publicKeyB64: string): Uint8Array {
  const padded = publicKeyB64 + "=".repeat((4 - (publicKeyB64.length % 4)) % 4);
  const raw = atob(padded.replace(/-/g, "+").replace(/_/g, "/"));
  const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i += 1) bytes[i] = raw.charCodeAt(i);
  return bytes;
}

/** Whether this browser can do Web Push at all (service worker +
 * PushManager + Notification). */
export function pushSupported(nav: Navigator = navigator): boolean {
  return (
    "serviceWorker" in nav &&
    typeof Notification !== "undefined" &&
    typeof PushManager !== "undefined"
  );
}

/** This browser's current subscription, or null when it has none. */
export async function currentSubscription(
  nav: Navigator = navigator,
): Promise<PushSubscription | null> {
  if (!("serviceWorker" in nav)) return null;
  const registration = await nav.serviceWorker.ready;
  return registration.pushManager.getSubscription();
}

/** Asks permission if needed and subscribes this browser, returning the
 * wire shape to register server-side. Null when the person declined the
 * permission prompt — a choice, not an error. */
export async function subscribeThisDevice(
  publicKeyB64: string,
  nav: Navigator = navigator,
): Promise<WirePushSubscription | null> {
  const permission = await Notification.requestPermission();
  if (permission !== "granted") return null;
  const registration = await nav.serviceWorker.ready;
  const subscription = await registration.pushManager.subscribe({
    userVisibleOnly: true,
    applicationServerKey: applicationServerKey(publicKeyB64).buffer as ArrayBuffer,
  });
  const json = subscription.toJSON();
  if (!json.endpoint || !json.keys?.p256dh || !json.keys.auth) {
    // A subscription the server could never deliver to: undo it here.
    await subscription.unsubscribe();
    return null;
  }
  return {
    endpoint: json.endpoint,
    keys: { p256dh: json.keys.p256dh, auth: json.keys.auth },
  };
}

/** Unsubscribes this browser, returning the endpoint that was let go (so
 * the caller can delete the matching server record), or null if there was
 * nothing to undo. */
export async function unsubscribeThisDevice(
  nav: Navigator = navigator,
): Promise<string | null> {
  const subscription = await currentSubscription(nav);
  if (!subscription) return null;
  const endpoint = subscription.endpoint;
  await subscription.unsubscribe();
  return endpoint;
}
