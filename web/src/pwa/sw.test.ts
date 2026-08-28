// The offline-shell contract, held by tests. sw.js is a plain script served
// from public/ (no build step may rewrite it), so the suite evaluates its real
// source inside a mock service-worker scope and drives the lifecycle the
// browser would: install precaches the offline screen, activate drops every
// previous version's cache (the version-bump path), and fetch touches ONLY
// failed navigations — API calls, asset loads and successful responses (auth
// redirects included) pass through untouched and uncached.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it, vi } from "vitest";

import { registerOfflineShell } from "./registerSw";

const HERE = dirname(fileURLToPath(import.meta.url));
const SW_SOURCE = readFileSync(join(HERE, "../../public/sw.js"), "utf8");
const OFFLINE_HTML = readFileSync(
  join(HERE, "../../public/offline.html"),
  "utf8",
);

const VERSION = /const VERSION = "([^"]+)"/.exec(SW_SOURCE)?.[1];
const CACHE_NAME = `alo-offline-${VERSION}`;

/** What FakeCache stores per precached URL. */
interface CachedEntry {
  url: string;
  init: { cache?: string } | undefined;
}

class FakeCache {
  store = new Map<string, CachedEntry>();
  async add(request: FakeRequest): Promise<void> {
    this.store.set(request.url, { url: request.url, init: request.init });
  }
  async match(url: string): Promise<CachedEntry | undefined> {
    return this.store.get(url);
  }
}

class FakeCaches {
  map = new Map<string, FakeCache>();
  async open(name: string): Promise<FakeCache> {
    let cache = this.map.get(name);
    if (!cache) {
      cache = new FakeCache();
      this.map.set(name, cache);
    }
    return cache;
  }
  async keys(): Promise<string[]> {
    return [...this.map.keys()];
  }
  async delete(name: string): Promise<boolean> {
    return this.map.delete(name);
  }
  async match(url: string): Promise<CachedEntry | undefined> {
    for (const cache of this.map.values()) {
      const hit = await cache.match(url);
      if (hit) return hit;
    }
    return undefined;
  }
}

class FakeRequest {
  constructor(
    public url: string,
    public init?: { cache?: string },
  ) {}
}

type Listener = (event: {
  waitUntil: (p: Promise<unknown>) => void;
  request?: unknown;
  respondWith?: (p: Promise<unknown> | unknown) => void;
}) => void;

/** Evaluate the real sw.js in a mock worker scope; return its seams. */
function loadWorker(fetchImpl: (req: unknown) => Promise<unknown> = () => Promise.resolve({})) {
  const listeners: Record<string, Listener> = {};
  const caches = new FakeCaches();
  const self = {
    addEventListener: (type: string, fn: Listener) => {
      listeners[type] = fn;
    },
    skipWaiting: vi.fn(async () => {}),
    clients: { claim: vi.fn(async () => {}) },
  };
  const fetchMock = vi.fn(fetchImpl);
  new Function("self", "caches", "fetch", "Request", "Response", SW_SOURCE)(
    self,
    caches,
    fetchMock,
    FakeRequest,
    Response,
  );
  const dispatch = async (type: "install" | "activate") => {
    const waits: Promise<unknown>[] = [];
    listeners[type]?.({ waitUntil: (p) => waits.push(p) });
    expect(waits.length).toBeGreaterThan(0);
    await Promise.all(waits);
  };
  const navigate = async (request: {
    method: string;
    mode: string;
    url: string;
  }) => {
    let responded: Promise<unknown> | undefined;
    listeners.fetch?.({
      waitUntil: () => {},
      request,
      respondWith: (p) => {
        responded = Promise.resolve(p);
      },
    });
    return responded;
  };
  return { listeners, caches, self, fetch: fetchMock, dispatch, navigate };
}

