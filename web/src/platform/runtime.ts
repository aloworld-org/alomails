// Where the app is running, and how it reaches the alo API.
//
// The same web bundle runs two ways:
//   - in a browser, served same-origin by the alo server — API calls are
//     relative to `window.location.origin`, and the platform `fetch` works;
//   - inside the Tauri desktop shell, where the UI is bundled locally (its
//     origin is `tauri://localhost`, not the API host). There, calls target an
//     absolute API base and go through Tauri's native HTTP client, which issues
//     the request from the Rust side — no browser cross-origin/CORS limits, and
//     redirects + response URLs behave like a real HTTP client (the OAuth login
//     reads the authorization code from the final redirect URL).
//
// Everything API-facing imports `API_BASE` and `apiFetch` from here rather than
// touching `window.location.origin` or the global `fetch` directly.
import { fetch as tauriFetch } from "@tauri-apps/plugin-http";

/** True when running inside the Tauri desktop shell (v2 exposes this global). */
export const inTauri: boolean =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Absolute origin the API is served from. In the browser it is the page's own
 *  origin (same-origin, unchanged behaviour). In the desktop app it is the
 *  hosted alo server, overridable at build time with `VITE_API_BASE`. */
export const API_BASE: string = inTauri
  ? (import.meta.env.VITE_API_BASE ?? "https://mail.alomails.com")
  : window.location.origin;

/** The operator control plane is a separate production service. Local Vite
 * only probes it when a developer explicitly configured that service. */
export const CONTROL_AVAILABLE: boolean =
  !import.meta.env.DEV || Boolean(import.meta.env.VITE_DEV_CONTROL_API?.trim());

/** `fetch` for API requests: the native Tauri HTTP client in the desktop app
 *  (bypasses the webview's origin/CORS rules), the platform `fetch` in a
 *  browser. Signature-compatible with WHATWG `fetch`.
 *
 *  In the browser a root-relative URL (`/.well-known/jmap`) resolves against the
 *  page origin — which IS the API host. In the desktop app the page origin is
 *  `tauri://localhost`, and the native HTTP client requires an absolute URL, so
 *  a root-relative request must be re-based onto `API_BASE` (the hosted server)
 *  first — otherwise the session/push calls that use relative paths silently
 *  fail and the app never leaves a blank screen. */
export const apiFetch: typeof fetch = inTauri
  ? ((input: RequestInfo | URL, init?: RequestInit) => {
      const rebased =
        typeof input === "string" && input.startsWith("/") ? `${API_BASE}${input}` : input;
      return (tauriFetch as typeof fetch)(rebased, init);
    })
  : globalThis.fetch.bind(globalThis);

// Build-time injected host for the Office (Collabora) editor — see vite.config.
declare const __ALO_OFFICE_HOST__: string;

/** Absolute host the Collabora editor loads and saves through. Same-origin (the
 *  page origin) in a real deployment; in local dev it is the real backend host
 *  (injected at build time), because Collabora runs there and must fetch the
 *  WOPI file from a host it can reach — never the developer's localhost. */
export const OFFICE_HOST: string =
  __ALO_OFFICE_HOST__.length > 0
    ? __ALO_OFFICE_HOST__
    : typeof window !== "undefined"
      ? window.location.origin
      : "";
