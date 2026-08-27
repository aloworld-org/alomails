import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ImageContentBlock } from "./ImageContentBlock";

describe("ImageContentBlock", () => {
  it("keeps editable and read-only image behavior distinct", () => {
    const onEdit = vi.fn();
    const block = { id: "image-1", kind: "image" as const, src: "/product.png", caption: "Product", body: "Details" };
    const { rerender } = render(<ImageContentBlock block={block} readOnly={false} onEdit={onEdit} />);
    fireEvent.doubleClick(screen.getByRole("img", { name: "Product" }));
    expect(onEdit).toHaveBeenCalledOnce();
    rerender(<ImageContentBlock block={block} readOnly onEdit={onEdit} />);
    fireEvent.doubleClick(screen.getByRole("img", { name: "Product" }));
    expect(onEdit).toHaveBeenCalledOnce();
  });
});
