import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { CreationTemplatePreview } from "./CreationTemplatePreview";

describe("CreationTemplatePreview", () => {
  it("renders every quotation template visual", () => {
    for (const kind of ["blank", "services", "project", "retainer"] as const) {
      const { container, unmount } = render(<CreationTemplatePreview kind={kind} />);
      expect(container.firstElementChild).not.toBeNull();
      unmount();
    }
  });
});
