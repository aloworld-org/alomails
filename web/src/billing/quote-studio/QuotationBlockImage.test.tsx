import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { QuotationBlockImage } from "./QuotationBlockImage";

describe("QuotationBlockImage", () => {
  it("renders the image and forwards double clicks", () => {
    const onDoubleClick = vi.fn();
    render(<QuotationBlockImage block={{ id: "image-1", kind: "image", src: "/product.png", caption: "Product" }} onDoubleClick={onDoubleClick} />);
    fireEvent.doubleClick(screen.getByRole("img", { name: "Product" }));
    expect(onDoubleClick).toHaveBeenCalledOnce();
  });
});
