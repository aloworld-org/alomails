// The wiring the type checker cannot see: that the list really renders what
// the API answered, that the create form really asks the server about the
// typed address and sends exactly what was typed, and that a refusal from the
// server is shown to the user instead of swallowed.
//
// The auth layer is stubbed down to one recording `fetch`, so the REAL
// client and the real views run — only the network is fake.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { saveTextFile } from "../platform/download";
import { SitesModule } from "./SitesModule";
import type { Site, SitePage, SitePost } from "./types";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

interface Reply {
  match: (url: string, method: string) => boolean;
  status: number;
  body: unknown;
}

const calls: Call[] = [];
let replies: Reply[] = [];

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({
    url,
    method,
    body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
  });
  const index = replies.findIndex((r) => r.match(url, method));
  const answer = index === -1 ? fallback(url) : (replies.splice(index, 1)[0] as Reply);
  return new Response(
    typeof answer.body === "string" ? answer.body : JSON.stringify(answer.body),
    {
      status: answer.status,
      headers: {
        "content-type":
          typeof answer.body === "string"
            ? "text/csv; charset=utf-8"
            : "application/json",
      },
    },
  );
});

/** The lists a screen loads before anything interesting happens. */
function fallback(url: string): Reply {
  const body = url.includes("/translation-readiness")
    ? {
        defaultLocale: "en",
        totalPages: 0,
        languages: [{ locale: "en", translatedPages: 0, ready: true }],
      }
    : url.includes("/posts")
    ? { posts: [] }
    : url.includes("/pages")
      ? { pages: [] }
    : url.endsWith("/sites")
      ? { sites: [] }
      : {};
  return { match: () => true, status: 200, body };
}

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

vi.mock("../platform/download", () => ({
  saveTextFile: vi.fn(),
}));

const fakeJmap = vi.hoisted(() => ({
  driveCreateDoc: vi.fn(),
  driveTrashNode: vi.fn(),
  driveUploadBlob: vi.fn(),
}));

vi.mock("../jmap/useJmapClient", () => ({
  useJmapClient: () => fakeJmap,
}));

const ALPHA: Site = {
  id: "site-1",
  name: "Alpha Bakery",
  subdomain: "alpha",
  status: "live",
  defaultLocale: "en",
  enabledLocales: ["en"],
};
const BETA: Site = {
  id: "site-2",
  name: "Beta Atelier",
  subdomain: "beta",
  status: "draft",
  defaultLocale: "en",
  enabledLocales: ["en"],
};
const HOME: SitePage = {
  id: "page-1",
  slug: "",
  title: "Welcome",
  home: true,
  seoTitle: null,
  seoDescription: null,
};
const ABOUT: SitePage = {
  id: "page-2",
  slug: "about",
  title: "About us",
  home: false,
  seoTitle: null,
  seoDescription: null,
};
const ARTICLE: SitePost = {
  id: "post-1",
  docNodeId: "doc-1",
  slug: "summer-menu",
  title: "Our summer menu",
  excerpt: "Fresh bakes for long afternoons.",
  coverBlobId: null,
  status: "draft",
  publishedAt: null,
  createdAt: "2026-08-08T09:00:00Z",
  updatedAt: "2026-08-08T10:00:00Z",
};

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{`${location.pathname}${location.search}`}</output>;
}

/** The module as it is really mounted: at `/sites/*`, routing itself. */
function ui(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <DialogProvider>
        <LocationProbe />
        <Routes>
          <Route path="/sites/*" element={<SitesModule />} />
          {/* The real shell owns Drive; this sink lets navigation assertions
              observe the hand-off without a test-only unmatched-route warning. */}
          <Route path="/drive" element={null} />
        </Routes>
      </DialogProvider>
    </MemoryRouter>,
  );
}

/** The last write the client made, if it made one. */
function lastWrite(): Call | undefined {
  return calls.filter((c) => c.method !== "GET").at(-1);
}

beforeEach(() => {
  calls.length = 0;
  replies = [];
  fakeFetch.mockClear();
  vi.mocked(saveTextFile).mockClear();
  fakeJmap.driveCreateDoc.mockReset();
  fakeJmap.driveTrashNode.mockReset();
  fakeJmap.driveUploadBlob.mockReset();
  fakeJmap.driveTrashNode.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe("the site list", () => {
  test("renders what the API answered, with the live/draft state", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites"),
        status: 200,
        body: { sites: [ALPHA, BETA] },
      },
    ];
    ui("/sites");
    expect(await screen.findByText("Alpha Bakery")).toBeTruthy();
    expect(screen.getByText("alpha")).toBeTruthy();
    expect(screen.getByText(strings.sitesStatusLive)).toBeTruthy();
    expect(screen.getByText("Beta Atelier")).toBeTruthy();
    expect(screen.getByText(strings.sitesStatusDraft)).toBeTruthy();
  });

  test("opens a website from the whole website row", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites"),
        status: 200,
        body: { sites: [ALPHA] },
      },
    ];
    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: /Alpha Bakery/ }));
    expect(screen.getByTestId("location").textContent).toBe("/sites/site-1");
  });

  test("an empty tenant sees the empty state, not a bare table", async () => {
    ui("/sites");
    expect(await screen.findByText(strings.sitesNoSitesTitle)).toBeTruthy();
    expect(screen.getByRole("region", { name: strings.moduleSites })).toBeTruthy();
    expect(screen.getByRole("region", { name: strings.sitesNoSitesTitle })).toBeTruthy();
    expect(screen.getAllByRole("button", { name: strings.sitesNewSite })).toHaveLength(1);
  });

  test("a failure to load is shown, never swallowed", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites"),
        status: 500,
        body: {},
      },
    ];
    ui("/sites");
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByText(strings.sitesLoadFailed)).toBeTruthy();
  });
});

