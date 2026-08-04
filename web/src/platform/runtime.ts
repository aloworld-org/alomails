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

/** `fetch` for API requests: the native Tauri HTTP client in the desktop app
 *  (bypasses the webview's origin/CORS rules), the platform `fetch` in a
 *  browser. Signature-compatible with WHATWG `fetch`. */
export const apiFetch: typeof fetch = inTauri
  ? (tauriFetch as typeof fetch)
  : globalThis.fetch.bind(globalThis);
