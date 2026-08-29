import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import * as subject from "./TotalsPanel";
import { QuoteTableOptionsProvider } from "./quoteTableOptions";

describe("TotalsPanel", () => {
  it("exports its public component API", () => {
    expect(Object.keys(subject).length).toBeGreaterThan(0);
  });

  it("uses the selected accent treatment for the final amount", () => {
    const { container } = render(
      <QuoteTableOptionsProvider
        value={{
          enabled: true,
          layout: "compact",
          showImages: false,
          showDescriptions: false,
          totalsPlacement: "summary",
          totalsDetail: "summary",
          totalsStyle: "accent",
          showCurrencyCode: false,
          emphasizeTotal: true,
          showTaxNote: false,
          lineContent: {},
          updateLineContent: vi.fn(),
        }}
      >
        <subject.TotalsPanel
          totals={{
            netCents: 97516,
            vatCents: 19181,
            grossCents: 116697,
            vatByRate: [{ rateBp: 2000, netCents: 95905, vatCents: 19181 }],
          }}
          currency="EUR"
          stale={false}
        />
      </QuoteTableOptionsProvider>,
    );

    expect(container.querySelector("dl > div:last-child")?.className).toContain(
      "bg-accent",
    );
  });
});
