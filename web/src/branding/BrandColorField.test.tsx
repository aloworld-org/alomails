import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { BrandColorField } from "./BrandColorField";

test("editable brand color updates its meaningful name", () => {
  const onChange = vi.fn();
  render(<BrandColorField color={{ id: "supporting", name: "Sage", value: "#6B7280" }} title="Sage" editableName onChange={onChange} />);
  fireEvent.change(screen.getByLabelText(strings.brandingColorName), { target: { value: "Forest" } });
  expect(onChange).toHaveBeenCalledWith({ id: "supporting", name: "Forest", value: "#6B7280" });
});
