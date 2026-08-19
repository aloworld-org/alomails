import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { Button } from "./Button";

describe("Button", () => {
  test("uses brand orange for every primary call to action", () => {
    render(<Button>Continue</Button>);

    const button = screen.getByRole("button", { name: "Continue" });
    expect(button.className).toContain("bg-accent");
    expect(button.className).toContain("text-on-accent");
    expect(button.className).toContain("enabled:hover:bg-accent-hover");
  });
});
