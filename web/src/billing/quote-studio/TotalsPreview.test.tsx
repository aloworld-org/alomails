import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { TotalsPreview } from "./TotalsPreview";

describe("TotalsPreview", () => {
  it.each([
    ["summary", "w-1/2"],
    ["full", "w-full"],
    ["footer", "rounded-t-none"],
  ] as const)("renders the %s placement", (placement, expectedClass) => {
    const { container } = render(<TotalsPreview placement={placement} />);
    expect(container.querySelector(`.${expectedClass.replace("/", "\\/")}`)).not.toBeNull();
  });
});
