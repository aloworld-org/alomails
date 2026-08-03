// `strings` — the single object every component reads user-facing text
// from (`strings.compose`, `strings.mailMoved(name)`, …). It is a proxy
// over the *active* locale's catalog, so switching language is
// transparent to all ~50 call sites: they keep `import { strings }` and
// get the current language at access time. A full re-render is triggered
// by the locale store (see `locale.ts` / the App root), and each read
// then resolves against the new catalog.
//
// Every key falls back to English (the active catalog is English overlaid
// with the locale's overrides), so a partial translation shows English,
// never a blank.
import { currentCatalog } from "./locale";
import type { Catalog, StringKey } from "./en";

export type { StringKey } from "./en";

export const strings: Catalog = new Proxy({} as Catalog, {
  get(_target, key: string | symbol): unknown {
    return (currentCatalog() as Record<string | symbol, unknown>)[key];
  },
  // Keep the object introspectable (Object.keys, `in`) against the
  // active catalog, so tooling and tests behave as if it were plain.
  has(_target, key): boolean {
    return key in currentCatalog();
  },
  ownKeys(): ArrayLike<string | symbol> {
    return Reflect.ownKeys(currentCatalog());
  },
  getOwnPropertyDescriptor(_target, key): PropertyDescriptor | undefined {
    return Object.getOwnPropertyDescriptor(currentCatalog(), key as StringKey);
  },
});
