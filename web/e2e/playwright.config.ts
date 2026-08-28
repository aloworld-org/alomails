// Playwright config for the responsive layout suite (docs/autonomy/responsive,
// R3). Browsers come from the machine's existing Playwright cache — the
// version below is pinned to the cached Chromium so `npm run test:responsive`
// never downloads anything.
import { defineConfig } from "@playwright/test";

import { WEB_ORIGIN } from "./stack";

export default defineConfig({
  testDir: ".",
  outputDir: ".artifacts/test-results",
  globalSetup: "./stack.setup.ts",
  globalTeardown: "./stack.teardown.ts",
  // One worker: the tests share one dev server, and each test walks every
  // module at one viewport, logging in once — parallel workers would multiply
  // logins for no wall-clock win on a transform-on-demand server.
  workers: 1,
  fullyParallel: false,
  // A test visits all modules at one width; the first pass also pays the dev
  // server's on-demand transform of every module chunk.
  timeout: 300_000,
  expect: { timeout: 10_000 },
  reporter: [["list"]],
  use: {
    baseURL: WEB_ORIGIN,
    screenshot: "only-on-failure",
  },
});
