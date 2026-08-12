// The ratchet that stops the design system being optional.
//
// A workspace looks like one product when a button is *the* button. Ours had
// drifted the other way: forty-six stylesheets defining their own `.input`,
// `.modal`, `.card` and `.field`, because CSS Modules scope a file's styles
// so completely that nobody can see the twenty-one other implementations of
// the thing they are about to write.
//
// Code review cannot hold that line — it asks a reviewer to remember every
// stylesheet in the repository. A build can. This is the same mechanism as
// `i18n/locale.test.ts`, which is the one convention here that has actually
// held: an explicit list of what is already wrong, a rule that nothing new may
// join it, and a list that may only shrink.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

import { describe, expect, test } from "vitest";

import { REDEFINES_PRIMITIVES } from "./redefined";

/** What the design system owns, and no module stylesheet may re-declare.
 *
 * Chosen from what the codebase actually kept re-deriving rather than from a
 * catalogue: every name here had at least eight independent implementations. */
const PRIMITIVES = [
  "button",
  "btn",
  "input",
  "field",
  "modal",
  "dialog",
  "card",
  "table",
  "badge",
  "chip",
  "toolbar",
  "select",
  "checkbox",
  "toggle",
];

const RULE = new RegExp(`^\\.(${PRIMITIVES.join("|")})s?\\b`, "m");

const SRC = join(import.meta.dirname, "..");

function stylesheets(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) return stylesheets(full);
    return full.endsWith(".module.css") ? [full] : [];
  });
}

/** Repo-relative, forward slashes, so the list reads the same on every OS. */
function id(path: string): string {
  return relative(SRC, path).split("\\").join("/");
}

describe("the design system owns the primitives", () => {
  const offenders = stylesheets(SRC)
    .map(id)
    // `ds/` is where a primitive is *supposed* to be declared, so it is not
    // scanned at all. Excluded here rather than listed as an exemption: an
    // exemption says "wrong, tolerated", and this is right.
    .filter((file) => !file.startsWith("ds/"))
    .filter((file) => RULE.test(readFileSync(join(SRC, file), "utf8")))
    .sort();
  const allowed = new Set(REDEFINES_PRIMITIVES);

  test("a new stylesheet may not hand-roll a primitive", () => {
    const fresh = offenders.filter((file) => !allowed.has(file));
    expect(
      fresh,
      `These stylesheets define a primitive that belongs to ds/.\n` +
        `Use the component, or argue for an exemption in ds/redefined.ts:\n  ` +
        fresh.join("\n  "),
    ).toEqual([]);
  });

  test("the list only shrinks — a migrated stylesheet must leave it", () => {
    const stale = REDEFINES_PRIMITIVES.filter(
      (file) => !offenders.includes(file),
    );
    expect(
      stale,
      `No longer define a primitive — delete these lines from ds/redefined.ts:\n  ` +
        stale.join("\n  "),
    ).toEqual([]);
  });

  test("the design system's own stylesheets are not scanned", () => {
    // If this ever fails, the folder has been renamed and the rule is now
    // policing the very place primitives belong — which would read as the
    // design system being illegal.
    expect(offenders.some((file) => file.startsWith("ds/"))).toBe(false);
    expect(REDEFINES_PRIMITIVES.some((f) => f.startsWith("ds/"))).toBe(false);
  });
});
