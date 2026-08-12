// The traffic desk (S2.08b): the aggregates collected in S2.08a/a2 read as
// three calm groups rather than a wall of numbers.
//
// What is pinned here is what an owner would be misled by if it broke: stored
// buckets are shown as words in their language, the reading-time histogram
// stays in duration order (a histogram sorted by count is not a histogram),
// each panel says how its numbers get there, and a dimension nothing reported
// says so in its own words instead of showing an empty box.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { AnalyticsView } from "./AnalyticsView";
import type { SiteAnalyticsReport } from "./types";

const mocks = vi.hoisted(() => ({
  site: vi.fn(),
  config: vi.fn(),
  analytics: vi.fn(),
}));

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return { ...original, useSitesApi: () => mocks };
});

function dimension(values: Array<[string, number]>) {
  return values.map(([label, visits]) => ({ label, visits }));
}

function report(overrides: Partial<SiteAnalyticsReport> = {}): SiteAnalyticsReport {
  return {
    from: "2026-08-06",
    to: "2026-08-12",
    totals: { visits: 412, uniqueVisitors: 260 },
    daily: [
      { date: "2026-08-11", visits: 200, uniqueVisitors: 130 },
      { date: "2026-08-12", visits: 212, uniqueVisitors: 130 },
    ],
    topPages: [
      { path: "/", visits: 300, uniqueVisitors: 190 },
      { path: "/prices", visits: 112, uniqueVisitors: 70 },
    ],
    topReferrers: [
      { domain: "", visits: 260, uniqueVisitors: 170 },
      { domain: "news.example", visits: 152, uniqueVisitors: 90 },
    ],
    campaigns: dimension([
      ["", 300],
      ["spring-mailing", 112],
    ]),
    countries: dimension([
      ["NL", 210],
      ["BE", 120],
      ["", 82],
    ]),
    devices: dimension([
      ["phone", 220],
      ["desktop", 150],
      ["bot", 42],
    ]),
    entryPages: dimension([["/", 260]]),
    exitPages: dimension([["/prices", 140]]),
    // Server order: the store sorts this one by bucket, not by count.
    readTime: dimension([
      ["0-10s", 40],
      ["10-30s", 12],
      ["30-60s", 90],
      ["1-3m", 30],
      ["3-10m", 8],
      ["10m+", 2],
    ]),
    outboundDomains: dimension([
      ["shop.example", 30],
      ["other", 11],
    ]),
    ...overrides,
  };
}

function renderAnalytics() {
  return render(
    <MemoryRouter initialEntries={["/sites/site-1/analytics"]}>
      <Routes>
        <Route path="/sites/:siteId/analytics" element={<AnalyticsView />} />
        <Route path="/sites/:siteId" element={<p>site home</p>} />
      </Routes>
    </MemoryRouter>,
  );
}

/** The panel a heading names, so an assertion cannot accidentally match a
 *  number that belongs to the panel beside it. */
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
  mocks.analytics.mockResolvedValue(report());
});

afterEach(cleanup);

