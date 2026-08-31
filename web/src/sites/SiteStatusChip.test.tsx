import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { strings } from "../i18n";
import { SiteStatusChip } from "./SiteStatusChip";

test("names the website publication state", () => {
  render(<SiteStatusChip status="draft" />);
  expect(screen.getByText(strings.sitesStatusDraft)).toBeTruthy();
});
