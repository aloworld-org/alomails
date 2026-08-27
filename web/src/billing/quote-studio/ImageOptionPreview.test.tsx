import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ImageOptionPreview } from "./ImageOptionPreview";

describe("ImageOptionPreview", () => {
  it.each([
    ["composition", "left"],
    ["frame", "square"],
    ["fit", "cover"],
  ] as const)("renders a %s preview", (kind, option) => {
    const { container } = render(<ImageOptionPreview kind={kind} option={option} />);
    expect(container.firstElementChild).not.toBeNull();
  });
});