describe("sw.js lifecycle", () => {
  it("declares a version and derives the cache name from it", () => {
    expect(VERSION).toBeTruthy();
    expect(SW_SOURCE).toContain('"alo-offline-" + VERSION');
  });

  it("install precaches exactly the offline screen, bypassing the HTTP cache, then skips waiting", async () => {
    const sw = loadWorker();
    await sw.dispatch("install");
    expect([...sw.caches.map.keys()]).toEqual([CACHE_NAME]);
    const entry = await sw.caches.match("/offline.html");
    expect(entry?.init?.cache).toBe("reload");
    expect(sw.caches.map.get(CACHE_NAME)?.store.size).toBe(1);
    expect(sw.self.skipWaiting).toHaveBeenCalled();
  });

  it("activate drops previous versions' caches, keeps the current one and foreign caches, and claims clients", async () => {
    const sw = loadWorker();
    // A previous deploy's cache plus a cache this worker does not own.
    sw.caches.map.set("alo-offline-v0", new FakeCache());
    sw.caches.map.set("someone-elses-cache", new FakeCache());
    await sw.dispatch("install");
    await sw.dispatch("activate");
    expect((await sw.caches.keys()).sort()).toEqual(
      [CACHE_NAME, "someone-elses-cache"].sort(),
    );
    expect(sw.self.clients.claim).toHaveBeenCalled();
  });
});

describe("sw.js fetch handling", () => {
  it("never intercepts API/XHR requests or non-GET navigations", async () => {
    const sw = loadWorker();
    await sw.dispatch("install");
    const xhr = await sw.navigate({ method: "GET", mode: "cors", url: "/api/mail" });
    const sameOrigin = await sw.navigate({
      method: "GET",
      mode: "same-origin",
      url: "/jmap",
    });
    const post = await sw.navigate({ method: "POST", mode: "navigate", url: "/" });
    expect(xhr).toBeUndefined();
    expect(sameOrigin).toBeUndefined();
    expect(post).toBeUndefined();
    expect(sw.fetch).not.toHaveBeenCalled();
  });

  it("passes a successful navigation through untouched and caches nothing new", async () => {
    const network = { status: 302, from: "network" };
    const sw = loadWorker(() => Promise.resolve(network));
    await sw.dispatch("install");
    const before = sw.caches.map.get(CACHE_NAME)?.store.size;
    const response = await sw.navigate({
      method: "GET",
      mode: "navigate",
      url: "/oauth/authorize",
    });
    // The exact network object — an auth redirect is neither rewritten nor
    // stored; the cache still holds only the precached offline screen.
    expect(await response).toBe(network);
    expect(sw.caches.map.size).toBe(1);
    expect(sw.caches.map.get(CACHE_NAME)?.store.size).toBe(before);
  });

  it("serves the precached offline screen when the network is unreachable", async () => {
    const sw = loadWorker(() => Promise.reject(new TypeError("offline")));
    await sw.dispatch("install");
    const response = (await sw.navigate({
      method: "GET",
      mode: "navigate",
      url: "/mail",
    })) as Promise<CachedEntry>;
    expect((await response).url).toBe("/offline.html");
  });

  it("answers 503 rather than hanging if the precache is gone too", async () => {
    const sw = loadWorker(() => Promise.reject(new TypeError("offline")));
    // No install: the cache is empty, as after a storage eviction.
    const response = (await sw.navigate({
      method: "GET",
      mode: "navigate",
      url: "/mail",
    })) as Promise<Response>;
    expect((await response).status).toBe(503);
  });
});