describe("the Base-backed collections workspace", () => {
  const baseReplies = (): Reply[] => [
    {
      match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/collections"),
      status: 200,
      body: { collections: [] },
    },
    {
      match: (url, method) => method === "GET" && url.endsWith("/spaces"),
      status: 200,
      body: { spaces: [] },
    },
  ];

  test("an account without a Base gets one visible next step", async () => {
    replies = [
      ...baseReplies(),
      {
        match: (url, method) => method === "GET" && url.includes("/drive/list?"),
        status: 200,
        body: { nodes: [] },
      },
    ];
    ui("/sites/site-1/collections");

    expect(await screen.findByText(strings.sitesCollectionNoBasesTitle)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: strings.sitesCollectionOpenDrive }));
    expect(screen.getByTestId("location").textContent).toBe("/drive");
  });

  test("connects a readable table, previews its rows, and disconnects without hidden steps", async () => {
    replies = [
      ...baseReplies(),
      {
        match: (url, method) => method === "GET" && url.includes("/drive/list?"),
        status: 200,
        body: { nodes: [{ id: "base-1", kind: "base", name: "Roasts" }] },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/drive/base/base-1"),
        status: 200,
        body: {
          nodeId: "base-1",
          tables: [{
            id: "table-1",
            name: "Seasonal roasts",
            records: [{ id: "record-1" }],
            fields: [
              { id: "title-1", name: "Name", type: "text" },
              { id: "summary-1", name: "Tasting notes", type: "text" },
              { id: "image-1", name: "Photo", type: "attachment" },
              { id: "date-1", name: "Published", type: "date" },
              { id: "ignored-1", name: "Score", type: "number" },
            ],
          }],
        },
      },
    ];
    ui("/sites/site-1/collections");

    expect(await screen.findByDisplayValue("Seasonal roasts")).toBeTruthy();
    expect(screen.getByText(strings.sitesCollectionMapping)).toBeTruthy();
    expect(screen.getByRole("option", { name: "Photo" })).toBeTruthy();
    expect(screen.getByRole("option", { name: "Published" })).toBeTruthy();
    expect(screen.queryByRole("option", { name: "Score" })).toBeNull();

    const stored = {
      id: "collection-1",
      name: "Seasonal roasts",
      baseNodeId: "base-1",
      baseTableId: "table-1",
      mapping: {
        title: "title-1",
        slug: null,
        summary: null,
        body: null,
        image: null,
        link: null,
        publishedAt: null,
      },
      createdAt: "2026-08-11T09:00:00Z",
      updatedAt: "2026-08-11T09:00:00Z",
    };
    replies.push(
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites/site-1/collections"),
        status: 200,
        body: stored,
      },
      {
        match: (url, method) =>
          method === "GET" && url.endsWith("/sites/site-1/collections/collection-1/preview"),
        status: 200,
        body: {
          id: "collection-1",
          name: "Seasonal roasts",
          items: [{
            title: "Harbour Blend",
            slug: "harbour-blend",
            summary: "Chocolate and red apple",
            body: null,
            imageBlobId: null,
            link: null,
            publishedAt: null,
          }],
        },
      },
    );
    fireEvent.click(screen.getByRole("button", { name: strings.sitesCollectionSave }));

    await waitFor(() => {
      const write = lastWrite();
      expect(write?.url.endsWith("/sites/site-1/collections")).toBe(true);
      expect(write?.method).toBe("POST");
      expect(write?.body).toEqual({
        name: "Seasonal roasts",
        baseNodeId: "base-1",
        baseTableId: "table-1",
        mapping: stored.mapping,
      });
    });
    expect(await screen.findByText("Harbour Blend")).toBeTruthy();
    expect(screen.getByText("Chocolate and red apple")).toBeTruthy();

    replies.push({
      match: (url, method) =>
        method === "DELETE" && url.endsWith("/sites/site-1/collections/collection-1"),
      status: 200,
      body: { status: "ok" },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesCollectionDisconnect }));
    expect(screen.getByText(strings.sitesCollectionDisconnectHint)).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesCollectionDisconnectConfirm }),
    );
    await waitFor(() => expect(lastWrite()?.method).toBe("DELETE"));
  });
});

describe("the contact submissions inbox", () => {
  const detail = { ...ALPHA, publish: null, theme: {} };
  const submission = {
    id: "submission-1",
    formId: "form-1",
    formName: "Contact us",
    senderName: "Ada Lovelace",
    senderEmail: "ada@example.test",
    message: "Could you call me tomorrow?",
    handled: false,
    receivedAt: "2026-08-08T09:30:00Z",
  };

  test("reads a visitor message and marks it handled without leaving the inbox", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: detail,
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/submissions"),
        status: 200,
        body: { submissions: [submission] },
      },
      {
        match: (url, method) =>
          method === "PUT" &&
          url.endsWith("/sites/site-1/forms/form-1/submissions/submission-1"),
        status: 200,
        body: { status: "ok" },
      },
    ];

    ui("/sites/site-1/submissions");
    expect((await screen.findAllByText("Ada Lovelace")).length).toBe(2);
    expect(screen.getByText("Could you call me tomorrow?")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: strings.sitesMarkHandled }));

    expect(await screen.findByText(strings.sitesHandled)).toBeTruthy();
    expect(lastWrite()).toMatchObject({ method: "PUT", body: { handled: true } });
    expect(screen.getByRole("button", { name: strings.sitesReopenSubmission })).toBeTruthy();
  });

  test("an empty inbox teaches the next step", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: detail,
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/submissions"),
        status: 200,
        body: { submissions: [] },
      },
    ];
    ui("/sites/site-1/submissions");
    expect(await screen.findByText(strings.sitesNoSubmissionsTitle)).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.sitesOpenPages })).toBeTruthy();
  });

  test("exports the visible inbox in one click", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: detail,
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/submissions"),
        status: 200,
        body: { submissions: [submission] },
      },
      {
        match: (url, method) =>
          method === "GET" && url.endsWith("/sites/site-1/submissions.csv"),
        status: 200,
        body: "receivedAt,form\r\n2026-08-08T09:30:00Z,Contact us\r\n",
      },
    ];

    ui("/sites/site-1/submissions");
    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesExportSubmissions }),
    );

    await waitFor(() => expect(saveTextFile).toHaveBeenCalledTimes(1));
    expect(saveTextFile).toHaveBeenCalledWith(
      expect.stringContaining("Contact us"),
      "submissions-alpha.csv",
      "text/csv;charset=utf-8",
    );
  });
});

