import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";

import { strings } from "../../i18n";
import { CustomizeTable } from "./CustomizeTable";
import { EMPTY_QUOTE_STUDIO_DESIGN } from "./quoteStudioNormalization";

function Subject() {
  const [design, setDesign] = useState(EMPTY_QUOTE_STUDIO_DESIGN);
  return (
    <CustomizeTable
      design={design}
      saveError=""
      onChange={setDesign}
      onClose={() => undefined}
    />
  );
}

describe("CustomizeTable", () => {
  it("exports the pricing-table customization dialog", () => {
    expect(CustomizeTable).toBeTypeOf("function");
  });

  it("offers four visual totals styles and applies the selected style", () => {
    render(<Subject />);

    const choices = (["soft", "minimal", "framed", "accent"] as const).map(
      (style) =>
        screen.getByRole("button", {
          name: strings.quoteStudioTotalsStyleName(style),
        }),
    );
    expect(choices).toHaveLength(4);
    expect(choices[0]?.getAttribute("aria-pressed")).toBe("true");

    fireEvent.click(choices[3]!);

    expect(choices[3]?.getAttribute("aria-pressed")).toBe("true");
    expect(choices[3]?.querySelector("svg")).not.toBeNull();
  });
});
