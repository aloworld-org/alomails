import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { BrandToneScale } from "./BrandToneScale";
import { toneScale } from "./colorTools";

test("each generated tone can be copied by its exact hex value", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  const first = toneScale("#E76F51")[0]!;
  render(<BrandToneScale color="#E76F51" />);

  fireEvent.click(screen.getByRole("button", { name: strings.brandingCopyColor(first) }));

  expect(writeText).toHaveBeenCalledWith(first);
  await waitFor(() => expect(screen.getByRole("button", { name: strings.brandingColorCopied(first) })).toBeTruthy());
});
