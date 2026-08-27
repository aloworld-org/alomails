import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import {
  QuoteTableOptionsProvider,
  type QuoteTableOptionsValue,
  useQuoteTableOptions,
} from "./quoteTableOptions";

function Consumer() {
  const options = useQuoteTableOptions();
  return (
    <button type="button" onClick={() => options.updateLineContent("line-1", { description: "Updated" })}>
      {options.layout}:{options.totalsPlacement}:{String(options.showImages)}
    </button>
  );
}

describe("QuoteTableOptionsProvider", () => {
  test("exposes the table configuration and update callback to descendants", () => {
    const updateLineContent = vi.fn();
    const value: QuoteTableOptionsValue = {
      enabled: true,
      layout: "catalogue",
      showImages: true,
      showDescriptions: true,
      totalsPlacement: "footer",
      totalsDetail: "breakdown",
      showCurrencyCode: true,
      emphasizeTotal: true,
      showTaxNote: true,
      lineContent: {},
      updateLineContent,
    };

    render(
      <QuoteTableOptionsProvider value={value}>
        <Consumer />
      </QuoteTableOptionsProvider>,
    );
    fireEvent.click(screen.getByRole("button"));

    expect(screen.getByText("catalogue:footer:true")).toBeTruthy();
    expect(updateLineContent).toHaveBeenCalledWith("line-1", { description: "Updated" });
  });
});
