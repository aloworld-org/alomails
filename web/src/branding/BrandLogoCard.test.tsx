import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { DialogProvider } from "../ds/Dialog";
import { BrandLogoCard } from "./BrandLogoCard";

const logo = { id: "logo-1", name: "alo-white.png", label: "alo-white", mimeType: "image/png" as const, dataUrl: "data:image/png;base64,AAAA" };

test("logo card identifies the primary variant and exposes management actions", async () => {
  const onRemove = vi.fn();
  const onRename = vi.fn();
  render(<DialogProvider><BrandLogoCard logo={logo} primary onMakePrimary={vi.fn()} onRename={onRename} onReplace={vi.fn()} onRemove={onRemove} /></DialogProvider>);
  expect(screen.getByText(strings.brandingLogoPrimary)).toBeTruthy();
  const name = screen.getByLabelText(strings.brandingLogoDisplayName);
  fireEvent.change(name, { target: { value: "Company long" } });
  fireEvent.blur(name);
  expect(onRename).toHaveBeenCalledWith("Company long");

  fireEvent.click(screen.getByRole("button", { name: strings.brandingLogoRemoveNamed(logo.label) }));
  expect(onRemove).not.toHaveBeenCalled();
  expect(screen.getByRole("dialog", { name: strings.brandingLogoRemoveTitle })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: strings.brandingLogoRemove }));
  await waitFor(() => expect(onRemove).toHaveBeenCalledOnce());
});
