import { describe, expect, it } from "vitest";

import {
  hasQuotePreviewText,
  quoteBlockHasPreviewContent,
} from "./quoteStudioPreview";

describe("quote studio preview visibility", () => {
  it("ignores empty rich text markup", () => {
    expect(hasQuotePreviewText("<p>&nbsp;</p>")).toBe(false);
  });

  it("keeps dividers visible", () => {
    expect(
      quoteBlockHasPreviewContent({ id: "divider", kind: "divider" }),
    ).toBe(true);
  });
});
