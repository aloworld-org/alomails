import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";

vi.mock("./repository", () => ({
  readBrandKit: () => ({
    primary: { id: "primary", name: "Primary", value: "#E76F51" },
    secondary: { id: "secondary", name: "Secondary", value: "#102A43" },
    supporting: [],
  }),
  saveBrandKit: (kit: unknown) => kit,
}));

import { BrandingModule } from "./BrandingModule";

afterEach(cleanup);

describe("Branding", () => {
  test("starts with the recommended roles and previews changes", () => {
    render(<BrandingModule />);
    expect(screen.getByRole("heading", { name: strings.brandingTitle })).toBeTruthy();
    expect(screen.getByRole("heading", { name: strings.brandingPrimary })).toBeTruthy();
    expect(screen.getByRole("heading", { name: strings.brandingSecondary })).toBeTruthy();
    expect(screen.getByText(strings.brandingSeeItInUse)).toBeTruthy();
    expect(screen.getByRole("tab", { name: strings.brandingPreviewWebsite }).getAttribute("aria-selected")).toBe("true");
  });

  test("previews the palette across websites, documents, and campaigns", () => {
    render(<BrandingModule />);
    fireEvent.click(screen.getByRole("tab", { name: strings.brandingPreviewDocument }));
    expect(screen.getByText("QUO-2026-0042")).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: strings.brandingPreviewCampaign }));
    expect(screen.getByText("NEW COLLECTION")).toBeTruthy();
  });

  test("keeps field guidance behind accessible information controls", () => {
    render(<BrandingModule />);
    expect(screen.queryByText(strings.brandingPrimaryHint)).toBeNull();
    fireEvent.click(screen.getByLabelText(strings.brandingMoreInfo(strings.brandingPrimary)));
    expect(screen.getByText(strings.brandingPrimaryHint)).toBeTruthy();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByText(strings.brandingPrimaryHint)).toBeNull();
  });

  test("adds named supporting colors and keeps them optional", () => {
    render(<BrandingModule />);
    fireEvent.click(screen.getByText(strings.brandingAddSupporting));
    const name = screen.getByLabelText(strings.brandingColorName) as HTMLInputElement;
    fireEvent.change(name, { target: { value: "Sage" } });
    expect(screen.getByDisplayValue("Sage")).toBeTruthy();
    fireEvent.click(screen.getByLabelText(strings.brandingRemoveColor("Sage")));
    expect(screen.queryByDisplayValue("Sage")).toBeNull();
  });

  test("lets a small brand work without a secondary accent", () => {
    render(<BrandingModule />);
    fireEvent.click(screen.getByLabelText(strings.brandingRemoveColor("Secondary")));
    expect(screen.getByText(strings.brandingAddSecondary)).toBeTruthy();
  });
});
