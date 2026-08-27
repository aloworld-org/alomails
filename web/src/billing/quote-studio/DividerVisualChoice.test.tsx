import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { DividerVisualChoice } from "./DividerVisualChoice";

describe("DividerVisualChoice", () => {
  it("reports selection and handles activation", () => {
    const onClick = vi.fn();
    const view = render(
      <DividerVisualChoice label="Dashed" selected onClick={onClick}>
        <span>—</span>
      </DividerVisualChoice>,
    );
    const button = view.getByRole("button", { name: "Dashed" });
    expect(button.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });
});