describe("the site analytics desk", () => {
  const detail = { ...ALPHA, publish: null, theme: {} };

  test("shows actionable traffic and the no-cookie promise", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: detail,
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/config"),
        status: 200,
        body: { domain: "sites.test" },
      },
      {
        match: (url, method) =>
          method === "GET" && url.endsWith("/sites/site-1/analytics?days=30"),
        status: 200,
        body: {
          from: "2026-07-11",
          to: "2026-08-09",
          totals: { visits: 42, uniqueVisitors: 17 },
          daily: [
            { date: "2026-08-08", visits: 12, uniqueVisitors: 7 },
            { date: "2026-08-09", visits: 30, uniqueVisitors: 10 },
          ],
          topPages: [{ path: "/menu", visits: 24, uniqueVisitors: 12 }],
          topReferrers: [{ domain: "", visits: 20, uniqueVisitors: 9 }],
        },
      },
    ];

    ui("/sites/site-1/analytics");
    expect(await screen.findByText(strings.sitesAnalyticsPrivacyTitle)).toBeTruthy();
    expect(screen.getByText("42")).toBeTruthy();
    expect(screen.getByText("17")).toBeTruthy();
    expect(screen.getByText("/menu")).toBeTruthy();
    expect(screen.getByText(strings.sitesAnalyticsDirect)).toBeTruthy();
    expect(screen.getByRole("list", { name: strings.sitesAnalyticsChartLabel })).toBeTruthy();
  });

  test("an empty report teaches the one next step", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: detail,
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/config"),
        status: 200,
        body: { domain: "sites.test" },
      },
      {
        match: (url, method) => method === "GET" && url.includes("/analytics?days=30"),
        status: 200,
        body: {
          from: "2026-07-11",
          to: "2026-08-09",
          totals: { visits: 0, uniqueVisitors: 0 },
          daily: [],
          topPages: [],
          topReferrers: [],
        },
      },
    ];

    ui("/sites/site-1/analytics");
    expect(await screen.findByText(strings.sitesAnalyticsEmptyTitle)).toBeTruthy();
    expect(screen.getByText(strings.sitesAnalyticsEmptyBody)).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.sitesAnalyticsOpenSite })).toBeTruthy();
  });
});

