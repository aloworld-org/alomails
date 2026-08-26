// The shell must be a positioned box, and jsdom cannot prove why.
//
// An absolutely-positioned element is clipped by an `overflow: hidden`
// ancestor only when that ancestor is in its containing-block chain. A
// `sr-only` helper (every ds/Table renders one as its caption) is absolutely
// positioned; without a positioned ancestor inside the shell it anchors to the
// body, escapes the shell's clip, lands at the y-coordinate it would have had
// in flow — which for a table deep in a scroll pane is far past the viewport —
// and stretches the document. The visible symptom is a page scrollbar and a
// dead band of background below the app.
//
// jsdom computes no layout, so this cannot be asserted on geometry here. The
// geometric proof was run in a real engine (Chromium, 2026-08-26): the
// minimal reproduction scrolled to 1203px unpositioned and clipped at the
// viewport once `.shell` was positioned. What CAN be pinned is the
// declaration this all hangs on, so nobody deletes an apparently pointless
// `position: relative` from a rule that never uses offsets.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, test } from "vitest";

describe("the document never scrolls", () => {
  test("#root is a positioned, clipping box — the app-wide backstop", () => {
    // The invariant behind every screen: whatever a future layout forgets, an
    // absolutely-positioned escapee anchors to #root and is clipped by #root,
    // so the document can never grow a scrollbar over dead background. Proven
    // geometrically in Chromium (2026-08-26): a layout WITHOUT its own
    // position:relative leaked the document to 1203px against a 600px
    // viewport; under this #root rule the same layout clipped at 600px. The
    // duty it creates — screens taller than the viewport must scroll
    // themselves — is carried by the standalone pages' own roots.
    const css = readFileSync(
      join(dirname(fileURLToPath(import.meta.url)), "..", "ds", "global.css"),
      "utf8",
    );
    const rootRules = [...css.matchAll(/#root\s*\{[^}]*\}/gs)].map((m) => m[0]);
    expect(rootRules.some((r) => r.includes("position: relative"))).toBe(true);
    expect(rootRules.some((r) => r.includes("overflow: clip"))).toBe(true);
    // And printing must lift the clip, or an alo Doc prints one page.
    expect(css).toMatch(/@media print\s*\{\s*#root\s*\{\s*overflow: visible/s);
  });
});

describe("the shell owns its absolutely-positioned descendants", () => {
  test("the Tailwind shell is a containing block, so its overflow clip actually applies", () => {
    const component = readFileSync(
      join(dirname(fileURLToPath(import.meta.url)), "AppShell.tsx"),
      "utf8",
    );
    const shellClasses =
      component.match(/<div className="([^"]*grid-template-areas:[^"]*)"/)?.[1] ?? "";
    expect(shellClasses).toContain("overflow-hidden");
    expect(
      shellClasses,
      "without position: relative, sr-only helpers anchor to the body, " +
        "escape this rule's overflow clip, and grow the document a scrollbar",
    ).toContain("relative");
  });
});
