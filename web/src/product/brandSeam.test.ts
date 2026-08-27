// The brand copy on the unauthenticated pages (login, signup, reset) is
// decided by the product surface (`surface.brand`, `surface.login`) so that a
// mail build says alomails and the suite says workspace — ADR 0019. A page
// that reaches for `strings.brandHeadline` directly wears the workspace's
// clothes on every product; the signup page shipped that way and was caught by
// eye, not by a test. This scan makes the seam structural: outside this folder
// nothing may name a brand-panel or login-form string.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

import { describe, expect, test } from "vitest";

const SRC = join(import.meta.dirname, "..");

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) return sourceFiles(full);
    return /\.tsx?$/.test(full) && !/\.(?:test|spec)\.tsx?$/.test(full) ? [full] : [];
  });
}

describe("brand copy goes through the product surface", () => {
  test("only the surfaces resolve brand and login strings", () => {
    const offenders = sourceFiles(SRC).flatMap((file) => {
      const path = relative(SRC, file).split("\\").join("/");
      // The surfaces themselves and the catalogs are where these keys live.
      if (path.startsWith("product/") || path.startsWith("i18n/")) return [];
      const source = readFileSync(file, "utf8");
      return /strings\.(?:brandHeadline|brandSubtitle|brandEuBadge|emailPlaceholder)/.test(source)
        ? [path]
        : [];
    });
    expect(
      offenders,
      "Use surface.brand / surface.login from @product — the build decides the product, not the page",
    ).toEqual([]);
  });
});
