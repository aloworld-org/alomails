import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { BrandingHeader } from "./BrandingHeader";
import { DEFAULT_BRAND_KIT } from "./model";

test("branding header saves a valid dirty kit", () => {
  const save = vi.fn();
  render(<BrandingHeader brand={{ draft: DEFAULT_BRAND_KIT, setDraft: vi.fn(), dirty: true, valid: true, savedNotice: false, saveFailed: false, save }} />);
  fireEvent.click(screen.getByRole("button", { name: strings.brandingSave }));
  expect(save).toHaveBeenCalledOnce();
});
