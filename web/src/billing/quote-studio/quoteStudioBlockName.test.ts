import { describe, expect, it } from "vitest";
import { quoteStudioBlockName } from "./quoteStudioBlockName";

describe("quoteStudioBlockName", () => {
  it("returns the localized label for a block", () => {
    expect(quoteStudioBlockName({ id: "one", kind: "divider", style: "solid", thickness: "fine", width: 100, color: "#E76F51" })).toBeTruthy();
  });
});
