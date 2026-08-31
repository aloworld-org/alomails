import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { strings } from "../i18n";
import { BrandApplicationPreview } from "./BrandApplicationPreview";
import { DEFAULT_BRAND_KIT } from "./model";

test("application preview exposes a keyboard-addressable tab for every output", () => {
  render(<BrandApplicationPreview kit={DEFAULT_BRAND_KIT} />);
  const documentTab = screen.getByRole("tab", { name: strings.brandingPreviewWorkspaceDocument });
  fireEvent.click(documentTab);
  expect(documentTab.getAttribute("aria-selected")).toBe("true");
});
