import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ImageColumnRatioPicker } from "./ImageColumnRatioPicker";

describe("ImageColumnRatioPicker", () => {
  it("changes the image-to-text ratio", () => {
    const onChange = vi.fn();
    render(<ImageColumnRatioPicker value="50-50" placement="left" onChange={onChange} />);
    fireEvent.click(screen.getByRole("button", { name: /40%.*60%/ }));
    expect(onChange).toHaveBeenCalledWith("40-60");
  });
});
