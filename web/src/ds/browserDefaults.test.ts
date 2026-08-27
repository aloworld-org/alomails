import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

import { describe, expect, test } from "vitest";

const SRC = join(import.meta.dirname, "..");

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) return sourceFiles(full);
    return full.endsWith(".tsx") && !/\.(?:test|spec)\.tsx$/.test(full) ? [full] : [];
  });
}

function withoutComments(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");
}

describe("browser defaults only shrink", () => {
  test("feature code does not add another native select popup", () => {
    const occurrences = sourceFiles(SRC)
      .filter((file) => relative(SRC, file).split("\\").join("/") !== "ds/Select.tsx")
      .reduce(
        (count, file) =>
          count + (withoutComments(readFileSync(file, "utf8")).match(/<select\b/g) ?? []).length,
        0,
      );

    // Legacy debt, not a target. Native option popups cannot guarantee Alo's
    // full-row hover/selection treatment; every touched occurrence migrates to
    // ChoicePicker and this ceiling only moves down.
    expect(occurrences, "Use ChoicePicker instead of adding a browser-owned menu").toBeLessThanOrEqual(59);
  });

  test("visible file inputs are not used as upload buttons", () => {
    const offenders = sourceFiles(SRC).flatMap((file) => {
      const source = withoutComments(readFileSync(file, "utf8"));
      return /type=["']file["']/.test(source) && !/(?:className=|\bhidden\b)/.test(source)
        ? [relative(SRC, file).split("\\").join("/")]
        : [];
    });
    expect(offenders, "Use the Alo upload trigger with a visually hidden file input").toEqual([]);
  });

  test("shared inputs remove browser search decoration with Tailwind", () => {
    const input = readFileSync(join(SRC, "ds", "Input.tsx"), "utf8");
    expect(input).toContain("[&[type='search']]:appearance-none");
    expect(input).toContain("::-webkit-search-cancel-button]:appearance-none");
  });

  test("the shared checkbox does not expose the native browser drawing", () => {
    const checkbox = readFileSync(join(SRC, "ds", "Checkbox.tsx"), "utf8");
    expect(checkbox).toContain('"peer sr-only"');
    expect(checkbox).toContain("peer-checked:bg-accent");
    expect(checkbox).not.toContain("accent-accent");
  });
});
