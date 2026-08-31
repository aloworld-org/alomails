import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { BrandTypographyFields } from "./BrandTypographyFields";

test("typography assigns independent heading and body roles", () => {
  const onChange = vi.fn();
  render(<BrandTypographyFields typography={{ heading: "inter", body: "inter" }} onChange={onChange} />);
  fireEvent.click(screen.getByRole("combobox", { name: strings.brandingHeadingFont }));
  fireEvent.click(screen.getByRole("option", { name: strings.brandingFontGeorgia }));
  expect(onChange).toHaveBeenCalledWith({ heading: "georgia", body: "inter" });
});
