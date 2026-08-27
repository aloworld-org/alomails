import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ImageZoomControl } from "./ImageZoomControl";

describe("ImageZoomControl", () => {
  it("increments and resets zoom", () => {
    const onChange = vi.fn();
    render(<ImageZoomControl value={125} onChange={onChange} />);
    fireEvent.click(screen.getByRole("button", { name: /zoom in/i }));
    fireEvent.click(screen.getByRole("button", { name: /reset/i }));
    expect(onChange).toHaveBeenNthCalledWith(1, 150);
    expect(onChange).toHaveBeenNthCalledWith(2, 100);
  });
});