describe("creating a site", () => {
  function chooseTemplatePath() {
    fireEvent.click(screen.getByRole("button", { name: strings.sitesTemplateChoice }));
  }

  test("a description generates a private draft and opens its Home page", async () => {
    replies = [
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites/generate"),
        status: 200,
        body: {
          site: { ...BETA, id: "site-generated", name: "Acme Bakery", subdomain: "acme-bakery" },
          pages: [{ ...HOME, id: "page-generated", sections: { schema_version: 1, sections: [] } }],
        },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-generated/pages/page-generated"),
        status: 200,
        body: { ...HOME, id: "page-generated", sections: { schema_version: 1, sections: [] } },
      },
    ];

    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));
    fireEvent.change(screen.getByLabelText(strings.sitesBusinessDescription), {
      target: { value: "A neighborhood bakery for local families" },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesGenerateSite }));

    await waitFor(() =>
      expect(screen.getByTestId("location").textContent).toBe(
        "/sites/site-generated/pages/page-generated",
      ),
    );
    expect(await screen.findByText(strings.sitesNoSectionsTitle)).toBeTruthy();
    // One click from adding a hero (S1.30c): the empty page's CTA opens the
    // section palette, and an unseeded hero tile opens its prop form.
    fireEvent.click(screen.getByRole("button", { name: strings.sitesAddSection }));
    const heroTile = document.querySelector<HTMLElement>('[data-palette-tile="hero"]');
    if (heroTile === null) throw new Error("no hero tile in the palette");
    fireEvent.click(heroTile);
    expect(
      screen.getByRole("dialog", {
        name: strings.sitesAddSectionTitle(strings.sitesSectionHero),
      }),
    ).toBeTruthy();
    expect(calls.find((call) => call.url.endsWith("/sites/generate"))?.body).toEqual({
      description: "A neighborhood bakery for local families",
    });
  });

  test("an unconfigured workspace reveals the complete manual template path", async () => {
    replies = [
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites/generate"),
        status: 503,
        body: { reason: "unconfigured", detail: "AI is not configured for this tenant" },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/templates"),
        status: 200,
        body: { templates: [] },
      },
      {
        match: (url, method) => method === "GET" && url.includes("/sites/subdomain-check"),
        status: 200,
        body: { subdomain: "manual-studio", available: true },
      },
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites"),
        status: 200,
        body: { ...BETA, id: "site-manual", name: "Manual Studio", subdomain: "manual-studio" },
      },
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites/site-manual/pages"),
        status: 200,
        body: { ...HOME, id: "page-manual", title: strings.sitesHomePageTitle },
      },
      {
        match: (url, method) =>
          method === "GET" && url.endsWith("/sites/site-manual/pages/page-manual"),
        status: 200,
        body: {
          ...HOME,
          id: "page-manual",
          title: strings.sitesHomePageTitle,
          sections: { schema_version: 1, sections: [] },
        },
      },
    ];

    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));
    fireEvent.change(screen.getByLabelText(strings.sitesBusinessDescription), {
      target: { value: "A useful business website" },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesGenerateSite }));

    expect(await screen.findByText(strings.sitesGenerationUnavailable)).toBeTruthy();
    expect(screen.getByLabelText(strings.sitesFieldName)).toBeTruthy();
    expect(
      await screen.findByRole("radio", { name: new RegExp(strings.sitesBlankTemplate, "i") }),
    ).toBeTruthy();
    fireEvent.change(screen.getByLabelText(strings.sitesFieldName), {
      target: { value: "Manual Studio" },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesFieldSubdomain), {
      target: { value: "manual-studio" },
    });
    await waitFor(() => expect(screen.getByText(strings.sitesAddressAvailable)).toBeTruthy(), {
      timeout: 3000,
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesCreateSite }));
    await waitFor(() =>
      expect(screen.getByTestId("location").textContent).toBe(
        "/sites/site-manual/pages/page-manual",
      ),
    );
  });

  test("the typed address is checked live against the server", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/config"),
        status: 200,
        body: { domain: "alosites.com" },
      },
    ];
    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));
    chooseTemplatePath();
    const dialog = screen.getByRole("dialog");
    expect(dialog).toBeTruthy();

    replies = [
      {
        match: (url, method) => method === "GET" && url.includes("/sites/subdomain-check"),
        status: 200,
        body: { subdomain: "acme", available: true },
      },
    ];
    fireEvent.change(screen.getByLabelText(strings.sitesFieldSubdomain), {
      target: { value: "acme" },
    });
    await waitFor(
      () => expect(screen.getByText(strings.sitesAddressAvailable)).toBeTruthy(),
      { timeout: 3000 },
    );
    expect(screen.getByText("acme.alosites.com")).toBeTruthy();
    const check = calls.find((c) => c.url.includes("/sites/subdomain-check"));
    expect(check?.url).toContain("subdomain=acme");
  });

  test("the site name suggests an editable full address and explains a disabled Create", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/config"),
        status: 200,
        body: { domain: "alosites.com" },
      },
      {
        match: (url, method) => method === "GET" && url.includes("/sites/subdomain-check"),
        status: 200,
        body: { subdomain: "axon-studio", available: true },
      },
    ];
    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));
    chooseTemplatePath();

    expect(screen.getByText(strings.sitesNameRequired)).toBeTruthy();
    fireEvent.change(screen.getByLabelText(strings.sitesFieldName), {
      target: { value: "Axón Studio" },
    });

    expect(screen.getByLabelText(strings.sitesFieldSubdomain)).toHaveProperty(
      "value",
      "axon-studio",
    );
    expect(await screen.findByText("axon-studio.alosites.com")).toBeTruthy();
    expect(screen.queryByText(strings.sitesNameRequired)).toBeNull();
    await waitFor(() => expect(screen.getByText(strings.sitesAddressAvailable)).toBeTruthy(), {
      timeout: 3000,
    });

    fireEvent.change(screen.getByLabelText(strings.sitesFieldSubdomain), {
      target: { value: "" },
    });
    expect(screen.getByText(strings.sitesAddressRequired)).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.sitesCreateSite })).toHaveProperty(
      "disabled",
      true,
    );
  });

  test("a taken address, and a rule the server names, are both shown", async () => {
    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));
    chooseTemplatePath();

    replies = [
      {
        match: (url, method) => method === "GET" && url.includes("/sites/subdomain-check"),
        status: 200,
        body: { subdomain: "taken", available: false },
      },
    ];
    const address = screen.getByLabelText(strings.sitesFieldSubdomain);
    fireEvent.change(address, { target: { value: "taken" } });
    await waitFor(() => expect(screen.getByText(strings.sitesAddressTaken)).toBeTruthy(), {
      timeout: 3000,
    });
    expect(screen.getByRole("button", { name: strings.sitesCreateSite })).toHaveProperty(
      "disabled",
      true,
    );

    replies = [
      {
        match: (url, method) => method === "GET" && url.includes("/sites/subdomain-check"),
        status: 422,
        body: { detail: "subdomain is reserved" },
      },
    ];
    fireEvent.change(address, { target: { value: "mail" } });
    await waitFor(() => expect(screen.getByText("subdomain is reserved")).toBeTruthy(), {
      timeout: 3000,
    });
    expect(screen.getByRole("button", { name: strings.sitesCreateSite })).toHaveProperty(
      "disabled",
      true,
    );
  });

  test("Create only enables for the address whose availability was confirmed", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.includes("subdomain=first-address"),
        status: 200,
        body: { subdomain: "first-address", available: true },
      },
      {
        match: (url, method) => method === "GET" && url.includes("subdomain=second-address"),
        status: 200,
        body: { subdomain: "second-address", available: true },
      },
    ];
    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));
    chooseTemplatePath();
    fireEvent.change(screen.getByLabelText(strings.sitesFieldName), {
      target: { value: "Confirmed Studio" },
    });
    const address = screen.getByLabelText(strings.sitesFieldSubdomain);
    fireEvent.change(address, { target: { value: "first-address" } });

    await waitFor(() => expect(screen.getByText(strings.sitesAddressAvailable)).toBeTruthy(), {
      timeout: 3000,
    });
    expect(screen.getByRole("button", { name: strings.sitesCreateSite })).toHaveProperty(
      "disabled",
      false,
    );

    fireEvent.change(address, { target: { value: "second-address" } });
    expect(screen.getByRole("button", { name: strings.sitesCreateSite })).toHaveProperty(
      "disabled",
      true,
    );
    await waitFor(() => expect(screen.getByText(strings.sitesAddressAvailable)).toBeTruthy(), {
      timeout: 3000,
    });
    expect(screen.getByRole("button", { name: strings.sitesCreateSite })).toHaveProperty(
      "disabled",
      false,
    );
  });

  test("a pasted full address is normalized before check and create", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/config"),
        status: 200,
        body: { domain: "alosites.com" },
      },
      {
        match: (url, method) => method === "GET" && url.includes("/sites/subdomain-check"),
        status: 200,
        body: { subdomain: "acme", available: true },
      },
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites"),
        status: 200,
        body: { ...ALPHA, id: "site-9", name: "Acme", subdomain: "acme", status: "draft" },
      },
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites/site-9/pages"),
        status: 200,
        body: { ...HOME, id: "page-9", title: strings.sitesHomePageTitle },
      },
      {
        match: (url, method) =>
          method === "GET" && url.endsWith("/sites/site-9/pages/page-9"),
        status: 200,
        body: {
          ...HOME,
          id: "page-9",
          title: strings.sitesHomePageTitle,
          sections: { schema_version: 1, sections: [] },
        },
      },
    ];
    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));
    chooseTemplatePath();
    fireEvent.change(screen.getByLabelText(strings.sitesFieldName), {
      target: { value: "Acme" },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesFieldSubdomain), {
      target: { value: "https://acme.alosites.com/" },
    });
    await waitFor(() =>
      expect(screen.getByLabelText(strings.sitesFieldSubdomain)).toHaveProperty("value", "acme"),
    );
    await waitFor(
      () => expect(calls.some((call) => call.url.includes("subdomain=acme"))).toBe(true),
      { timeout: 3000 },
    );
    fireEvent.click(screen.getByRole("button", { name: strings.sitesCreateSite }));

    // The write carries the label behind the displayed complete address…
    await waitFor(() => expect(calls.some((call) => call.url.endsWith("/sites/site-9/pages"))).toBe(true));
    expect(
      calls.find((call) => call.method === "POST" && call.url.endsWith("/sites")),
    ).toMatchObject({
      method: "POST",
      body: { name: "Acme", subdomain: "acme" },
    });
    // …and the module navigated directly into its new Home page.
    await waitFor(() =>
      expect(screen.getByTestId("location").textContent).toContain("/sites/site-9/pages/"),
    );
  });

  test("the server's refusal is shown in the dialog, which stays open", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.includes("/sites/subdomain-check"),
        status: 200,
        body: { subdomain: "acme", available: true },
      },
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites"),
        status: 422,
        body: { detail: "subdomain is already taken" },
      },
    ];
    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));
    chooseTemplatePath();
    fireEvent.change(screen.getByLabelText(strings.sitesFieldName), {
      target: { value: "Acme" },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesFieldSubdomain), {
      target: { value: "acme" },
    });
    await waitFor(() => expect(screen.getByText(strings.sitesAddressAvailable)).toBeTruthy(), {
      timeout: 3000,
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesCreateSite }));
    expect(await screen.findByText("subdomain is already taken")).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});

