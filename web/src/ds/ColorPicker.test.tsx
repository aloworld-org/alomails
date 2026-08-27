import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { ColorPicker } from "./ColorPicker";

describe("ColorPicker", () => {
  test("opens the Alo picker and applies a preset colour", () => {
    const onChange = vi.fn();
    render(<ColorPicker label="Choose accent colour" value="#102A43" onChange={onChange} />);

    fireEvent.click(screen.getByRole("button", { name: "Choose accent colour" }));

    expect(screen.getByRole("dialog", { name: "Choose accent colour" })).toBeTruthy();
    expect((screen.getByLabelText("Hex colour") as HTMLInputElement).value).toBe("#102A43");

    fireEvent.click(screen.getByRole("button", { name: "Use #E76F51" }));
    expect(onChange).toHaveBeenCalledWith("#E76F51");
  });

  test("keeps disabled colour controls visible and non-interactive", () => {
    render(<ColorPicker label="Choose fill colour" value="#E76F51" onChange={vi.fn()} disabled />);

    expect((screen.getByRole("button", { name: "Choose fill colour" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
