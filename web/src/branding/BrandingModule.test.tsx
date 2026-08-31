import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";

import { strings } from "../i18n";

vi.mock("./repository", () => ({
  readBrandKit: () => ({
    foundation: { name: "", tagline: "", purpose: "", audience: "", positioning: "", personality: "", voice: "" },
    logos: [],
    primaryLogoId: null,
    typography: { heading: "inter", body: "inter" },
    primary: { id: "primary", name: "Primary", value: "#E76F51" },
    secondary: { id: "secondary", name: "Secondary", value: "#102A43" },
    supporting: [],
  }),
  saveBrandKit: (kit: unknown) => kit,
}));

import { BrandingModule } from "./BrandingModule";

afterEach(cleanup);

function renderBranding(path = "/branding/foundation") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes><Route path="/branding/*" element={<BrandingModule />} /></Routes>
    </MemoryRouter>,
  );
}

describe("Branding", () => {
  test("opens with four clear areas and a foundation editor", () => {
    renderBranding();
    expect(screen.getByRole("heading", { name: strings.brandingTitle })).toBeTruthy();
    expect(screen.getByRole("navigation", { name: strings.brandingNavLabel }).querySelectorAll("a")).toHaveLength(4);
    expect(screen.getByRole("heading", { name: strings.brandingFoundationTitle })).toBeTruthy();
    expect(screen.getByLabelText(strings.brandingBrandName)).toBeTruthy();
  });

  test("keeps the existing colour editor under visual identity", () => {
    renderBranding("/branding/visual-identity");
    expect(screen.getByRole("heading", { name: strings.brandingVisualIdentityTitle })).toBeTruthy();
    expect(screen.getByRole("heading", { name: strings.brandingPrimary })).toBeTruthy();
    expect(screen.getByRole("heading", { name: strings.brandingSecondary })).toBeTruthy();
    expect(screen.getByRole("heading", { name: strings.brandingLogoTitle })).toBeTruthy();
  });

  test("previews the shared kit across four real applications", () => {
    renderBranding("/branding/applications");
    expect(screen.getByRole("tab", { name: strings.brandingPreviewWebsite }).getAttribute("aria-selected")).toBe("true");
    fireEvent.click(screen.getByRole("tab", { name: strings.brandingPreviewDocument }));
    expect(screen.getByText("QUO-2026-0042")).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: strings.brandingPreviewCampaign }));
    expect(screen.getByText(strings.brandingPreviewCampaignBadge)).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: strings.brandingPreviewWorkspaceDocument }));
    expect(screen.getByText(strings.brandingPreviewDocumentType)).toBeTruthy();
  });

  test("generates printable guidelines from the same draft", () => {
    renderBranding("/branding/guidelines");
    expect(screen.getByRole("heading", { name: strings.brandingGuidelinesTitle })).toBeTruthy();
    expect(screen.getByRole("heading", { name: strings.brandingGuidelineFoundation })).toBeTruthy();
    expect(screen.getByRole("heading", { name: strings.brandingGuidelineColors })).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.brandingPrintGuidelines })).toBeTruthy();
  });
});
