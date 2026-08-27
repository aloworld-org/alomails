import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ImageOptionGroup } from "./ImageOptionGroup";

describe("ImageOptionGroup", () => {
  it("reports the selected option", () => {
    const onChange = vi.fn();
    render(<ImageOptionGroup label="Frame" value="natural" options={[["natural", "Natural"], ["square", "Square"]]} onChange={onChange} />);
    fireEvent.click(screen.getByRole("button", { name: "Square" }));
    expect(onChange).toHaveBeenCalledWith("square");
  });
});
