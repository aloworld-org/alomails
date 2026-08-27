import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { DividerLine } from "./DividerLine";

describe("DividerLine", () => {
  it("renders the selected divider appearance", () => {
    const { container } = render(
      <DividerLine
        block={{
          id: "divider",
          kind: "divider",
          thickness: "bold",
          style: "dashed",
          width: 50,
          color: "#E76F51",
        }}
      />,
    );
    const line = container.firstElementChild as HTMLElement;
    expect(line.className).toContain("border-t-4");
    expect(line.className).toContain("w-1/2");
    expect(line.style.borderTopStyle).toBe("dashed");
    expect(line.style.borderTopColor).toBe("rgb(231, 111, 81)");
  });
});
