import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { DocumentLines } from "./DocumentLines";
import {
  QuoteTableOptionsProvider,
  type QuoteTableOptionsValue,
} from "./quoteTableOptions";

const IMAGE = "data:image/png;base64,cHJvZHVjdA==";

afterEach(cleanup);

function renderLines(updateLineContent = vi.fn()) {
  const value: QuoteTableOptionsValue = {
    enabled: true,
    layout: "catalogue",
    showImages: true,
    showDescriptions: true,
    totalsPlacement: "summary",
    totalsDetail: "summary",
    showCurrencyCode: false,
    emphasizeTotal: true,
    showTaxNote: false,
    lineContent: {
      "product:p-1": { description: "", image: IMAGE },
    },
    updateLineContent,
  };

  render(
    <QuoteTableOptionsProvider value={value}>
      <DocumentLines
        rows={[
          {
            key: "line-1",
            productId: "p-1",
            description: "Industrial fan",
            unit: "item",
            qty: "1",
            price: "3800",
            rate: "21",
          },
        ]}
        products={[]}
        savedLines={[]}
        saved={false}
        currency="EUR"
        readOnly={false}
        onChange={vi.fn()}
        nextKey={() => "next"}
      />
    </QuoteTableOptionsProvider>,
  );

  return updateLineContent;
}

describe("pricing-table product image editor", () => {
  test("opens from the edit icon and applies PDF image settings", () => {
    const update = renderLines();

    fireEvent.click(screen.getByRole("button", { name: "Edit product image" }));
    expect(screen.getByRole("dialog", { name: "Edit product image" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "PDF preview" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Show full image" }));
    fireEvent.change(screen.getByRole("spinbutton", { name: "Custom zoom percentage" }), {
      target: { value: "180" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Top" }));
    fireEvent.click(screen.getByRole("button", { name: "Apply image" }));

    expect(update).toHaveBeenCalledWith("product:p-1", {
      image: IMAGE,
      imageFit: "contain",
      imagePosition: "top",
      imageZoom: 180,
    });
  });

  test("opens when the product image is double-clicked", () => {
    renderLines();
    fireEvent.doubleClick(screen.getByRole("img", { name: "Product image" }));
    expect(screen.getByRole("dialog", { name: "Edit product image" })).toBeTruthy();
  });
});
