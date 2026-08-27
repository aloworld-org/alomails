import { describe, expect, it } from "vitest";

import { QuoteStudioWorkspace } from "./QuoteStudioWorkspace";

describe("QuoteStudioWorkspace", () => {
  it("exposes the quotation studio workspace", () => {
    expect(QuoteStudioWorkspace).toBeTruthy();
  });
});
