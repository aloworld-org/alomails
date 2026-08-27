import { render } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { Card } from "./Card";

describe("Card", () => {
  test("keeps interactive cards spatially stable on hover", () => {
    const { container } = render(<Card interactive>Content</Card>);
    const className = container.firstElementChild?.getAttribute("class") ?? "";

    expect(className).toContain("hover:bg-raised");
    expect(className).not.toMatch(/hover:(?:-?translate|scale|shadow)/);
  });
});
