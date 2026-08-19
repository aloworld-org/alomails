import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { Button } from "./Button";

describe("Button", () => {
  test("uses brand orange for every primary call to action", () => {
    render(<Button>Continue</Button>);

    const button = screen.getByRole("button", { name: "Continue" });
    expect(button.className).toContain("!bg-accent");
    expect(button.className).toContain("!text-on-accent");
    expect(button.className).toContain("enabled:hover:!bg-accent-hover");
  });

  test("protects every label from the button edges", () => {
    const { rerender } = render(<Button>Save changes</Button>);

    let button = screen.getByRole("button", { name: "Save changes" });
    expect(button.className).toContain("min-h-10");
    expect(button.className).toContain("px-6");
    expect(button.className).toContain("shrink-0");

    rerender(<Button size="sm">Save changes</Button>);
    button = screen.getByRole("button", { name: "Save changes" });
    expect(button.className).toContain("min-h-10");
    expect(button.className).toContain("px-5");
  });
});
