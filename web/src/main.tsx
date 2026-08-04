// Application entry point: load the self-hosted fonts and design-system base
// styles (once), then mount the router. Fonts are bundled — no external CDN
// (sovereignty: the app makes no third-party requests to render).
import "@fontsource-variable/inter";
import "@fontsource-variable/eb-garamond";
import "./ds/tokens.css";
import "./ds/global.css";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { inTauri } from "./platform/runtime";
import { checkForUpdatesInBackground } from "./platform/updater";

const container = document.getElementById("root");
if (container === null) {
  throw new Error("index.html must provide a #root element");
}

// In the desktop shell, make the interface feel native: chrome text (headings,
// labels, buttons) is not selectable like a web page — only real content
// (message bodies, input fields) is. Scoped to `.desktop`, so the browser app
// keeps normal text selection. See ds/global.css.
if (inTauri) {
  document.documentElement.classList.add("desktop");
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

// Desktop app only: keep itself current. No-op in the browser.
void checkForUpdatesInBackground();
