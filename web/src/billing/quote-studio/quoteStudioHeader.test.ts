import { describe, expect, it } from "vitest";

import {
  createContactVCard,
  formatQuoteDocumentDate,
  quotationHeaderRatioClass,
} from "./quoteStudioHeader";

describe("quote studio header presentation", () => {
  it("reverses unequal ratios when identity is on the right", () => {
    expect(quotationHeaderRatioClass("40-60", "right")).toContain("3fr");
  });

  it("formats document dates for the active locale", () => {
    expect(formatQuoteDocumentDate("2026-08-25", "de-DE")).toContain("2026");
  });

  it("creates a contact vCard", () => {
    expect(
      createContactVCard({
        companyName: "Alo",
        address: "Berlin",
        email: "hello@example.com",
        phone: "",
        website: "",
        vatId: "",
        registrationNo: "",
      }),
    ).toContain("EMAIL;TYPE=WORK:hello@example.com");
  });
});