describe("publishing a site", () => {
  const config: Reply = {
    match: (url, method) => method === "GET" && url.endsWith("/sites/config"),
    status: 200,
    body: { domain: "alosites.com" },
  };

  test("a draft offers Publish with the goes-live address; publishing flips it live", async () => {
    replies = [
      config,
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-2"),
        status: 200,
        body: { ...BETA, publish: null },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-2/pages"),
        status: 200,
        body: { pages: [HOME] },
      },
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites/site-2/publish"),
        status: 200,
        body: { publishId: "pub-9", status: "live" },
      },
      // The reload after publishing.
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-2"),
        status: 200,
        body: {
          ...BETA,
          status: "live",
          publish: { id: "pub-9", publishedAt: "2026-08-07T12:00:00Z" },
        },
      },
    ];
    ui("/sites/site-2");
    fireEvent.click(
      await screen.findByRole("tab", { name: strings.sitesPublishing }),
    );
    // The copy names the real address the site will serve at.
    expect(await screen.findByText(strings.sitesGoesLiveAt("beta.alosites.com"))).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: strings.sitesPublish }));
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()?.method).toBe("POST");
    expect(lastWrite()?.url.endsWith("/sites/site-2/publish")).toBe(true);
    // The reload shows the live state, the address now a real link.
    await waitFor(() =>
      expect(screen.getAllByText(strings.sitesStatusLive).length).toBeGreaterThan(0),
    );
    const link = screen.getByRole("link", { name: "beta.alosites.com" }) as HTMLAnchorElement;
    expect(link.href).toBe("https://beta.alosites.com/");
  });

  test("a refused publish shows the server's own sentence and stays draft", async () => {
    replies = [
      config,
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-2"),
        status: 200,
        body: { ...BETA, publish: null },
      },
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites/site-2/publish"),
        status: 422,
        body: { detail: "site has no home page" },
      },
    ];
    ui("/sites/site-2");
    fireEvent.click(
      await screen.findByRole("tab", { name: strings.sitesPublishing }),
    );
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesPublish }));
    expect(await screen.findByText("site has no home page")).toBeTruthy();
    expect(screen.getAllByText(strings.sitesStatusDraft).length).toBeGreaterThan(0);
  });

  test("taking a live site offline needs the second click", async () => {
    replies = [
      config,
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: { ...ALPHA, publish: { id: "pub-1", publishedAt: "2026-08-07T10:00:00Z" } },
      },
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites/site-1/unpublish"),
        status: 200,
        body: { status: "draft" },
      },
      // The reload after unpublishing.
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: { ...ALPHA, status: "draft", publish: null },
      },
    ];
    ui("/sites/site-1");
    fireEvent.click(
      await screen.findByRole("tab", { name: strings.sitesPublishing }),
    );
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesUnpublish }));
    // Armed, not fired: nothing was written, the button now asks to confirm.
    expect(lastWrite()).toBeUndefined();
    fireEvent.click(screen.getByRole("button", { name: strings.sitesConfirmUnpublish }));
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()?.url.endsWith("/sites/site-1/unpublish")).toBe(true);
    await waitFor(() =>
      expect(screen.getAllByText(strings.sitesStatusDraft).length).toBeGreaterThan(0),
    );
  });

  test("the create form previews the full address once the domain is known", async () => {
    replies = [config];
    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));
    fireEvent.click(screen.getByRole("button", { name: strings.sitesTemplateChoice }));
    fireEvent.change(screen.getByLabelText(strings.sitesFieldSubdomain), {
      target: { value: "Acme" },
    });
    // The live status composes the typed label (lowercased) with the domain.
    expect(await screen.findByText("acme.alosites.com")).toBeTruthy();
  });
});

