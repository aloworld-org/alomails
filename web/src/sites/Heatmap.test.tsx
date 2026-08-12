// The attention map (S2.09b). What is pinned here is what an owner would be
// misled by if it broke:
//
//  - a map is not drawn below the minimum sample, and the screen says how far
//    off it is rather than showing a picture of three people;
//  - every coloured square has the same finding in words beside it, ordered
//    busiest first, and positions are described ("Centre, 30–40% down") rather
//    than given as grid coordinates;
//  - the depth curve keeps all ten tenths in depth order — the quiet ones are
//    the interesting ones;
//  - screen classes are separate, because a layout that reflows makes a shared
//    grid meaningless;
//  - a site nobody has clicked gets an onboarding, not an empty frame.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { HeatmapView } from "./HeatmapView";
import { HEATMAP_MINIMUM_SAMPLE, clickRegions, depthRows, sideLabel } from "./heatmapReading";
import type { SiteHeatmapCell, SiteHeatmapPage, SiteHeatmapReport } from "./types";

const mocks = vi.hoisted(() => ({
  site: vi.fn(),
  config: vi.fn(),
  heatmap: vi.fn(),
}));

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return { ...original, useSitesApi: () => mocks };
});

const COLUMNS = 32;
const ROWS = 64;

function cells(values: Array<[number, number, number]>): SiteHeatmapCell[] {
  return values.map(([column, row, hits]) => ({ column, row, hits }));
}

/** Ten tenths, as the server always sends them. */
function depth(hits: number[]) {
  return hits.map((count, bucket) => ({ bucket, hits: count }));
}

function viewport(
  name: string,
  clicks: SiteHeatmapCell[],
  scroll: number[] = new Array<number>(10).fill(0),
) {
  return {
    viewport: name,
    clicks,
    clickTotal: clicks.reduce((sum, cell) => sum + cell.hits, 0),
    scrollDepth: depth(scroll),
    scrollTotal: scroll.reduce((sum, count) => sum + count, 0),
  };
}

function page(overrides: Partial<SiteHeatmapPage> = {}): SiteHeatmapPage {
  return {
    path: "/prices",
    grid: { columns: COLUMNS, rows: ROWS },
    viewports: [
      // Phone: plenty of clicks, busiest in the centre near the top.
      viewport(
        "phone",
        cells([
          [16, 4, 30],
          [17, 5, 12],
          [2, 40, 8],
        ]),
        [40, 30, 20, 10, 6, 4, 2, 1, 0, 0],
      ),
      // Tablet: nothing at all.
      viewport("tablet", []),
      // Desktop: three clicks — real, but far below the sample floor.
      viewport("desktop", cells([[8, 8, 3]]), [4, 2, 0, 0, 0, 0, 0, 0, 0, 0]),
    ],
    ...overrides,
  };
}

function report(overrides: Partial<SiteHeatmapReport> = {}): SiteHeatmapReport {
  return {
    from: "2026-07-14",
    to: "2026-08-12",
    paths: [
      { path: "/prices", events: 126 },
      { path: "/", events: 40 },
    ],
    page: null,
    ...overrides,
  };
}

/** The endpoint answers the menu without a path and the grid with one — the
 *  screen asks for both, so the mock has to behave like the real route. */
function servesMenuAndPage(pageAnswer: SiteHeatmapPage | null = page()) {
  mocks.heatmap.mockImplementation(
    (_siteId: string, _days: number, path?: string) =>
      Promise.resolve(
        report(path === undefined ? {} : { page: pageAnswer }),
      ),
  );
}

function renderHeatmap() {
  return render(
    <MemoryRouter initialEntries={["/sites/site-1/heatmap"]}>
      <Routes>
        <Route path="/sites/:siteId/heatmap" element={<HeatmapView />} />
        <Route path="/sites/:siteId/analytics" element={<p>analytics</p>} />
      </Routes>
    </MemoryRouter>,
  );
}

function panel(title: string): HTMLElement {
  const heading = screen.getByRole("heading", { name: title });
  const section = heading.closest("section");
  if (section === null) throw new Error(`no panel around ${title}`);
  return section;
}

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  mocks.site.mockResolvedValue({
    id: "site-1",
    name: "Axon",
    subdomain: "axon",
    status: "live",
  });
  mocks.config.mockResolvedValue({ domain: "alosites.com" });
  servesMenuAndPage();
});

