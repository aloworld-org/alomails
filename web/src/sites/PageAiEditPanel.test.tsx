import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { PageAiEditPanel } from "./PageAiEditPanel";

const mocks = vi.hoisted(() => ({
  proposePageEdit: vi.fn(),
  applyPageEdit: vi.fn(),
}));

vi.mock("./api", () => ({
  sitesMessage: () => "failed",
  useSitesApi: () => mocks,
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("PageAiEditPanel", () => {
  test("presents one compact instruction path without changing the page", () => {
    render(
      <PageAiEditPanel
        siteId="site-1"
        pageId="page-1"
        onApplied={vi.fn()}
        onPreviewChange={vi.fn()}
      />,
    );

    expect(screen.getByText(strings.sitesAiEditTitle)).toBeTruthy();
    expect(screen.getByLabelText(strings.sitesAiInstruction)).toBeTruthy();
    expect(
      screen
        .getByRole("button", { name: strings.sitesAiPropose })
        .hasAttribute("disabled"),
    ).toBe(true);
    expect(mocks.proposePageEdit).not.toHaveBeenCalled();
  });
});
