import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { LayoutPreview } from "./LayoutPreview";

describe("LayoutPreview", () => {
  it("shows product imagery only for the catalogue layout", () => {
    const { rerender } = render(<LayoutPreview layout="catalogue" selected />);
    expect(screen.queryByTestId("catalogue-image")).not.toBeNull();
    rerender(<LayoutPreview layout="compact" selected={false} />);
    expect(screen.queryByTestId("catalogue-image")).toBeNull();
  });
});
