import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

const { createProduct } = vi.hoisted(() => ({ createProduct: vi.fn(async () => ({})) }));

vi.mock("./api", () => ({
  useBillingApi: () => ({ createProduct, extractPriceListImage: vi.fn() }),
  billingMessage: (_error: unknown, fallback: string) => fallback,
}));

import { PriceImportDialog } from "./PriceImportDialog";

afterEach(() => { cleanup(); createProduct.mockClear(); });

describe("price import dialog", () => {
  test("previews a CSV and creates only confirmed valid rows", async () => {
    const onImported = vi.fn();
    const { container } = render(<PriceImportDialog existing={[]} onClose={vi.fn()} onImported={onImported} />);
    const picker = container.querySelector('input[type="file"]');
    expect(picker).not.toBeNull();
    fireEvent.change(picker!, { target: { files: [new File(["Product,Unit,Unit price,VAT rate\nConsulting,hour,125.50,21\nBroken,hour,nope,21"], "prices.csv", { type: "text/csv" })] } });

    await screen.findByText("Consulting");
    expect(screen.getByText("Invalid price")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Import 1 items" }));

    await waitFor(() => expect(createProduct).toHaveBeenCalledTimes(1));
    expect(createProduct).toHaveBeenCalledWith({ name: "Consulting", unit: "hour", unitPriceCents: 12550, vatRateBp: 2100 });
    expect(await screen.findByText("1 price-list items imported")).toBeTruthy();
  });
});