describe("the version-bump path end to end", () => {
  it("a bumped worker replaces the old cache with its own on the same scope", async () => {
    // Deploy 1: the current source, rewritten to an older version.
    const oldSource = SW_SOURCE.replace(
      `const VERSION = "${VERSION}"`,
      'const VERSION = "test-old"',
    );
    const caches = new FakeCaches();
    const listeners: Record<string, Listener> = {};
    const self = {
      addEventListener: (type: string, fn: Listener) => {
        listeners[type] = fn;
      },
      skipWaiting: vi.fn(async () => {}),
      clients: { claim: vi.fn(async () => {}) },
    };
    new Function("self", "caches", "fetch", "Request", "Response", oldSource)(
      self,
      caches,
      vi.fn(),
      FakeRequest,
      Response,
    );
    const run = async (type: "install" | "activate") => {
      const waits: Promise<unknown>[] = [];
      listeners[type]?.({ waitUntil: (p) => waits.push(p) });
      expect(waits.length).toBeGreaterThan(0);
      await Promise.all(waits);
    };
    await run("install");
    await run("activate");
    expect(await caches.keys()).toEqual(["alo-offline-test-old"]);

    // Deploy 2: the shipped source arrives; browsers reuse the same
    // CacheStorage, the new worker installs beside the old cache and its
    // activate leaves only its own version behind.
    new Function("self", "caches", "fetch", "Request", "Response", SW_SOURCE)(
      self,
      caches,
      vi.fn(),
      FakeRequest,
      Response,
    );
    await run("install");
    await run("activate");
    expect(await caches.keys()).toEqual([CACHE_NAME]);
    expect(await caches.match("/offline.html")).toBeTruthy();
  });
});

describe("sw.js push notifications", () => {
  interface PushEvent {
    waitUntil: (p: Promise<unknown>) => void;
    data: { json: () => unknown } | null | undefined;
  }
  interface ClickEvent {
    waitUntil: (p: Promise<unknown>) => void;
    notification: { close: () => void; data?: { url?: string } };
  }

  /** Evaluate the real sw.js with the push-side seams mocked. */
  function loadPushWorker(options?: {
    windows?: { focused: boolean; focus?: () => void }[];
    language?: string;
  }) {
    const listeners: Record<string, (event: never) => void> = {};
    const showNotification = vi.fn(async () => {});
    const openWindow = vi.fn(async () => {});
    const windows = options?.windows ?? [];
    const self = {
      addEventListener: (type: string, fn: (event: never) => void) => {
        listeners[type] = fn;
      },
      skipWaiting: vi.fn(async () => {}),
      registration: { showNotification },
      navigator: { language: options?.language ?? "en" },
      clients: {
        claim: vi.fn(async () => {}),
        matchAll: vi.fn(async () => windows),
        openWindow,
      },
    };
    new Function("self", "caches", "fetch", "Request", "Response", SW_SOURCE)(
      self,
      new FakeCaches(),
      vi.fn(),
      FakeRequest,
      Response,
    );
    const push = async (data: PushEvent["data"]) => {
      const waits: Promise<unknown>[] = [];
      (listeners.push as (event: PushEvent) => void)({
        waitUntil: (p) => waits.push(p),
        data,
      });
      await Promise.all(waits);
    };
    const click = async (notification: ClickEvent["notification"]) => {
      const waits: Promise<unknown>[] = [];
      (listeners.notificationclick as (event: ClickEvent) => void)({
        waitUntil: (p) => waits.push(p),
        notification,
      });
      await Promise.all(waits);
    };
    return { showNotification, openWindow, windows, push, click };
  }

  const stateChange = {
    "@type": "StateChange",
    changed: { "user-1": { Email: "state-9", Mailbox: "state-9" } },
  };

  it("a mail state change shows the generic mail notification — never payload detail", async () => {
    const sw = loadPushWorker();
    await sw.push({ json: () => stateChange });
    expect(sw.showNotification).toHaveBeenCalledTimes(1);
    const [title, opts] = sw.showNotification.mock.calls[0] as unknown as [
      string,
      { body: string; tag: string },
    ];
    expect(title).toBe("New mail");
    // Nothing from the payload — no account id, no state string — reaches
    // the notification the OS shows.
    expect(`${title} ${opts.body}`).not.toContain("user-1");
    expect(`${title} ${opts.body}`).not.toContain("state-9");
    expect(opts.tag).toBe("alo-state");
  });

  it("speaks the device's language", async () => {
    const sw = loadPushWorker({ language: "de-DE" });
    await sw.push({ json: () => stateChange });
    expect((sw.showNotification.mock.calls[0] as unknown[])[0]).toBe(
      "Neue E-Mail",
    );
  });

  it("stays quiet while an alo window is focused", async () => {
    const sw = loadPushWorker({ windows: [{ focused: true }] });
    await sw.push({ json: () => stateChange });
    expect(sw.showNotification).not.toHaveBeenCalled();
  });

  it("an unreadable payload still nudges rather than failing silently", async () => {
    const sw = loadPushWorker();
    await sw.push({
      json: () => {
        throw new Error("not json");
      },
    });
    expect(sw.showNotification).toHaveBeenCalledTimes(1);
  });

  it("a click focuses an existing window, or opens the app when none is open", async () => {
    const focus = vi.fn();
    const focused = loadPushWorker({ windows: [{ focused: false, focus }] });
    await focused.click({ close: () => {}, data: { url: "/" } });
    expect(focus).toHaveBeenCalled();
    expect(focused.openWindow).not.toHaveBeenCalled();

    const empty = loadPushWorker();
    const close = vi.fn();
    await empty.click({ close, data: { url: "/" } });
    expect(close).toHaveBeenCalled();
    expect(empty.openWindow).toHaveBeenCalledWith("/");
  });
});