describe("one site", () => {
  test("shows the site and its pages in order, the home page marked", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: { ...ALPHA, publish: { id: "pub-1", publishedAt: "2026-08-07T10:00:00Z" } },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/pages"),
        status: 200,
        body: { pages: [HOME, ABOUT] },
      },
    ];
    ui("/sites/site-1?section=pages");
    expect(await screen.findByText("Alpha Bakery")).toBeTruthy();
    expect(screen.getAllByText(strings.sitesStatusLive).length).toBeGreaterThan(0);
    expect(screen.getByText("Welcome")).toBeTruthy();
    expect(screen.getByText(strings.sitesHomeBadge)).toBeTruthy();
    expect(screen.getByText("About us")).toBeTruthy();
    expect(screen.getByText("/about")).toBeTruthy();
  });

  test("keeps pages, publishing, and languages in separate workspaces", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: { ...ALPHA, publish: null },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/pages"),
        status: 200,
        body: { pages: [HOME] },
      },
    ];
    ui("/sites/site-1?section=pages");

    expect(
      await screen.findByRole("heading", { name: strings.sitesPages }),
    ).toBeTruthy();
    expect(screen.getAllByText(strings.sitesStatusLive).length).toBeGreaterThan(0);
    expect(screen.queryByLabelText(strings.sitesLanguagesHint)).toBeNull();

    fireEvent.click(
      screen.getByRole("tab", { name: strings.sitesPublishing }),
    );
    expect(screen.getAllByText(strings.sitesStatusLive).length).toBeGreaterThan(0);
    expect(
      screen.queryByRole("heading", { name: strings.sitesPages }),
    ).toBeNull();

    fireEvent.click(
      screen.getByRole("tab", { name: strings.sitesLanguages }),
    );
    expect(screen.getByLabelText(strings.sitesLanguagesHint)).toBeTruthy();
    expect(screen.getAllByText(strings.sitesStatusLive).length).toBeGreaterThan(0);
  });

  test("a page behind a password is marked in the list, in one read", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: { ...ALPHA, publish: { id: "pub-1", publishedAt: "2026-08-07T10:00:00Z" } },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/pages"),
        status: 200,
        body: { pages: [HOME, ABOUT] },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/passwords"),
        status: 200,
        body: {
          pages: [
            {
              protected: true,
              pageId: ABOUT.id,
              createdAt: "2026-08-12T09:30:00Z",
              updatedAt: "2026-08-12T09:30:00Z",
            },
          ],
        },
      },
    ];
    ui("/sites/site-1?section=pages");

    const marked = await screen.findByText(strings.sitesPagePasswordBadge);
    // The badge belongs to the protected page's row and to no other.
    const row = marked.closest("tr");
    expect(within(row as HTMLElement).getByText("About us")).toBeTruthy();
    expect(screen.getAllByText(strings.sitesPagePasswordBadge)).toHaveLength(1);
    // One read for the whole list, not one per page.
    expect(
      calls.filter((call) => call.url.endsWith("/sites/site-1/passwords")),
    ).toHaveLength(1);
  });

  test("duplicating a page uses the server page-copy API", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: { ...ALPHA, publish: { id: "pub-1", publishedAt: "2026-08-07T10:00:00Z" } },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/pages"),
        status: 200,
        body: { pages: [HOME, ABOUT] },
      },
      {
        match: (url, method) =>
          method === "POST" && url.endsWith("/sites/site-1/pages/page-2/duplicate"),
        status: 200,
        body: {
          ...ABOUT,
          id: "page-copy",
          slug: "about-copy",
          title: "About us copy",
          sections: { schema_version: 1, sections: [] },
        },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: { ...ALPHA, publish: { id: "pub-1", publishedAt: "2026-08-07T10:00:00Z" } },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/pages"),
        status: 200,
        body: {
          pages: [
            HOME,
            ABOUT,
            { ...ABOUT, id: "page-copy", slug: "about-copy", title: "About us copy" },
          ],
        },
      },
    ];
    ui("/sites/site-1?section=pages");

    const pageName = await screen.findByText("About us");
    const row = pageName.closest("tr") as HTMLElement;
    fireEvent.click(within(row).getByLabelText(strings.sitesPageActions));
    fireEvent.click(within(row).getByRole("button", { name: strings.sitesDuplicatePage }));

    await waitFor(() =>
      expect(lastWrite()?.url.endsWith("/sites/site-1/pages/page-2/duplicate")).toBe(true),
    );
    expect(await screen.findByText("About us copy")).toBeTruthy();
  });

  test("shows translation readiness and adds a visitor language on the surface", async () => {
    const multilingual = {
      ...ALPHA,
      defaultLocale: "en",
      enabledLocales: ["en", "fr"],
      publish: null,
      theme: {},
    };
    const readiness = {
      defaultLocale: "en",
      totalPages: 2,
      languages: [
        { locale: "en", translatedPages: 2, ready: true },
        { locale: "fr", translatedPages: 1, ready: false },
      ],
    };
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: multilingual,
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/pages"),
        status: 200,
        body: { pages: [HOME, ABOUT] },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/translation-readiness"),
        status: 200,
        body: readiness,
      },
      {
        match: (url, method) => method === "PUT" && url.endsWith("/sites/site-1"),
        status: 200,
        body: { status: "ok" },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: { ...multilingual, enabledLocales: ["en", "fr", "nl"] },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/pages"),
        status: 200,
        body: { pages: [HOME, ABOUT] },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/translation-readiness"),
        status: 200,
        body: {
          ...readiness,
          languages: [
            ...readiness.languages,
            { locale: "nl", translatedPages: 0, ready: false },
          ],
        },
      },
    ];
    ui("/sites/site-1");

    fireEvent.click(
      await screen.findByRole("tab", { name: strings.sitesLanguages }),
    );
    expect(await screen.findByText(strings.sitesTranslationProgress(1, 2))).toBeTruthy();
    fireEvent.change(screen.getByPlaceholderText(strings.sitesLanguagePlaceholder), {
      target: { value: "nl" },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesAddLanguageAction }));

    await waitFor(() => expect(calls.some((call) =>
      call.method === "PUT" && call.url.endsWith("/sites/site-1") &&
      JSON.stringify(call.body) === JSON.stringify({
        defaultLocale: "en",
        enabledLocales: ["en", "fr", "nl"],
      })
    )).toBe(true));
    expect(await screen.findByText("NL")).toBeTruthy();
  });

  test("reviews a complete page and post translation before approval", async () => {
    const multilingual = {
      ...ALPHA,
      defaultLocale: "en",
      enabledLocales: ["en", "fr"],
      publish: null,
      theme: {},
    };
    const readiness = {
      defaultLocale: "en",
      totalPages: 1,
      languages: [
        { locale: "en", translatedPages: 1, ready: true },
        { locale: "fr", translatedPages: 0, ready: false },
      ],
    };
    const proposal = {
      schema_version: 1,
      source_locale: "en",
      target_locale: "fr",
      pages: [
        {
          before: {
            id: "page-1",
            title: "Welcome",
            slug: "",
            seo_title: null,
            seo_description: null,
            sections: { schema_version: 1, sections: [] },
          },
          after: {
            id: "page-1",
            title: "Bienvenue",
            slug: "",
            seo_title: null,
            seo_description: null,
            sections: { schema_version: 1, sections: [] },
          },
        },
      ],
      posts: [
        {
          before: { id: "post-1", title: "News", slug: "news", excerpt: "Latest" },
          after: {
            id: "post-1",
            title: "Actualités",
            slug: "actualites",
            excerpt: "Dernières nouvelles",
          },
        },
      ],
    };
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: multilingual,
      },
      {
        match: (url, method) =>
          method === "GET" && url.endsWith("/sites/site-1/pages"),
        status: 200,
        body: { pages: [HOME] },
      },
      {
        match: (url, method) =>
          method === "GET" && url.endsWith("/translation-readiness"),
        status: 200,
        body: readiness,
      },
      {
        match: (url, method) =>
          method === "POST" && url.endsWith("/translation-proposals"),
        status: 200,
        body: { proposal },
      },
      {
        match: (url, method) =>
          method === "PUT" && url.endsWith("/translation-proposals"),
        status: 200,
        body: { applied: true },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: multilingual,
      },
      {
        match: (url, method) =>
          method === "GET" && url.endsWith("/sites/site-1/pages"),
        status: 200,
        body: { pages: [HOME] },
      },
      {
        match: (url, method) =>
          method === "GET" && url.endsWith("/translation-readiness"),
        status: 200,
        body: {
          ...readiness,
          languages: readiness.languages.map((language) => ({
            ...language,
            translatedPages: 1,
            ready: true,
          })),
        },
      },
    ];
    ui("/sites/site-1");

    fireEvent.click(
      await screen.findByRole("tab", { name: strings.sitesLanguages }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesTranslateWholeSite }),
    );
    expect(await screen.findByText("Bienvenue")).toBeTruthy();
    expect(screen.getByText("Actualités")).toBeTruthy();
    expect(
      calls.some(
        (call) =>
          call.method === "POST" &&
          JSON.stringify(call.body) ===
            JSON.stringify({ sourceLocale: "en", targetLocale: "fr" }),
      ),
    ).toBe(true);

    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesWholeTranslationApprove }),
    );
    await waitFor(() =>
      expect(
        calls.some(
          (call) =>
            call.method === "PUT" &&
            call.url.endsWith("/translation-proposals") &&
            JSON.stringify(call.body) === JSON.stringify({ proposal }),
        ),
      ).toBe(true),
    );
    await waitFor(() => expect(screen.queryByText("Bienvenue")).toBeNull());
  });

  test("a foreign or stale id reads as not-found with the way back", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/other"),
        status: 404,
        body: { detail: "no such site" },
      },
    ];
    ui("/sites/other");
    expect(await screen.findByText("no such site")).toBeTruthy();
    expect(screen.getByText(strings.sitesBack)).toBeTruthy();
  });

  test("adding a page sends title, path and the home flag", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-2"),
        status: 200,
        body: { ...BETA, publish: null },
      },
      // The initial load: no pages yet (replies are consumed in order, so the
      // reload below answers the second pages request, not this one).
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-2/pages"),
        status: 200,
        body: { pages: [] },
      },
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites/site-2/pages"),
        status: 200,
        body: { id: "page-9", slug: "", title: "Welcome", home: true },
      },
      // The reload after creating.
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-2"),
        status: 200,
        body: { ...BETA, publish: null },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-2/pages"),
        status: 200,
        body: { pages: [{ id: "page-9", slug: "", title: "Welcome", home: true }] },
      },
    ];
    ui("/sites/site-2?section=pages");
    // The site has no pages, so the empty state's CTA opens the dialog and the
    // home flag defaults to on — the first page IS the home page.
    fireEvent.click((await screen.findAllByRole("button", { name: strings.sitesNewPage }))[0]!);
    const homeToggle = screen.getByLabelText(strings.sitesFieldHome) as HTMLInputElement;
    expect(homeToggle.checked).toBe(true);
    fireEvent.change(screen.getByLabelText(strings.sitesFieldPageTitle), {
      target: { value: "Welcome" },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesCreatePage }));

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()).toMatchObject({
      method: "POST",
      body: { title: "Welcome", slug: "", home: true },
    });
    // The list reloaded with the created page.
    expect(await screen.findByText("Welcome")).toBeTruthy();
  });
});