afterEach(cleanup);

describe("the attention map", () => {
  test("opens on the busiest page and the busiest screen size", async () => {
    renderHeatmap();
    await screen.findByRole("heading", { name: strings.sitesHeatmapClicks });

    // The menu is the pages that have data, most-active first, and the map
    // asked for is the first of them.
    const menu = screen.getByLabelText(strings.sitesHeatmapPage);
    expect((menu as HTMLSelectElement).value).toBe("/prices");
    expect(mocks.heatmap).toHaveBeenCalledWith("site-1", 30, "/prices");

    // Phone has the most counted, so that is the tab that is open.
    const phone = screen.getByRole("button", {
      name: strings.sitesHeatmapScreenTab(strings.sitesAnalyticsDevicePhone, "163"),
    });
    expect(phone.getAttribute("aria-pressed")).toBe("true");

    // The picture carries one honest label naming page, screen and count.
    expect(
      screen.getByRole("img", {
        name: strings.sitesHeatmapClicksLabel(
          "/prices",
          strings.sitesAnalyticsDevicePhone,
          50,
        ),
      }),
    ).toBeTruthy();
  });

  test("says the same thing in words, busiest area first", async () => {
    renderHeatmap();
    await screen.findByRole("heading", { name: strings.sitesHeatmapSpots });

    const rows = within(panel(strings.sitesHeatmapSpots)).getAllByRole("listitem");
    expect(rows[0]?.firstElementChild?.textContent).toBe(
      strings.sitesHeatmapSpot(strings.sitesHeatmapCentre, strings.sitesHeatmapDepthBand(0, 10)),
    );
    // …and the quieter one lower down the page is named, not dropped.
    expect(
      within(panel(strings.sitesHeatmapSpots)).getByText(
        strings.sitesHeatmapSpot(
          strings.sitesHeatmapLeft,
          strings.sitesHeatmapDepthBand(60, 70),
        ),
      ),
    ).toBeTruthy();
  });

  test("keeps all ten tenths of the depth curve, in depth order", async () => {
    renderHeatmap();
    await screen.findByRole("heading", { name: strings.sitesHeatmapDepth });
    const rows = within(panel(strings.sitesHeatmapDepth)).getAllByRole("listitem");
    expect(rows.length).toBe(10);
    expect(rows.map((row) => row.firstElementChild?.textContent)).toEqual(
      Array.from({ length: 10 }, (_, band) =>
        strings.sitesHeatmapDepthBand(band * 10, band * 10 + 10),
      ),
    );
  });

  test("a handful of clicks is held back, and says how far off it is", async () => {
    renderHeatmap();
    await screen.findByRole("heading", { name: strings.sitesHeatmapClicks });

    fireEvent.click(
      screen.getByRole("button", {
        name: strings.sitesHeatmapScreenTab(strings.sitesAnalyticsDeviceDesktop, "9"),
      }),
    );

    // Three clicks: no picture at all, and the reason in the owner's words.
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.getByText(strings.sitesHeatmapTooFewTitle)).toBeTruthy();
    expect(
      screen.getByText(strings.sitesHeatmapTooFewClicks(3, HEATMAP_MINIMUM_SAMPLE)),
    ).toBeTruthy();
    // The written summary is suppressed with it — otherwise the threshold
    // would only move the same three clicks into a list.
    expect(
      within(panel(strings.sitesHeatmapSpots)).queryAllByRole("listitem").length,
    ).toBe(0);
    // Six depth reports are below the floor too, and said separately.
    expect(
      screen.getByText(strings.sitesHeatmapTooFewDepth(6, HEATMAP_MINIMUM_SAMPLE)),
    ).toBeTruthy();
  });

  test("a screen size with nothing on it says so, and the others still count", async () => {
    renderHeatmap();
    await screen.findByRole("heading", { name: strings.sitesHeatmapClicks });

    fireEvent.click(
      screen.getByRole("button", {
        name: strings.sitesHeatmapScreenTab(strings.sitesAnalyticsDeviceTablet, "0"),
      }),
    );
    expect(screen.getByText(strings.sitesHeatmapClicksEmpty)).toBeTruthy();
    expect(screen.getByText(strings.sitesHeatmapSpotsEmpty)).toBeTruthy();
    expect(screen.getByText(strings.sitesHeatmapDepthEmpty)).toBeTruthy();
    // "Nothing here" is not "not enough here": no threshold copy is shown.
    expect(screen.queryByText(strings.sitesHeatmapTooFewTitle)).toBeNull();
  });

  test("choosing another page asks for that page and reopens on its busiest screen", async () => {
    renderHeatmap();
    await screen.findByRole("heading", { name: strings.sitesHeatmapClicks });
    fireEvent.click(
      screen.getByRole("button", {
        name: strings.sitesHeatmapScreenTab(strings.sitesAnalyticsDeviceDesktop, "9"),
      }),
    );

    servesMenuAndPage(
      page({
        path: "/",
        viewports: [
          viewport("phone", []),
          viewport("tablet", []),
          viewport("desktop", cells([[4, 4, 40]]), [50, 20, 0, 0, 0, 0, 0, 0, 0, 0]),
        ],
      }),
    );
    fireEvent.change(screen.getByLabelText(strings.sitesHeatmapPage), {
      target: { value: "/" },
    });

    await waitFor(() => {
      expect(mocks.heatmap).toHaveBeenCalledWith("site-1", 30, "/");
    });
    // The desktop choice made for the old page does not carry over as a
    // choice — the new page opens on its own busiest screen, which happens to
    // be desktop, and the map is drawn because it now has enough.
    await screen.findByRole("img", {
      name: strings.sitesHeatmapClicksLabel(
        "/",
        strings.sitesAnalyticsDeviceDesktop,
        40,
      ),
    });
  });

  test("a period change asks again over the new window", async () => {
    renderHeatmap();
    await screen.findByRole("heading", { name: strings.sitesHeatmapClicks });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesAnalyticsDays(7) }));
    await waitFor(() => {
      expect(mocks.heatmap).toHaveBeenCalledWith("site-1", 7, "/prices");
    });
  });

  test("a site nobody has clicked gets an onboarding, not an empty frame", async () => {
    mocks.heatmap.mockResolvedValue(report({ paths: [] }));
    renderHeatmap();
    await screen.findByText(strings.sitesHeatmapEmptyTitle);
    expect(screen.getByText(strings.sitesHeatmapPrivacyTitle)).toBeTruthy();
    expect(screen.queryByRole("img")).toBeNull();
    // And the endpoint was never asked for a page there is no menu entry for.
    expect(mocks.heatmap).toHaveBeenCalledTimes(1);
  });

  test("a failed read is shown, never swallowed", async () => {
    mocks.heatmap.mockRejectedValue(new Error("nope"));
    renderHeatmap();
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toBe(
        strings.sitesHeatmapLoadFailed,
      );
    });
  });
});

