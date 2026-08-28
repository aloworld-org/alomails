import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

const { billingApi } = vi.hoisted(() => ({
  billingApi: {
    products: async () => [
      { id: "product-support", name: "Implementation support", unit: "hour", unitPriceCents: 13500, vatRateBp: 2100, sku: "SUP-01", barcode: "", stocked: false, purchasePriceCents: 0, photoNodeId: null, defaultSupplierId: null, archived: false, archivedAt: null, createdBy: "test", createdAt: "2026-08-01", updatedAt: "2026-08-01" },
      { id: "product-hosting", name: "Managed hosting", unit: "month", unitPriceCents: 45000, vatRateBp: 2100, sku: "HOST-01", barcode: "", stocked: false, purchasePriceCents: 0, photoNodeId: null, defaultSupplierId: null, archived: false, archivedAt: null, createdBy: "test", createdAt: "2026-08-01", updatedAt: "2026-08-01" },
      { id: "product-workshop", name: "Product design workshop", unit: "workshop", unitPriceCents: 125000, vatRateBp: 2100, sku: "DES-01", barcode: "", stocked: false, purchasePriceCents: 0, photoNodeId: null, defaultSupplierId: null, archived: false, archivedAt: null, createdBy: "test", createdAt: "2026-08-01", updatedAt: "2026-08-01" },
    ],
  },
}));

vi.mock("./api", () => ({
  useBillingApi: () => billingApi,
}));

import { DialogProvider } from "../ds";
import { PriceConnectionsView } from "./PriceConnectionsView";

afterEach(cleanup);

function renderView() {
  return render(<DialogProvider><PriceConnectionsView /></DialogProvider>);
}

describe("price connections", () => {
  test("shows both directions without mixing supplier and client catalogues", () => {
    renderView();

    expect(screen.getByText("Nordwerk Components")).toBeTruthy();
    expect(screen.queryByText("Atlas Advisory GmbH")).toBeNull();

    fireEvent.click(screen.getByRole("tab", { name: /Shared by me/ }));
    expect(screen.getByText("Atlas Advisory GmbH")).toBeTruthy();
    expect(screen.queryByText("Nordwerk Components")).toBeNull();
  });

  test("connects an alo supplier only after its invitation has been previewed", () => {
    renderView();
    fireEvent.click(screen.getByRole("button", { name: "Connect supplier prices" }));

    const dialog = screen.getByRole("dialog", { name: "Connect supplier prices" });
    fireEvent.change(within(dialog).getByPlaceholderText("Supplier company name"), {
      target: { value: "Fjord Fasteners AS" },
    });
    const preview = within(dialog).getByRole("button", { name: "Test and preview" });
    expect(preview.hasAttribute("disabled")).toBe(true);

    fireEvent.change(within(dialog).getByPlaceholderText("Paste the alo invitation link"), {
      target: { value: "https://alo.example/connect/prices/FJORD" },
    });
    fireEvent.click(preview);
    expect(within(dialog).getByText("148 products found · 131 matched automatically · 17 can be reviewed after connecting.")).toBeTruthy();

    fireEvent.click(within(dialog).getByRole("button", { name: "Connect prices" }));
    expect(screen.getByText("Fjord Fasteners AS")).toBeTruthy();
    expect(screen.getByText(/is now supplying prices to this workspace/)).toBeTruthy();
  });

  test("creates a client share linked to the live price list", async () => {
    renderView();
    fireEvent.click(screen.getByRole("button", { name: "Share my prices" }));

    const dialog = screen.getByRole("dialog", { name: "Share my prices" });
    await waitFor(() => expect(within(dialog).getByText("Live price list (3)")).toBeTruthy());
    fireEvent.change(within(dialog).getByPlaceholderText("Company name"), {
      target: { value: "Orion Assembly GmbH" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Create secure connection" }));
    expect(within(dialog).getByDisplayValue("https://alo.example/connect/prices/AL7K-Q9M2")).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Done" }));

    expect(screen.getByRole("tab", { name: /Shared by me/ }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByText("Orion Assembly GmbH")).toBeTruthy();
    expect(screen.getByText(/is now receiving prices from this workspace/)).toBeTruthy();
  });

  test("can share an explicit selection from the price list", async () => {
    renderView();
    fireEvent.click(screen.getByRole("button", { name: "Share my prices" }));
    const dialog = screen.getByRole("dialog", { name: "Share my prices" });
    await waitFor(() => expect(within(dialog).getByText("Live price list (3)")).toBeTruthy());

    fireEvent.click(within(dialog).getByLabelText("Prices to share"));
    const selectedPricesOption = within(dialog).getByRole("option", { name: /Choose products/ });
    // The picker's own rule: the fill follows the pointer, the accent marks
    // the chosen value — so an option is never lit twice.
    fireEvent.mouseEnter(selectedPricesOption);
    expect(selectedPricesOption.className).toContain("!bg-raised");
    expect(selectedPricesOption.className).not.toContain("!text-accent");
    fireEvent.click(selectedPricesOption);
    fireEvent.click(within(dialog).getByRole("button", { name: /Managed hosting/ }));

    expect(within(dialog).getByText("1 selected products")).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: /Managed hosting/ }).getAttribute("aria-pressed")).toBe("true");
  });

  test("sync, pause and disconnect controls update a received connection", async () => {
    renderView();
    const card = screen.getByText("Nordwerk Components").closest("article");
    expect(card).not.toBeNull();
    const controls = within(card!);

    fireEvent.click(controls.getByRole("button", { name: "Sync now" }));
    expect(controls.getByText("Updated just now")).toBeTruthy();
    fireEvent.click(controls.getByRole("button", { name: "Pause" }));
    expect(controls.getByText("Paused")).toBeTruthy();
    fireEvent.click(controls.getByRole("button", { name: "Resume" }));
    fireEvent.click(controls.getByRole("button", { name: "Disconnect Nordwerk Components" }));
    expect(screen.getByText(/will stop sending supplier prices/)).toBeTruthy();
    expect(screen.getByText("Nordwerk Components")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Disconnect" }));
    await waitFor(() => expect(screen.queryByText("Nordwerk Components")).toBeNull());
  });

  test("keeps a connection when disconnect confirmation is cancelled", () => {
    renderView();
    fireEvent.click(screen.getByRole("button", { name: "Disconnect Nordwerk Components" }));
    fireEvent.click(screen.getByRole("button", { name: "Keep connected" }));
    expect(screen.getByText("Nordwerk Components")).toBeTruthy();
  });

  test("offers advanced synchronization, matching, mapping and authentication controls", () => {
    renderView();
    fireEvent.click(screen.getByRole("button", { name: "Connect supplier prices" }));
    const dialog = screen.getByRole("dialog", { name: "Connect supplier prices" });
    fireEvent.click(within(dialog).getByText("Advanced settings"));

    expect(within(dialog).getByLabelText("Check for updates")).toBeTruthy();
    expect(within(dialog).getByLabelText("Apply price changes")).toBeTruthy();
    expect(within(dialog).getByLabelText("Match products by")).toBeTruthy();
    expect(within(dialog).getByLabelText("New supplier products")).toBeTruthy();

    fireEvent.click(within(dialog).getByLabelText("Connection type"));
    fireEvent.click(within(dialog).getByRole("option", { name: "External pricing API" }));
    expect(within(dialog).getByText("Supplier field mapping")).toBeTruthy();
    expect(within(dialog).getByText("Custom authentication header")).toBeTruthy();
    expect(within(dialog).getByLabelText("Net price field")).toBeTruthy();
    expect(within(dialog).getByLabelText("Header name")).toBeTruthy();
  });
});
