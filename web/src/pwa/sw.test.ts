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
    const win = {
      addEventListener: (type: string, fn: () => void) => {
        listeners[type] = fn;
      },
      navigator: withServiceWorker ? { serviceWorker: { register } } : {},
    } as unknown as Window;
    return { win, listeners, register };
  }

  it("registers /sw.js after the page load in production", () => {
    const { win, listeners, register } = fakeWindow();
    registerOfflineShell(win, true);
    expect(register).not.toHaveBeenCalled();
    listeners.load?.();
    expect(register).toHaveBeenCalledWith("/sw.js");
  });

  it("does nothing in dev builds", () => {
    const { win, listeners, register } = fakeWindow();
    registerOfflineShell(win, false);
    expect(listeners.load).toBeUndefined();
    expect(register).not.toHaveBeenCalled();
  });

  it("does nothing where service workers are unsupported", () => {
    const { win, listeners } = fakeWindow(false);
    registerOfflineShell(win, true);
    expect(listeners.load).toBeUndefined();
  });
});
