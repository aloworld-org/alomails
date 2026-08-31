import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { strings } from "../i18n";
import { BrandApplicationsView } from "./BrandApplicationsView";
import { DEFAULT_BRAND_KIT } from "./model";

test("application review switches between customer outputs", () => {
  render(<BrandApplicationsView kit={DEFAULT_BRAND_KIT} />);
  fireEvent.click(screen.getByRole("tab", { name: strings.brandingPreviewWorkspaceDocument }));
  expect(screen.getByText(strings.brandingPreviewDocumentHeading)).toBeTruthy();
});