describe("reading a grid in words", () => {
  test("a column is named by the third of the width its centre falls in", () => {
    expect(sideLabel(0, COLUMNS)).toBe(strings.sitesHeatmapLeft);
    expect(sideLabel(10, COLUMNS)).toBe(strings.sitesHeatmapLeft);
    expect(sideLabel(11, COLUMNS)).toBe(strings.sitesHeatmapCentre);
    expect(sideLabel(20, COLUMNS)).toBe(strings.sitesHeatmapCentre);
    expect(sideLabel(21, COLUMNS)).toBe(strings.sitesHeatmapRight);
    expect(sideLabel(31, COLUMNS)).toBe(strings.sitesHeatmapRight);
  });

  test("cells in the same region are one row, and the last row is still the last tenth", () => {
    const regions = clickRegions(
      cells([
        [16, 0, 5],
        [17, 1, 5],
        [0, 63, 3],
      ]),
      COLUMNS,
      ROWS,
    );
    expect(regions).toEqual([
      {
        label: strings.sitesHeatmapSpot(
          strings.sitesHeatmapCentre,
          strings.sitesHeatmapDepthBand(0, 10),
        ),
        visits: 10,
      },
      {
        label: strings.sitesHeatmapSpot(
          strings.sitesHeatmapLeft,
          strings.sitesHeatmapDepthBand(90, 100),
        ),
        visits: 3,
      },
    ]);
  });

  test("the depth curve is sorted by depth even if the server is not", () => {
    expect(
      depthRows([
        { bucket: 2, hits: 3 },
        { bucket: 0, hits: 9 },
        { bucket: 1, hits: 5 },
      ]).map((row) => row.visits),
    ).toEqual([9, 5, 3]);
  });
});
