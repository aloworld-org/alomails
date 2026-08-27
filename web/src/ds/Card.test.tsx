import { render } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { Card } from "./Card";

describe("Card", () => {
  test("uses the shared comfortable card contract by default", () => {
    const { container } = render(<Card>Content</Card>);
    const className = container.firstElementChild?.getAttribute("class") ?? "";

    expect(className).toContain("rounded-2xl");
    expect(className).toContain("border-subtle");
    expect(className).toContain("bg-surface");
    expect(className).toContain("p-6");
    expect(className).toContain("shadow-sm");
  });

  test("gives only interactive cards one level of hover elevation", () => {
    const { container } = render(<Card interactive>Content</Card>);
    const className = container.firstElementChild?.getAttribute("class") ?? "";

    expect(className).toContain("hover:shadow-md");
    expect(className).not.toMatch(/hover:(?:-?translate|scale)/);
  });

  test("maps compact and spacious density to the shared spacing scale", () => {
    const { container, rerender } = render(<Card pad="sm">Compact</Card>);
    expect(container.firstElementChild?.getAttribute("class")).toContain("p-4");

    rerender(<Card pad="lg">Spacious</Card>);
    expect(container.firstElementChild?.getAttribute("class")).toContain("p-8");
  });
});
