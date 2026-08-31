import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { strings } from "../i18n";
import { ContrastRow } from "./ContrastRow";

test("contrast row recommends readable text for a dark color", () => {
  render(<ContrastRow label="Primary" color="#102A43" ink="#FFFFFF" />);
  expect(screen.getByText(strings.brandingUseLightText)).toBeTruthy();
});
