import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { strings } from "../i18n";
import { FieldHelp } from "./FieldHelp";

test("field help opens its explanation on demand", () => {
  render(<FieldHelp title="Primary">Use it for important actions.</FieldHelp>);
  fireEvent.click(screen.getByLabelText(strings.brandingMoreInfo("Primary")));
  expect(screen.getByText("Use it for important actions.")).toBeTruthy();
});
