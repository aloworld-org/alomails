import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { RelatedBillingDocuments } from "./RelatedBillingDocuments";

const billingDocuments = vi.fn();

vi.mock("./api", async () => {
  const actual = await vi.importActual<typeof import("./api")>("./api");
  return {
    ...actual,
    useCrmApi: () => ({ billingDocuments }),
  };
});

describe("RelatedBillingDocuments", () => {
  beforeEach(() => billingDocuments.mockReset());

  test("shows only the server-linked quote and invoice with direct destinations", async () => {
    billingDocuments.mockResolvedValue([
      {
        kind: "quote",
        documentId: "quote-1",
        status: "accepted",
        number: "QUO-2026-0042",
        createdAt: "2026-09-01T10:00:00Z",
      },
      {
        kind: "invoice",
        documentId: "invoice-1",
        status: "draft",
        number: null,
        createdAt: "2026-09-01T09:00:00Z",
      },
    ]);
    render(
      <MemoryRouter>
        <RelatedBillingDocuments dealId="deal-1" revision={0} />
      </MemoryRouter>,
    );

    expect(await screen.findByText("QUO-2026-0042")).toBeTruthy();
    expect(screen.getByText("Draft invoice")).toBeTruthy();
    expect(screen.getByRole("link", { name: /QUO-2026-0042/ }).getAttribute("href")).toBe(
      "/billing/quotes/quote-1",
    );
    expect(screen.getByRole("link", { name: /Draft invoice/ }).getAttribute("href")).toBe(
      "/billing/invoices/invoice-1",
    );
  });
});
