import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { HeaderStylePreview, type HeaderStyle } from "./HeaderStylePreview";

describe("HeaderStylePreview", () => {
  it.each<HeaderStyle>(["signature", "editorial", "band", "minimal", "stacked"])(
    "renders the %s visual without exposing decorative content",
    (style) => {
      const { container } = render(<HeaderStylePreview style={style} />);
      expect(container.firstElementChild?.getAttribute("aria-hidden")).toBe(
        "true",
      );
      expect(container.querySelector("button")).toBeNull();
    },
  );
});
