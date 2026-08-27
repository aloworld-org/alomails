import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { HeaderField } from "./HeaderField";

describe("HeaderField", () => {
  it("labels the field and reports edits", () => {
    const onChange = vi.fn();
    render(
      <HeaderField
        id="company-name"
        label="Company name"
        value="Alo"
        placeholder="Company"
        onChange={onChange}
      />,
    );

    fireEvent.change(screen.getByLabelText("Company name"), {
      target: { value: "Ficina" },
    });
    expect(onChange).toHaveBeenCalledWith("Ficina");
  });
});