describe("the grouped traffic desk", () => {
  test("shows the three groups and the totals above them", async () => {
    renderAnalytics();
    await screen.findByText(strings.sitesAnalyticsGroupArrival);
    expect(screen.getByText(strings.sitesAnalyticsGroupPages)).toBeTruthy();
    expect(screen.getByText(strings.sitesAnalyticsGroupReading)).toBeTruthy();
    expect(within(panel(strings.sitesAnalyticsTopPages)).getByText("/prices")).toBeTruthy();
  });

  test("names the stored buckets in the reader's language", async () => {
    renderAnalytics();
    await screen.findByText(strings.sitesAnalyticsGroupReading);

    // Devices: five stored words, shown as the words a person uses.
    const devices = panel(strings.sitesAnalyticsDevices);
    expect(within(devices).getByText(strings.sitesAnalyticsDevicePhone)).toBeTruthy();
    expect(within(devices).getByText(strings.sitesAnalyticsDeviceBot)).toBeTruthy();
    expect(within(devices).queryByText("phone")).toBeNull();

    // Countries: a two-letter code is a country name, an empty one is
    // "not reported" rather than a blank row.
    const countries = panel(strings.sitesAnalyticsCountries);
    expect(within(countries).getByText("Netherlands")).toBeTruthy();
    expect(within(countries).getByText(strings.sitesAnalyticsNotReported)).toBeTruthy();

    // The empty campaign bucket is most visits on most sites, and saying
    // "No campaign" is the only reading of it that is true.
    expect(
      within(panel(strings.sitesAnalyticsCampaigns)).getByText(
        strings.sitesAnalyticsNoCampaign,
      ),
    ).toBeTruthy();

    // The referrer that no browser named is the site's own direct traffic.
    expect(
      within(panel(strings.sitesAnalyticsTopReferrers)).getByText(
        strings.sitesAnalyticsDirect,
      ),
    ).toBeTruthy();

    // "other" is the day's overflow bucket, never a destination.
    const outbound = panel(strings.sitesAnalyticsOutbound);
    expect(within(outbound).getByText(strings.sitesAnalyticsOutboundOther)).toBeTruthy();
    expect(within(outbound).getByText("shop.example")).toBeTruthy();
  });

  test("keeps the reading-time histogram in duration order, never by count", async () => {
    renderAnalytics();
    await screen.findByText(strings.sitesAnalyticsGroupReading);
    const rows = within(panel(strings.sitesAnalyticsReadTime)).getAllByRole("listitem");
    expect(rows.map((row) => row.firstElementChild?.textContent)).toEqual([
      strings.sitesAnalyticsReadUnder10s,
      strings.sitesAnalyticsRead10to30s,
      strings.sitesAnalyticsRead30to60s,
      strings.sitesAnalyticsRead1to3m,
      strings.sitesAnalyticsRead3to10m,
      strings.sitesAnalyticsReadOver10m,
    ]);
    // …and all six stay visible: a histogram truncated to its top five is a
    // different claim about the same data.
    expect(
      within(panel(strings.sitesAnalyticsReadTime)).queryByRole("button"),
    ).toBeNull();
  });

  test("every panel says how its numbers get there", async () => {
    renderAnalytics();
    await screen.findByText(strings.sitesAnalyticsGroupReading);
    // The caveat that matters most: reading times cannot be read as a bounce
    // rate, because a browser that reports nothing still counted as a visit.
    expect(
      within(panel(strings.sitesAnalyticsReadTime)).getByText(
        strings.sitesAnalyticsReadTimeNote,
      ),
    ).toBeTruthy();
    expect(
      within(panel(strings.sitesAnalyticsCountries)).getByText(
        strings.sitesAnalyticsCountriesNote,
      ),
    ).toBeTruthy();
    // And the privacy note now names the script the pages carry.
    expect(screen.getByText(strings.sitesAnalyticsPrivacyBeacon)).toBeTruthy();
  });

  test("a dimension nothing reported says so, and the rest still count", async () => {
    mocks.analytics.mockResolvedValue(
      report({ countries: [], readTime: [], outboundDomains: [] }),
    );
    renderAnalytics();
    await screen.findByText(strings.sitesAnalyticsGroupReading);

    // The honest deployment case: no edge names countries in front of this
    // site, and the screen says the other numbers are unaffected.
    expect(
      within(panel(strings.sitesAnalyticsCountries)).getByText(
        strings.sitesAnalyticsCountriesEmpty,
      ),
    ).toBeTruthy();
    expect(
      within(panel(strings.sitesAnalyticsReadTime)).getByText(
        strings.sitesAnalyticsReadTimeEmpty,
      ),
    ).toBeTruthy();
    expect(
      within(panel(strings.sitesAnalyticsOutbound)).getByText(
        strings.sitesAnalyticsOutboundEmpty,
      ),
    ).toBeTruthy();
    expect(within(panel(strings.sitesAnalyticsDevices)).getAllByRole("listitem").length).toBe(3);
  });

  test("a long ranking shows the top five and opens to all of them", async () => {
    const many = Array.from({ length: 9 }, (_, index): [string, number] => [
      `campaign-${index}`,
      100 - index,
    ]);
    mocks.analytics.mockResolvedValue(report({ campaigns: dimension(many) }));
    renderAnalytics();
    await screen.findByText(strings.sitesAnalyticsGroupArrival);

    const campaigns = panel(strings.sitesAnalyticsCampaigns);
    expect(within(campaigns).getAllByRole("listitem").length).toBe(5);
    const more = within(campaigns).getByRole("button", {
      name: strings.sitesAnalyticsShowAll(9),
    });
    expect(more.getAttribute("aria-expanded")).toBe("false");

    fireEvent.click(more);
    expect(within(campaigns).getAllByRole("listitem").length).toBe(9);
    const fewer = within(campaigns).getByRole("button", {
      name: strings.sitesAnalyticsShowTop(5),
    });
    expect(fewer.getAttribute("aria-expanded")).toBe("true");

    fireEvent.click(fewer);
    expect(within(campaigns).getAllByRole("listitem").length).toBe(5);
  });

  test("a site with no visits keeps its onboarding empty state", async () => {
    mocks.analytics.mockResolvedValue(
      report({
        totals: { visits: 0, uniqueVisitors: 0 },
        daily: [],
        topPages: [],
        topReferrers: [],
      }),
    );
    renderAnalytics();
    await screen.findByText(strings.sitesAnalyticsEmptyTitle);
    expect(screen.getByText(strings.sitesAnalyticsPrivacyTitle)).toBeTruthy();
    // No panel is drawn for a site nobody has visited: nine empty boxes are
    // not an onboarding.
    expect(screen.queryByText(strings.sitesAnalyticsGroupReading)).toBeNull();
  });

  test("a failed report is shown, never swallowed", async () => {
    mocks.analytics.mockRejectedValue(new Error("nope"));
    renderAnalytics();
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toBe(
        strings.sitesAnalyticsLoadFailed,
      );
    });
  });
});
