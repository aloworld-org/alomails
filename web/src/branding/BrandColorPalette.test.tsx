import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { BrandColorPalette } from "./BrandColorPalette";
import { DEFAULT_BRAND_KIT } from "./model";

test("color palette can remove the optional secondary role", () => {
  const onChange = vi.fn();
  render(<BrandColorPalette kit={DEFAULT_BRAND_KIT} onChange={onChange} />);
  fireEvent.click(screen.getByLabelText(strings.brandingRemoveColor("Secondary")));
  expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ secondary: null }));
});

test("color palette does not crop the open color picker", () => {
  const { container } = render(<BrandColorPalette kit={DEFAULT_BRAND_KIT} onChange={vi.fn()} />);

  fireEvent.click(within(container).getByRole("button", { name: strings.brandingPrimary }));

  const dialog = within(container).getByRole("dialog", { name: strings.brandingPrimary });
  expect(dialog).toBeTruthy();
  expect(dialog.closest("section")?.className).not.toContain("overflow-hidden");
});
