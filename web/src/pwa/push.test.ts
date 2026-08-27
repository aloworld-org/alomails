// The browser half of Web Push (mail M5.3), proven structurally: the
// permission → subscribe → wire-shape flow against a mocked Push API. The
// real browser prompt and OS notification are the owner's manual check
// (recorded in STATE.md); what tests CAN hold is that we subscribe with the
// server's key and userVisibleOnly, respect a "no", and never hand the
// server a subscription it could not deliver to.
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  applicationServerKey,
  currentSubscription,
  pushSupported,
  subscribeThisDevice,
  unsubscribeThisDevice,
} from "./push";

afterEach(() => {
  vi.unstubAllGlobals();
});

/** A navigator whose service worker registration carries the given push
 * manager state. */
function fakeNavigator(subscription: unknown, subscribe = vi.fn()) {
  const registration = {
    pushManager: {
      getSubscription: vi.fn(() => Promise.resolve(subscription)),
      subscribe,
    },
  };
  return {
    nav: { serviceWorker: { ready: Promise.resolve(registration) } } as unknown as Navigator,
    registration,
  };
}

function stubNotificationPermission(result: NotificationPermission) {
  vi.stubGlobal("Notification", {
    requestPermission: vi.fn(() => Promise.resolve(result)),
  });
}

describe("applicationServerKey", () => {
  it("decodes unpadded base64url into the exact bytes", () => {
    // "AQID_-4" is base64url for [1, 2, 3, 255, 238].
    expect([...applicationServerKey("AQID_-4")]).toEqual([1, 2, 3, 255, 238]);
  });

  it("round-trips a realistic 65-byte P-256 point", () => {
    const bytes = new Uint8Array(65).map((_, i) => (i * 7) % 256);
    const b64 = btoa(String.fromCharCode(...bytes))
      .replace(/\+/g, "-")
      .replace(/\//g, "_")
      .replace(/=+$/, "");
    expect([...applicationServerKey(b64)]).toEqual([...bytes]);
  });
});

describe("pushSupported", () => {
  it("needs serviceWorker, Notification and PushManager", () => {
    vi.stubGlobal("Notification", {});
    vi.stubGlobal("PushManager", function PushManager() {});
    expect(pushSupported({ serviceWorker: {} } as unknown as Navigator)).toBe(
      true,
    );
    expect(pushSupported({} as unknown as Navigator)).toBe(false);
  });
});

describe("subscribeThisDevice", () => {
  it("subscribes with userVisibleOnly and the server's key, and returns the wire shape", async () => {
    stubNotificationPermission("granted");
    const subscribe = vi.fn(() =>
      Promise.resolve({
        toJSON: () => ({
          endpoint: "https://push.example/send/dev",
          keys: { p256dh: "pk", auth: "au" },
        }),
        unsubscribe: vi.fn(),
      }),
    );
    const { nav } = fakeNavigator(null, subscribe);

    const wire = await subscribeThisDevice("AQID", nav);
    expect(wire).toEqual({
      endpoint: "https://push.example/send/dev",
      keys: { p256dh: "pk", auth: "au" },
    });
    const options = (subscribe.mock.calls[0] as unknown[])[0] as {
      userVisibleOnly: boolean;
      applicationServerKey: ArrayBuffer;
    };
    // userVisibleOnly is the promise the browser holds us to: every push
    // shows something, none snoops silently.
    expect(options.userVisibleOnly).toBe(true);
    expect([...new Uint8Array(options.applicationServerKey)]).toEqual([
      1, 2, 3,
    ]);
  });

  it("returns null without subscribing when the person declines", async () => {
    stubNotificationPermission("denied");
    const subscribe = vi.fn();
    const { nav } = fakeNavigator(null, subscribe);
    expect(await subscribeThisDevice("AQID", nav)).toBeNull();
    expect(subscribe).not.toHaveBeenCalled();
  });

  it("undoes a subscription the server could never deliver to", async () => {
    stubNotificationPermission("granted");
    const unsubscribe = vi.fn(() => Promise.resolve(true));
    const subscribe = vi.fn(() =>
      Promise.resolve({
        // A browser that withheld the keys: registering this server-side
        // would store a device nothing can encrypt toward.
        toJSON: () => ({ endpoint: "https://push.example/x", keys: {} }),
        unsubscribe,
      }),
    );
    const { nav } = fakeNavigator(null, subscribe);
    expect(await subscribeThisDevice("AQID", nav)).toBeNull();
    expect(unsubscribe).toHaveBeenCalled();
  });
});

describe("unsubscribeThisDevice / currentSubscription", () => {
  it("lets the subscription go and reports which endpoint it was", async () => {
    const unsubscribe = vi.fn(() => Promise.resolve(true));
    const { nav } = fakeNavigator({
      endpoint: "https://push.example/send/old",
      unsubscribe,
    });
    expect(await unsubscribeThisDevice(nav)).toBe(
      "https://push.example/send/old",
    );
    expect(unsubscribe).toHaveBeenCalled();
  });

  it("is a clean no-op when nothing was subscribed", async () => {
    const { nav } = fakeNavigator(null);
    expect(await unsubscribeThisDevice(nav)).toBeNull();
    expect(await currentSubscription(nav)).toBeNull();
  });
});
