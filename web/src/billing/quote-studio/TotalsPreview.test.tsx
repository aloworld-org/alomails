import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { TotalsPreview } from "./TotalsPreview";

describe("TotalsPreview", () => {
  it.each([
    ["summary", "w-1/2"],
    ["full", "w-full"],
    ["footer", "rounded-t-none"],
  ] as const)("renders the %s placement", (placement, expectedClass) => {
    const { container } = render(<TotalsPreview placement={placement} style="soft" />);
    expect(container.querySelector(`.${expectedClass.replace("/", "\\/")}`)).not.toBeNull();
  });

  it.each([
    ["soft", "bg-raised/70"],
    ["minimal", "border-transparent"],
    ["framed", "border-primary/25"],
    ["accent", "bg-accent"],
  ] as const)("renders the %s totals style", (style, expectedClass) => {
    const { container } = render(<TotalsPreview placement="summary" style={style} />);

    expect(container.innerHTML).toContain(expectedClass);
  });
});
