import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { strings } from "../../i18n";
import type { BillingSettings } from "../types";
import { QuoteStudioWorkspace } from "./QuoteStudioWorkspace";

const api = {
  quoteDesign: vi.fn(),
  saveQuoteDesign: vi.fn(),
};

vi.mock("../api", () => ({
  useBillingApi: () => api,
}));

function renderWorkspace(issuer?: BillingSettings) {
  return render(
    <QuoteStudioWorkspace
      quoteId="quote-1"
      readOnly={false}
      pricingTable={() => <div>Pricing rows</div>}
      tableSubtotal={() => null}
      lineKeys={[]}
      {...(issuer === undefined ? {} : { issuer })}
    />,
  );
}

describe("QuoteStudioWorkspace", () => {
  beforeEach(() => {
    api.quoteDesign.mockReset().mockResolvedValue({
      design: null,
      updatedAt: null,
    });
    api.saveQuoteDesign.mockReset().mockResolvedValue(undefined);
  });

  it("keeps editing blocks borderless until interaction", async () => {
    renderWorkspace();

    const tableName = await screen.findByRole("textbox", {
      name: strings.quoteStudioTableName,
    });
    const block = tableName.closest("article");
    const toolbar = tableName.closest("label")?.parentElement;

    expect(block).not.toBeNull();
    expect(block?.className).not.toMatch(/\bborder\b/);
    expect(block?.className).toContain("ring-transparent");
    expect(toolbar?.className).not.toContain("border-b");

    await waitFor(() => expect(api.quoteDesign).toHaveBeenCalledWith("quote-1"));
  });

  it("keeps the header edit action visual and clear of narrow company names", async () => {
    renderWorkspace({
      stated: true,
      legalName: "Alo Demo Works SRL",
      addressLine1: "",
      addressLine2: "",
      postalCode: "",
      city: "",
      country: "",
      vatId: null,
      registrationNo: "",
      email: "",
      phone: "",
      website: "",
      iban: null,
      bic: null,
      bankName: "",
      accountHolder: "",
      footerNote: "",
      baseCurrency: "EUR",
      updatedBy: null,
      updatedAt: null,
    });

    const action = await screen.findByRole("button", {
      name: strings.quoteStudioEditHeader,
    });
    const company = screen.getByText("Alo Demo Works SRL");

    expect(action.className).toContain("size-10");
    expect(action.querySelector(".sr-only")?.textContent).toBe(
      strings.quoteStudioEditHeader,
    );
    expect(company.closest("div.flex")?.className).toContain("max-sm:pr-16");
  });
});
