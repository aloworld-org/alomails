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

    fireEvent.click(screen.getByText("Studio"));
    expect(screen.getByTestId("location").textContent).toBe(
      "/sites/site-1/pages/page-1",
    );
  });

  test("keeps the quotation-style page creation action", () => {
    const onCreate = vi.fn();
    render(
      <MemoryRouter>
        <SitePagesPanel
          pages={[page]}
          loading={false}
          protectedPages={new Set()}
          siteStatus="draft"
          onCreate={onCreate}
          onRename={vi.fn()}
          onDuplicate={vi.fn()}
          onSetHome={vi.fn()}
          onDelete={vi.fn()}
        />
      </MemoryRouter>,
    );

    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesNewPage }),
    );

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

    fireEvent.change(screen.getByLabelText(strings.sitesPageFilter), {
      target: { value: "protected" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesNewPage }),
    );

    expect(onCreate).toHaveBeenCalledOnce();
  });

  test("keeps the page list focused on essential status", () => {
    render(
      <MemoryRouter>
        <SitePagesPanel
          pages={[homePage, page]}
          loading={false}
          protectedPages={new Set(["page-1"])}
          siteStatus="live"
          onCreate={vi.fn()}
          onRename={vi.fn()}
          onDuplicate={vi.fn()}
          onSetHome={vi.fn()}
          onDelete={vi.fn()}
        />
      </MemoryRouter>,
    );

    expect(screen.queryByText(strings.sitesColSeo)).toBeNull();
    expect(screen.queryByText(strings.sitesColAccess)).toBeNull();
    expect(screen.getAllByText(strings.sitesStatusPublished).length).toBeGreaterThan(0);
  });

  test("nests child pages under an expandable parent", () => {
    render(
      <MemoryRouter>
        <SitePagesPanel
          pages={[homePage, { ...page, parentId: homePage.id }]}
          loading={false}
          protectedPages={new Set()}
          siteStatus="draft"
          onCreate={vi.fn()}
          onRename={vi.fn()}
          onDuplicate={vi.fn()}
          onSetHome={vi.fn()}
          onDelete={vi.fn()}
        />
      </MemoryRouter>,
    );

    const rows = screen
      .getAllByRole("row")
      .filter((row) => row.hasAttribute("aria-level"));
    expect(rows[0]?.getAttribute("aria-level")).toBe("1");
    expect(rows[1]?.getAttribute("aria-level")).toBe("2");

    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesCollapseChildPages }),
    );
    expect(screen.queryByText("Studio")).toBeNull();
  });
});