describe("offline.html", () => {
  it("is fully self-contained — no external stylesheet, script, font or image", () => {
    expect(OFFLINE_HTML).not.toMatch(/<link/i);
    expect(OFFLINE_HTML).not.toMatch(/<script[^>]*src=/i);
    expect(OFFLINE_HTML).not.toMatch(/<img/i);
    expect(OFFLINE_HTML).not.toMatch(/url\(/i);
  });

  it("carries all four catalog languages", () => {
    for (const lang of ["en", "fr", "nl", "de"]) {
      expect(OFFLINE_HTML).toMatch(new RegExp(`${lang}: \\{`));
    }
    expect(OFFLINE_HTML).toContain("hors ligne");
    expect(OFFLINE_HTML).toContain("offline");
  });
});

describe("registerOfflineShell", () => {
  function fakeWindow(withServiceWorker = true) {
    const listeners: Record<string, () => void> = {};
    const register = vi.fn(() => Promise.resolve({}));
    const unregister = vi.fn(() => Promise.resolve(true));
    const getRegistrations = vi.fn(() => Promise.resolve([{ unregister }]));
    const cacheKeys = vi.fn(() =>
      Promise.resolve(["alo-offline-v1", "another-application-cache"]),
    );
    const deleteCache = vi.fn(() => Promise.resolve(true));
    const win = {
      addEventListener: (type: string, fn: () => void) => {
        listeners[type] = fn;
      },
      navigator: withServiceWorker
        ? { serviceWorker: { register, getRegistrations } }
        : {},
      caches: { keys: cacheKeys, delete: deleteCache },
    } as unknown as Window;
    return {
      win,
      listeners,
      register,
      unregister,
      getRegistrations,
      deleteCache,
    };
  }

  it("registers /sw.js after the page load in production", () => {
    const { win, listeners, register } = fakeWindow();
    registerOfflineShell(win, true);
    expect(register).not.toHaveBeenCalled();
    listeners.load?.();
    expect(register).toHaveBeenCalledWith("/sw.js");
  });

  it("removes stale Alo offline state in dev builds", async () => {
    const {
      win,
      listeners,
      register,
      unregister,
      getRegistrations,
      deleteCache,
    } = fakeWindow();
    registerOfflineShell(win, false);
    await vi.waitFor(() => expect(unregister).toHaveBeenCalledOnce());
    expect(listeners.load).toBeUndefined();
    expect(register).not.toHaveBeenCalled();
    expect(getRegistrations).toHaveBeenCalledOnce();
    expect(deleteCache).toHaveBeenCalledWith("alo-offline-v1");
    expect(deleteCache).not.toHaveBeenCalledWith("another-application-cache");
  });

  it("does nothing where service workers are unsupported", () => {
    const { win, listeners } = fakeWindow(false);
    registerOfflineShell(win, true);
    expect(listeners.load).toBeUndefined();
  });
});
