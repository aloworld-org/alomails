import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, test } from "vitest";

const sourceRoot = join(process.cwd(), "src");
const checkedRoots = [join(sourceRoot, "billing"), join(sourceRoot, "ds")];
const userFacingLiteral =
  />\s*[A-Z][a-z]+ [^<>{}]{3,}<|(placeholder|title|aria-label|label|alt)="[A-Z][^"]{3,}"/g;

function sourceFiles(path: string): string[] {
  return readdirSync(path).flatMap((entry) => {
    const candidate = join(path, entry);
    if (statSync(candidate).isDirectory()) return sourceFiles(candidate);
    if (!entry.endsWith(".tsx") || entry.includes(".test.")) return [];
    return [candidate];
  });
}

describe("user-facing copy", () => {
  test("billing and design-system components use the locale catalogs", () => {
    const violations = checkedRoots.flatMap((root) =>
      sourceFiles(root).flatMap((file) => {
        const lines = readFileSync(file, "utf8").split(/\r?\n/);
        return lines.flatMap((line, index) => {
          userFacingLiteral.lastIndex = 0;
          if (!userFacingLiteral.test(line)) return [];
          return [`${relative(sourceRoot, file)}:${index + 1}: ${line.trim()}`];
        });
      }),
    );

    expect(
      violations,
      "Move each user-facing literal to strings.* and translate it in en, fr, nl and de.",
    ).toEqual([]);
  });
});
