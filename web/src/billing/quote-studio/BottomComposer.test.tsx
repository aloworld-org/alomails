import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { strings } from "../../i18n";
import { BottomComposer } from "./BottomComposer";

describe("BottomComposer", () => {
  afterEach(cleanup);

  it("uses whitespace instead of decorative rules between content blocks", () => {
    const { container } = render(
      <BottomComposer index={1} onAdd={vi.fn()} onImage={vi.fn()} />,
    );

    expect(container.querySelector(".h-px")).toBeNull();
    expect(
      container.querySelector<HTMLButtonElement>("button")?.className,
    ).toContain("!bg-accent-soft");
  });

  it("opens, filters, and inserts a selected block at the requested index", () => {
    const onAdd = vi.fn();
    const { getByRole } = render(
      <BottomComposer index={3} onAdd={onAdd} onImage={vi.fn()} />,
    );

    fireEvent.click(getByRole("button", { name: strings.quoteStudioAddContentBelow }));
    fireEvent.change(getByRole("textbox", { name: strings.quoteStudioSearchBlocksA11y }), {
      target: { value: strings.quoteStudioDivider },
    });
    fireEvent.click(getByRole("button", { name: new RegExp(strings.quoteStudioDivider) }));

    expect(onAdd).toHaveBeenCalledWith(3, "divider", false);
  });
});
