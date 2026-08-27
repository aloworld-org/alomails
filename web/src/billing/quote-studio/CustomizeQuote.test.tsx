import { describe, expect, it } from "vitest";

import { CustomizeQuote } from "./CustomizeQuote";

describe("CustomizeQuote", () => {
  it("exports the quotation customization dialog", () => {
    expect(CustomizeQuote).toBeTypeOf("function");
  });
});
