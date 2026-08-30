import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitePagesPanel } from "./SitePagesPanel";
import type { SitePage } from "./types";

const page: SitePage = {
  id: "page-1",
  slug: "studio",
  title: "Studio",
  home: false,
  seoTitle: null,
  seoDescription: null,
};

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{location.pathname}</output>;
}

describe("SitePagesPanel", () => {
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
                onTheme={vi.fn()}
                onCreate={vi.fn()}
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
          onTheme={onTheme}
          onCreate={onCreate}
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
});
