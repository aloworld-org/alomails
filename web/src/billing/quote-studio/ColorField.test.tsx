import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ColorField } from "./ColorField";

describe("ColorField", () => {
  it("normalizes edited hexadecimal values", () => {
    const onChange = vi.fn();
    render(
      <ColorField
        label="Accent"
        help="Brand actions"
        value="#E76F51"
        onChange={onChange}
      />,
    );

    fireEvent.change(screen.getByLabelText(/hex colour/i), {
      target: { value: "102A43" },
    });

    expect(onChange).toHaveBeenCalledWith("#102A43");
  });
});
