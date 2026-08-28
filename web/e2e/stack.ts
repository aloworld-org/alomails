// The e2e stack: one throwaway database, one backend, one web server.
//
// The responsive suite drives the real app in a real browser, so it needs a
// real stack — but the developer stack (5173 + 8080 + the `alo` database) may
// belong to somebody else at any moment, and a test that writes into the
// product database is the bug CLAUDE.md's one-database rule exists to prevent.
// So the suite brings its own: a database named `alo_e2e` created before the
// run and dropped after it, the debug `alo-jmap` on its own port, and Vite on
// its own port with its OAuth client registered in the throwaway database
// only. Nothing here can collide with — or survive into — anybody's real
// stack.
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { mkdirSync, openSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const E2E_DIR = path.dirname(fileURLToPath(import.meta.url));
export const WEB_DIR = path.resolve(E2E_DIR, "..");
export const REPO_DIR = path.resolve(WEB_DIR, "..");
/** Logs, per-failure screenshots, blobs and pid bookkeeping — gitignored. */
export const ARTIFACTS_DIR = path.join(E2E_DIR, ".artifacts");

// Ports far from the developer stack (5173/8080), so a running dev session
// and this suite never contend.
export const WEB_PORT = 5199;
export const API_PORT = 8199;
export const WEB_ORIGIN = `http://localhost:${WEB_PORT}`;
export const API_ORIGIN = `http://127.0.0.1:${API_PORT}`;

/** The throwaway database. Never a name a product runs on (`alo`, `ficina`) —
 *  guarded in code because the guard in a doc is not a guard. */
export const E2E_DB = "alo_e2e";
export const DATABASE_URL = `postgres://alo:alo-dev-only@localhost:5432/${E2E_DB}`;

export const ADMIN_EMAIL = "admin@e2e.test";
// A local-only credential for a database that exists for minutes; not a secret.
export const ADMIN_PASSWORD = "e2e-responsive-suite";

const exe = process.platform === "win32" ? ".exe" : "";
export const JMAP_BIN = path.join(REPO_DIR, "target", "debug", `alo-jmap${exe}`);
export const IDENTITYCTL_BIN = path.join(REPO_DIR, "target", "debug", `identityctl${exe}`);

const PIDS_FILE = path.join(ARTIFACTS_DIR, "stack-pids.json");

/** Environment both backend processes run with. */
export function backendEnv(): NodeJS.ProcessEnv {
  return {
    ...process.env,
    DATABASE_URL,
    ALO_BLOB_DIR: path.join(ARTIFACTS_DIR, "blobs"),
    ALO_IDENTITY_ISSUER: WEB_ORIGIN,
    ALO_JMAP_ADDR: `127.0.0.1:${API_PORT}`,
    ALO_ADMIN_PASSWORD: ADMIN_PASSWORD,
  };
}

/** Run one SQL statement against the dockerised postgres, outside any
 *  database this suite owns (server-level commands connect to `postgres`). */
export function psql(sql: string): string {
  const result = spawnSync(
    "docker",
    ["exec", "alo-pg", "psql", "-U", "alo", "-d", "postgres", "-v", "ON_ERROR_STOP=1", "-tAc", sql],
    { encoding: "utf8" },
  );
  if (result.error) throw new Error(`psql could not run: ${result.error.message}`);
  if (result.status !== 0) {
    throw new Error(`psql failed (${sql}): ${result.stderr}`);
  }
  return result.stdout;
}

/** Run one of the alo binaries to completion, logging to the artifacts dir. */
export function runTool(bin: string, args: string[], logName: string): void {
  const result = spawnSync(bin, args, {
    encoding: "utf8",
    env: backendEnv(),
    cwd: REPO_DIR,
    // First run migrates the whole schema; give it room.
    timeout: 300_000,
  });
  writeFileSync(
    path.join(ARTIFACTS_DIR, logName),
    `${result.stdout ?? ""}\n${result.stderr ?? ""}`,
  );
  if (result.error) throw new Error(`${path.basename(bin)} could not run: ${result.error.message}`);
  if (result.status !== 0) {
    throw new Error(
      `${path.basename(bin)} ${args[0]} failed (exit ${result.status}) — see e2e/.artifacts/${logName}`,
    );
  }
}

/** Spawn a long-lived stack process with its output tee'd to a log file. */
export function spawnLogged(
  bin: string,
  args: string[],
  logName: string,
  env: NodeJS.ProcessEnv,
): ChildProcess {
  mkdirSync(ARTIFACTS_DIR, { recursive: true });
  const log = openSync(path.join(ARTIFACTS_DIR, logName), "w");
  const child = spawn(bin, args, {
    env,
    cwd: WEB_DIR,
    stdio: ["ignore", log, log],
  });
  return child;
}

/** Poll a URL until it answers at all (any HTTP status — a 401 is "up"). */
export async function waitForHttp(url: string, timeoutMs: number, what: string): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  let lastError = "";
  while (Date.now() < deadline) {
    try {
      await fetch(url, { signal: AbortSignal.timeout(2_000) });
      return;
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
      await new Promise((r) => setTimeout(r, 250));
    }
  }
  throw new Error(`${what} did not answer at ${url} within ${timeoutMs}ms (${lastError})`);
}

/** Remember the stack's pids so a crashed run can be cleaned up by the next. */
export function writePids(pids: { jmap: number; vite: number }): void {
  writeFileSync(PIDS_FILE, JSON.stringify(pids));
}

/** Kill whatever a previous run left behind; quiet when there is nothing. */
export function killRecordedPids(): void {
  let recorded: { jmap?: number; vite?: number };
  try {
    recorded = JSON.parse(readFileSync(PIDS_FILE, "utf8")) as { jmap?: number; vite?: number };
  } catch {
    return;
  }
  for (const pid of [recorded.jmap, recorded.vite]) {
    if (typeof pid !== "number") continue;
    try {
      process.kill(pid);
    } catch {
      // Already gone — the normal case.
    }
  }
}
