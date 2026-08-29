import { describe, expect, it } from "vitest";

import {
  QuoteStudioWorkspace,
  quoteStudioWorkspaceOverflow,
} from "./QuoteStudioWorkspace";

describe("QuoteStudioWorkspace", () => {
  it("exposes the quotation studio workspace", () => {
    expect(QuoteStudioWorkspace).toBeTruthy();
  });

  it("lets editor overlays escape while keeping previews clipped", () => {
    expect(quoteStudioWorkspaceOverflow(false)).toBe("overflow-visible");
    expect(quoteStudioWorkspaceOverflow(true)).toBe("overflow-hidden");
  });
});
