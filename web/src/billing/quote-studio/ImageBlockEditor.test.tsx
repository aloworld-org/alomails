import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { strings } from "../../i18n";
import { ImageBlockEditor } from "./ImageBlockEditor";

describe("ImageBlockEditor", () => {
  it("exposes replacement and completion actions", () => {
    const onReplace = vi.fn();
    const onClose = vi.fn();
    render(<ImageBlockEditor block={{ id: "image-1", kind: "image", src: "/product.png", caption: "Product" }} onChange={vi.fn()} onReplace={onReplace} onClose={onClose} />);
    fireEvent.click(screen.getByRole("button", { name: strings.quoteStudioReplace }));
    fireEvent.click(screen.getByRole("button", { name: strings.quoteStudioDone }));
    expect(onReplace).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });
});
