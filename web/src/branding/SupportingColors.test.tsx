import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { DEFAULT_BRAND_KIT } from "./model";
import { SupportingColors } from "./SupportingColors";

test("supporting colors begin optional and can be added deliberately", () => {
  const onChange = vi.fn();
  render(<SupportingColors kit={DEFAULT_BRAND_KIT} onChange={onChange} />);
  fireEvent.click(screen.getAllByRole("button", { name: strings.brandingAddSupporting })[1]!);
  expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ supporting: [expect.objectContaining({ value: "#6B7280" })] }));
});
