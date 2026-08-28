// Global teardown: stop the stack and drop the throwaway database.
//
// The artifacts directory (logs, failure screenshots) is deliberately kept —
// it is what a human reads after a red run — and is gitignored.
import { E2E_DB, killRecordedPids, psql } from "./stack";

export default async function globalTeardown(): Promise<void> {
  killRecordedPids();
  // Give Windows a moment to release the processes' database connections,
  // then drop with FORCE so a lingering connection cannot keep the scratch
  // database alive past the run.
  await new Promise((r) => setTimeout(r, 1_000));
  psql(`DROP DATABASE IF EXISTS ${E2E_DB} WITH (FORCE)`);
}
