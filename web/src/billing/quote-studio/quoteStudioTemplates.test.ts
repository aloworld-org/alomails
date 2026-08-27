import { describe, expect, it } from "vitest";

import { createQuoteTemplateDesign } from "./quoteStudioTemplates";

describe("quote studio templates", () => {
  it.each(["blank", "services", "project", "retainer"] as const)(
    "creates a %s design with a pricing table",
    (preset) => {
      const design = createQuoteTemplateDesign(preset);
      expect(design.blocks.some((block) => block.kind === "pricing")).toBe(
        true,
      );
    },
  );
});
