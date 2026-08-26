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

describe("the shell owns its absolutely-positioned descendants", () => {
  test("`.shell` is a containing block, so its overflow clip actually applies", () => {
    const css = readFileSync(
      join(dirname(fileURLToPath(import.meta.url)), "AppShell.module.css"),
      "utf8",
    );
    const shellRule = css.match(/\.shell\s*\{[^}]*\}/s)?.[0] ?? "";
    expect(shellRule).toContain("overflow: hidden");
    expect(
      shellRule,
      "without position: relative, sr-only helpers anchor to the body, " +
        "escape this rule's overflow clip, and grow the document a scrollbar",
    ).toContain("position: relative");
  });
});
