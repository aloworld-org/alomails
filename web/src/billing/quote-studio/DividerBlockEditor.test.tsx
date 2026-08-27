import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DividerBlockEditor } from "./DividerBlockEditor";

describe("DividerBlockEditor", () => {
  it("renders the divider presentation", () => {
    const { container } = render(<DividerBlockEditor block={{ id: "d", kind: "divider" }} fallbackColor="#e76f51" onChange={vi.fn()} open={false} onOpenChange={vi.fn()} />);
    expect(container.querySelector('[aria-hidden="true"]')).not.toBeNull();
  });
});
