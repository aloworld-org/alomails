// Global setup: provision the throwaway stack the responsive suite runs on.
//
// Order matters: the database must exist before identityctl migrates it and
// seeds the tenant, the tenant and OAuth client must exist before the backend
// answers a login, and the backend must be up before Vite proxies to it. All
// of it is torn down again in stack.teardown.ts — including the database,
// because a kept scratch database is how a machine ends up with twenty.
import { existsSync, mkdirSync, rmSync } from "node:fs";
import path from "node:path";

import {
  ADMIN_EMAIL,
  API_ORIGIN,
  ARTIFACTS_DIR,
  E2E_DB,
  IDENTITYCTL_BIN,
  JMAP_BIN,
  WEB_DIR,
  WEB_ORIGIN,
  WEB_PORT,
  backendEnv,
  killRecordedPids,
  psql,
  runTool,
  spawnLogged,
  waitForHttp,
  writePids,
} from "./stack";

export default async function globalSetup(): Promise<void> {
  // The one-database rule, enforced where it can actually bite: this file is
  // the only place the suite names its database, and it refuses product names.
  if (["alo", "ficina"].includes(E2E_DB)) {
    throw new Error(`refusing to run the e2e suite against the product database "${E2E_DB}"`);
  }
  for (const bin of [JMAP_BIN, IDENTITYCTL_BIN]) {
    if (!existsSync(bin)) {
      throw new Error(
        `${bin} is missing — build it first: cargo build -p alo-jmap -p alo-identity --bins`,
      );
    }
  }

  // A previous run that died mid-flight leaves servers squatting the ports;
  // kill what it recorded before claiming them again.
  killRecordedPids();

  rmSync(ARTIFACTS_DIR, { recursive: true, force: true });
  mkdirSync(path.join(ARTIFACTS_DIR, "blobs"), { recursive: true });

  // Fresh database; FORCE disconnects any stale client of a crashed run.
  psql(`DROP DATABASE IF EXISTS ${E2E_DB} WITH (FORCE)`);
  psql(`CREATE DATABASE ${E2E_DB}`);

  // identityctl migrates the schema on connect, then seeds the tenant, its
  // admin, and the web app's OAuth client. The redirect URI is this suite's
  // origin, registered in this suite's database only — the developer stack's
  // 5173 registration lives in a different database and is untouched.
  runTool(IDENTITYCTL_BIN, ["bootstrap-admin", "e2e", ADMIN_EMAIL], "identityctl.log");
  runTool(
    IDENTITYCTL_BIN,
    ["register-client", "web", "alo web e2e", `${WEB_ORIGIN}/auth/callback`],
    "register-client.log",
  );

  const jmap = spawnLogged(JMAP_BIN, [], "alo-jmap.log", backendEnv());
  await waitForHttp(`${API_ORIGIN}/.well-known/jmap`, 60_000, "alo-jmap");

  // Vite serves the real app; the proxy in vite.config.ts forwards the API
  // prefixes to the suite's backend. Spawned via node directly (no npm/cmd
  // wrapper) so the recorded pid is the server itself.
  const vite = spawnLogged(
    process.execPath,
    [
      path.join(WEB_DIR, "node_modules", "vite", "bin", "vite.js"),
      "--port",
      String(WEB_PORT),
      "--strictPort",
    ],
    "vite.log",
    { ...process.env, VITE_DEV_API: API_ORIGIN },
  );
  await waitForHttp(WEB_ORIGIN, 120_000, "vite");

  if (typeof jmap.pid !== "number" || typeof vite.pid !== "number") {
    throw new Error("a stack process failed to spawn (no pid)");
  }
  writePids({ jmap: jmap.pid, vite: vite.pid });
}
