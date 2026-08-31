import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { strings } from "../i18n";
import { DocumentBrandPreview } from "./DocumentBrandPreview";
import { DEFAULT_BRAND_KIT } from "./model";

test("document preview uses foundation content when it is available", () => {
  const kit = { ...DEFAULT_BRAND_KIT, foundation: { ...DEFAULT_BRAND_KIT.foundation, purpose: "Make complex work clear." } };
  render(<DocumentBrandPreview kit={kit} />);
  expect(screen.getByText("Make complex work clear.")).toBeTruthy();
  expect(screen.getByText(strings.brandingPreviewDocumentType)).toBeTruthy();
});
