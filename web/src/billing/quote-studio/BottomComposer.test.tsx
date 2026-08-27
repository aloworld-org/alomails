import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { strings } from "../../i18n";
import { BottomComposer } from "./BottomComposer";

describe("BottomComposer", () => {
  it("opens, filters, and inserts a selected block at the requested index", () => {
    const onAdd = vi.fn();
    render(<BottomComposer index={3} onAdd={onAdd} onImage={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: strings.quoteStudioAddContentBelow }));
    fireEvent.change(screen.getByRole("textbox", { name: strings.quoteStudioSearchBlocksA11y }), {
      target: { value: strings.quoteStudioDivider },
    });
    fireEvent.click(screen.getByRole("button", { name: new RegExp(strings.quoteStudioDivider) }));

    expect(onAdd).toHaveBeenCalledWith(3, "divider", false);
  });
});
