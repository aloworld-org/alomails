import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { EmptyBuilder } from "./EmptyBuilder";

describe("EmptyBuilder", () => {
  it("renders both editing and read-only states", () => {
    const { rerender, container } = render(<EmptyBuilder readOnly={false} />);
    expect(container.textContent).toBeTruthy();
    rerender(<EmptyBuilder readOnly />);
    expect(container.textContent).toBeTruthy();
  });
});
