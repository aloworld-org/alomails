import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitePagesPanel } from "./SitePagesPanel";
import type { SitePage } from "./types";

const page: SitePage = {
  id: "page-1",
  slug: "studio",
  title: "Studio",
  home: false,
  navOrder: 1,
  createdAt: "2026-08-30T10:00:00Z",
  updatedAt: "2026-08-30T10:00:00Z",
  seoTitle: null,
  seoDescription: null,
};

const homePage: SitePage = {
  id: "page-home",
  slug: "",
  title: "Home",
  home: true,
  navOrder: 0,
  createdAt: "2026-08-30T09:00:00Z",
  updatedAt: "2026-08-30T09:00:00Z",
  seoTitle: "Alo demo site",
  seoDescription: "A complete website preview.",
};

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{location.pathname}</output>;
}

describe("SitePagesPanel", () => {
  afterEach(() => cleanup());

  test("opens a page from anywhere on its row", () => {
    render(
      <MemoryRouter initialEntries={["/sites/site-1"]}>
        <LocationProbe />
        <Routes>
          <Route
            path="/sites/:siteId"
            element={
              <SitePagesPanel
                pages={[page]}
                loading={false}
                protectedPages={new Set()}
                siteStatus="draft"
                enabledLocales={["en"]}
                onTheme={vi.fn()}
                onCreate={vi.fn()}
                onRename={vi.fn()}
                onDuplicate={vi.fn()}
                onSetHome={vi.fn()}
                onDelete={vi.fn()}
              />
            }
          />
          <Route path="/sites/:siteId/pages/:pageId" element={null} />
        </Routes>
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByText("/studio"));
    expect(screen.getByTestId("location").textContent).toBe(
      "/sites/site-1/pages/page-1",
    );
  });

  test("keeps theme and page creation as clear actions", () => {
    const onTheme = vi.fn();
    const onCreate = vi.fn();
    render(
      <MemoryRouter>
        <SitePagesPanel
          pages={[page]}
          loading={false}
          protectedPages={new Set()}
          siteStatus="draft"
          enabledLocales={["en"]}
          onTheme={onTheme}
          onCreate={onCreate}
          onRename={vi.fn()}
          onDuplicate={vi.fn()}
          onSetHome={vi.fn()}
          onDelete={vi.fn()}
        />
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole("button", { name: strings.sitesTheme }));
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesNewPage }),
    );

    expect(onTheme).toHaveBeenCalledOnce();
    expect(onCreate).toHaveBeenCalledOnce();
  });

  test("filters pages without losing the bottom create action", () => {
    const onCreate = vi.fn();
    render(
      <MemoryRouter>
        <SitePagesPanel
          pages={[homePage, page]}
          loading={false}
          protectedPages={new Set(["page-1"])}
          siteStatus="live"
          enabledLocales={["en", "fr", "nl", "de"]}
          onTheme={vi.fn()}
          onCreate={onCreate}
          onRename={vi.fn()}
          onDuplicate={vi.fn()}
          onSetHome={vi.fn()}
          onDelete={vi.fn()}
        />
      </MemoryRouter>,
    );

    fireEvent.change(screen.getByLabelText(strings.sitesSearchPages), {
      target: { value: "studio" },
    });

    expect(screen.getByText("Studio")).toBeTruthy();
    expect(screen.queryByText("Home")).toBeNull();

    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesFilterProtectedPages }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesNewPage }),
    );

    expect(onCreate).toHaveBeenCalledOnce();
  });

  test("shows search and access readiness on page rows", () => {
    render(
      <MemoryRouter>
        <SitePagesPanel
          pages={[homePage, page]}
          loading={false}
          protectedPages={new Set(["page-1"])}
          siteStatus="live"
          enabledLocales={["en", "fr", "nl", "de"]}
          onTheme={vi.fn()}
          onCreate={vi.fn()}
          onRename={vi.fn()}
          onDuplicate={vi.fn()}
          onSetHome={vi.fn()}
          onDelete={vi.fn()}
        />
      </MemoryRouter>,
    );

    expect(screen.getAllByText(strings.sitesSeoReady)).toHaveLength(1);
    expect(screen.getByText(strings.sitesSeoNeedsWork)).toBeTruthy();
    expect(screen.getByText(strings.sitesPagePasswordBadge)).toBeTruthy();
    expect(screen.getByText(strings.sitesPublicPage)).toBeTruthy();
    expect(screen.getAllByText(strings.sitesStatusPublished).length).toBeGreaterThan(0);
    expect(screen.getAllByText("FR").length).toBeGreaterThan(0);
  });
});
