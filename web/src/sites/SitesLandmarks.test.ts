// One page, one `main` (S2.16b).
//
// `shell/AppShell.tsx` renders the routed module inside `<main>`. Six sites
// screens — analytics, funnel, heatmap, catalogs, bookings, collections —
// opened their content region with a second `<main>` inside it. Nesting is
// invalid HTML and, more to the point, a screen reader offering "main" twice
// in its landmark list is offering a choice that means nothing: the reader
// lands somewhere arbitrary and has no way to tell which one holds the page.
//
// A test rather than a review note, for `ds/primitives.test.ts`'s reason: the
// mistake is invisible in the file it is made in — every one of those six
// screens reads perfectly well on its own — and only wrong in combination with
// a component nobody editing them has open.
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, test } from "vitest";

const HERE = import.meta.dirname;

/** Rendered outside `AppShell` (`App.tsx`: `/sites/invite/:token` is public,
 *  because the person holding the invitation may not have an account yet), so
 *  it is a whole page and owns its own landmarks. */
const OWNS_ITS_PAGE = ["SiteInvitationView.tsx"];

function sourceFiles(): string[] {
  return readdirSync(HERE).filter(
    (name) => name.endsWith(".tsx") && !name.endsWith(".test.tsx"),
  );
}

describe("the sites module does not claim the shell's landmarks", () => {
  test("the file list is the real module, not an empty filter", () => {
    // A glob that matched nothing would make the assertion below vacuous.
    expect(sourceFiles().length).toBeGreaterThan(30);
    expect(sourceFiles()).toContain("AnalyticsView.tsx");
  });

  test("only the public invitation page opens a <main>", () => {
    const offenders = sourceFiles().filter(
      (name) =>
        !OWNS_ITS_PAGE.includes(name) &&
        /<main[\s>]/.test(readFileSync(join(HERE, name), "utf8")),
    );
    expect(offenders).toEqual([]);
  });

  test("the page that does own one still has it", () => {
    const source = readFileSync(join(HERE, "SiteInvitationView.tsx"), "utf8");
    expect(/<main[\s>]/.test(source)).toBe(true);
  });
});
