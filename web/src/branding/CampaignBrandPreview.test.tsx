import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { strings } from "../i18n";
import { CampaignBrandPreview } from "./CampaignBrandPreview";
import { DEFAULT_BRAND_KIT } from "./model";

test("campaign preview shows the localized campaign treatment", () => {
  render(<CampaignBrandPreview kit={DEFAULT_BRAND_KIT} />);
  expect(screen.getByText(strings.brandingPreviewCampaignHeading)).toBeTruthy();
  expect(screen.getByText(strings.brandingPreviewCampaignAction)).toBeTruthy();
});
