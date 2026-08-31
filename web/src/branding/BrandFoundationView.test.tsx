import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { BrandFoundationView } from "./BrandFoundationView";
import { DEFAULT_BRAND_KIT } from "./model";

test("brand foundation edits the shared domain object", () => {
  const onChange = vi.fn();
  render(<BrandFoundationView kit={DEFAULT_BRAND_KIT} onChange={onChange} />);
  fireEvent.change(screen.getByLabelText(strings.brandingBrandName), { target: { value: "Northstar" } });
  expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ foundation: expect.objectContaining({ name: "Northstar" }) }));
});
