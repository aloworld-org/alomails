import { describe, expect, it } from "vitest";

import {
  BULLET_STYLES,
  NUMBERING_STYLES,
  listMarker,
  resolveListStyle,
} from "./listStyles";

describe("listMarker", () => {
  it("reproduces the numbering library at every level", () => {
    // Counters are [top, second, third]; the marker for a level reads its
    // own counter, and the outline scheme reads its ancestors' too.
    expect(listMarker("decimal", 0, [3, 0, 0])).toBe("3.");
    expect(listMarker("decimal", 1, [3, 2, 0])).toBe("b.");
    expect(listMarker("decimal", 2, [3, 2, 4])).toBe("iv.");
    expect(listMarker("parenthesis", 0, [1, 0, 0])).toBe("1)");
    expect(listMarker("parenthesis", 1, [1, 1, 0])).toBe("a)");
    expect(listMarker("parenthesis", 2, [1, 1, 1])).toBe("i)");
    expect(listMarker("outline", 1, [1, 2, 0])).toBe("1.2.");
    expect(listMarker("outline", 2, [1, 2, 1])).toBe("1.2.1.");
    expect(listMarker("upper-alpha", 0, [2, 0, 0])).toBe("B.");
    expect(listMarker("roman", 0, [4, 0, 0])).toBe("IV.");
    expect(listMarker("roman", 1, [4, 1, 0])).toBe("A.");
    expect(listMarker("roman", 2, [4, 1, 7])).toBe("7.");
    expect(listMarker("leading-zero", 0, [7, 0, 0])).toBe("07.");
    expect(listMarker("leading-zero", 0, [12, 0, 0])).toBe("12.");
  });

  it("keeps counting past the alphabet", () => {
    expect(listMarker("decimal", 1, [1, 27, 0])).toBe("aa.");
    expect(listMarker("upper-alpha", 0, [52, 0, 0])).toBe("AZ.");
  });

  it("uses the bullet glyphs of each scheme per level", () => {
    expect(listMarker("disc", 0, [1])).toBe("●");
    expect(listMarker("disc", 2, [1, 1, 1])).toBe("■");
    expect(listMarker("diamond", 1, [1, 1])).toBe("➢");
    expect(listMarker("checkbox", 2, [1, 1, 1])).toBe("☐");
  });
});

describe("resolveListStyle", () => {
  it("defaults to what lists looked like before styles existed", () => {
    expect(resolveListStyle(undefined, true)).toBe("decimal");
    expect(resolveListStyle(undefined, false)).toBe("disc");
  });

  it("rejects unknown ids and schemes of the wrong kind", () => {
    expect(resolveListStyle("fancy", true)).toBe("decimal");
    expect(resolveListStyle("disc", true)).toBe("decimal");
    expect(resolveListStyle("roman", false)).toBe("disc");
    for (const style of NUMBERING_STYLES) expect(resolveListStyle(style, true)).toBe(style);
    for (const style of BULLET_STYLES) expect(resolveListStyle(style, false)).toBe(style);
  });
});