describe("blog authoring", () => {
  const detail = { ...ALPHA, publish: null, theme: {} };

  test("lists linked articles and opens the source document in one click", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: detail,
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/posts"),
        status: 200,
        body: { posts: [ARTICLE] },
      },
    ];

    ui("/sites/site-1/posts");
    expect(await screen.findByText("Our summer menu")).toBeTruthy();
    expect(screen.getByText("Fresh bakes for long afternoons.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: strings.sitesEditInDocs }));
    expect(screen.getByTestId("location").textContent).toBe("/drive?open=doc-1");
  });

  test("creates and links an alo Doc before opening it", async () => {
    fakeJmap.driveCreateDoc.mockResolvedValueOnce("doc-9");
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: detail,
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/posts"),
        status: 200,
        body: { posts: [] },
      },
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites/site-1/posts"),
        status: 200,
        body: { ...ARTICLE, id: "post-9", docNodeId: "doc-9", title: strings.sitesUntitledArticle },
      },
    ];

    ui("/sites/site-1/posts");
    fireEvent.click(
      (await screen.findAllByRole("button", { name: strings.sitesWriteInDocs }))[0]!,
    );

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(fakeJmap.driveCreateDoc).toHaveBeenCalledWith(
      null,
      null,
      strings.sitesUntitledArticle,
    );
    expect(lastWrite()).toMatchObject({
      method: "POST",
      body: {
        docNodeId: "doc-9",
        slug: "draft-doc-9",
        title: strings.sitesUntitledArticle,
        excerpt: "",
      },
    });
    expect(screen.getByTestId("location").textContent).toBe("/drive?open=doc-9");
  });

  test("shows the server reason and trashes a new blank doc when linking is refused", async () => {
    fakeJmap.driveCreateDoc.mockResolvedValueOnce("doc-9");
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: detail,
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/posts"),
        status: 200,
        body: { posts: [] },
      },
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites/site-1/posts"),
        status: 422,
        body: { detail: "the document is not available in this workspace" },
      },
    ];

    ui("/sites/site-1/posts");
    fireEvent.click(
      (await screen.findAllByRole("button", { name: strings.sitesWriteInDocs }))[0]!,
    );

    expect(
      await screen.findByText("the document is not available in this workspace"),
    ).toBeTruthy();
    expect(fakeJmap.driveTrashNode).toHaveBeenCalledWith("doc-9");
    expect(screen.getByTestId("location").textContent).toBe("/sites/site-1/posts");
  });

  test("publishes a draft with its public details and uploaded cover", async () => {
    fakeJmap.driveUploadBlob.mockResolvedValueOnce({
      id: "cover-node",
      blobId: "cover-blob",
      size: 4,
    });
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: detail,
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/posts"),
        status: 200,
        body: { posts: [ARTICLE] },
      },
      {
        match: (url, method) => method === "PUT" && url.endsWith("/sites/site-1/posts/post-1"),
        status: 200,
        body: { status: "ok" },
      },
      {
        match: (url, method) =>
          method === "POST" && url.endsWith("/sites/site-1/posts/post-1/publish"),
        status: 200,
        body: { status: "ok" },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: detail,
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/posts"),
        status: 200,
        body: {
          posts: [
            {
              ...ARTICLE,
              title: "Summer at Alpha",
              slug: "summer-at-alpha",
              excerpt: "The season's newest bakes.",
              coverBlobId: "cover-blob",
              status: "published",
              publishedAt: "2026-08-09T10:00:00Z",
            },
          ],
        },
      },
    ];

    ui("/sites/site-1/posts");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesPublishArticle }));
    const dialog = screen.getByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText(strings.sitesFieldPostTitle), {
      target: { value: "Summer at Alpha" },
    });
    fireEvent.change(within(dialog).getByLabelText(strings.sitesFieldPostSlug), {
      target: { value: "summer-at-alpha" },
    });
    fireEvent.change(within(dialog).getByLabelText(strings.sitesFieldPostExcerpt), {
      target: { value: "The season's newest bakes." },
    });
    const cover = dialog.querySelector('input[type="file"]') as HTMLInputElement;
    const file = new File(["hero"], "summer.png", { type: "image/png" });
    fireEvent.change(cover, { target: { files: [file] } });
    expect(await within(dialog).findByText(strings.sitesPostCoverAdded)).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: strings.sitesPublishArticle }));

    await waitFor(() =>
      expect(
        calls.some(
          (call) => call.method === "POST" && call.url.endsWith("/posts/post-1/publish"),
        ),
      ).toBe(true),
    );
    expect(fakeJmap.driveUploadBlob).toHaveBeenCalledWith(null, null, file);
    expect(
      calls.find(
        (call) => call.method === "PUT" && call.url.endsWith("/sites/site-1/posts/post-1"),
      ),
    ).toMatchObject({
      body: {
        title: "Summer at Alpha",
        slug: "summer-at-alpha",
        excerpt: "The season's newest bakes.",
        coverBlobId: "cover-blob",
      },
    });
    expect(await screen.findByText(strings.sitesPostStatusPublished)).toBeTruthy();
  });

  test("keeps a refused publish open with the server's reason", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: detail,
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/posts"),
        status: 200,
        body: { posts: [ARTICLE] },
      },
      {
        match: (url, method) => method === "PUT" && url.endsWith("/sites/site-1/posts/post-1"),
        status: 200,
        body: { status: "ok" },
      },
      {
        match: (url, method) =>
          method === "POST" && url.endsWith("/sites/site-1/posts/post-1/publish"),
        status: 422,
        body: { detail: "the article document is empty" },
      },
    ];

    ui("/sites/site-1/posts");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesPublishArticle }));
    const dialog = screen.getByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText(strings.sitesFieldPostSlug), {
      target: { value: "summer-menu" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: strings.sitesPublishArticle }));

    expect(await within(dialog).findByText("the article document is empty")).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  test("takes a published article offline in one click", async () => {
    const published = {
      ...ARTICLE,
      status: "published" as const,
      publishedAt: "2026-08-09T10:00:00Z",
    };
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: detail,
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/posts"),
        status: 200,
        body: { posts: [published] },
      },
      {
        match: (url, method) =>
          method === "POST" && url.endsWith("/sites/site-1/posts/post-1/unpublish"),
        status: 200,
        body: { status: "ok" },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
        status: 200,
        body: detail,
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/posts"),
        status: 200,
        body: { posts: [ARTICLE] },
      },
    ];

    ui("/sites/site-1/posts");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesUnpublishArticle }));

    await waitFor(() =>
      expect(
        calls.some(
          (call) => call.method === "POST" && call.url.endsWith("/posts/post-1/unpublish"),
        ),
      ).toBe(true),
    );
    expect(await screen.findByText(strings.sitesPostStatusDraft)).toBeTruthy();
  });
});
