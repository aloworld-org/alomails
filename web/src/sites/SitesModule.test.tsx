// The wiring the type checker cannot see: that the list really renders what
// the API answered, that the create form really asks the server about the
// typed address and sends exactly what was typed, and that a refusal from the
// server is shown to the user instead of swallowed.
//
// The auth layer is stubbed down to one recording `fetch`, so the REAL
// client and the real views run — only the network is fake.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

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
  const body = url.includes("/posts")
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
}));

vi.mock("../jmap/useJmapClient", () => ({
  useJmapClient: () => fakeJmap,
}));

const ALPHA: Site = { id: "site-1", name: "Alpha Bakery", subdomain: "alpha", status: "live" };
const BETA: Site = { id: "site-2", name: "Beta Atelier", subdomain: "beta", status: "draft" };
const HOME: SitePage = { id: "page-1", slug: "", title: "Welcome", home: true };
const ABOUT: SitePage = { id: "page-2", slug: "about", title: "About us", home: false };
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
      <LocationProbe />
      <Routes>
        <Route path="/sites/*" element={<SitesModule />} />
        {/* The real shell owns Drive; this sink lets navigation assertions
            observe the hand-off without a test-only unmatched-route warning. */}
        <Route path="/drive" element={null} />
      </Routes>
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

  test("an empty tenant sees the empty state, not a bare table", async () => {
    ui("/sites");
    expect(await screen.findByText(strings.sitesNoSitesTitle)).toBeTruthy();
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

describe("creating a site", () => {
  test("the typed address is checked live against the server", async () => {
    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));
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
      () => expect(screen.getByText(strings.sitesSubdomainAvailable("acme"))).toBeTruthy(),
      { timeout: 3000 },
    );
    const check = calls.find((c) => c.url.includes("/sites/subdomain-check"));
    expect(check?.url).toContain("subdomain=acme");
  });

  test("a taken address, and a rule the server names, are both shown", async () => {
    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));

    replies = [
      {
        match: (url, method) => method === "GET" && url.includes("/sites/subdomain-check"),
        status: 200,
        body: { subdomain: "taken", available: false },
      },
    ];
    const address = screen.getByLabelText(strings.sitesFieldSubdomain);
    fireEvent.change(address, { target: { value: "taken" } });
    await waitFor(() => expect(screen.getByText(strings.sitesSubdomainTaken("taken"))).toBeTruthy(), {
      timeout: 3000,
    });

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
  });

  test("submitting sends what was typed and opens the new site", async () => {
    replies = [
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites"),
        status: 200,
        body: { ...ALPHA, id: "site-9", name: "Acme", subdomain: "acme", status: "draft" },
      },
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/site-9"),
        status: 200,
        body: { id: "site-9", name: "Acme", subdomain: "acme", status: "draft", publish: null },
      },
    ];
    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));
    fireEvent.change(screen.getByLabelText(strings.sitesFieldName), {
      target: { value: "Acme" },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesFieldSubdomain), {
      target: { value: "acme" },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesCreateSite }));

    // The write carried exactly what was typed…
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()).toMatchObject({
      method: "POST",
      body: { name: "Acme", subdomain: "acme" },
    });
    // …and the module navigated into the created site.
    expect(await screen.findByText(strings.sitesNoPagesTitle)).toBeTruthy();
  });

  test("the server's refusal is shown in the dialog, which stays open", async () => {
    replies = [
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites"),
        status: 422,
        body: { detail: "subdomain is already taken" },
      },
    ];
    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));
    fireEvent.change(screen.getByLabelText(strings.sitesFieldName), {
      target: { value: "Acme" },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesFieldSubdomain), {
      target: { value: "acme" },
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
    // The copy names the real address the site will serve at.
    expect(await screen.findByText(strings.sitesGoesLiveAt("beta.alosites.com"))).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: strings.sitesPublish }));
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()?.method).toBe("POST");
    expect(lastWrite()?.url.endsWith("/sites/site-2/publish")).toBe(true);
    // The reload shows the live state, the address now a real link.
    expect(await screen.findByText(strings.sitesStatusLive)).toBeTruthy();
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
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesPublish }));
    expect(await screen.findByText("site has no home page")).toBeTruthy();
    expect(screen.getByText(strings.sitesStatusDraft)).toBeTruthy();
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
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesUnpublish }));
    // Armed, not fired: nothing was written, the button now asks to confirm.
    expect(lastWrite()).toBeUndefined();
    fireEvent.click(screen.getByRole("button", { name: strings.sitesConfirmUnpublish }));
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()?.url.endsWith("/sites/site-1/unpublish")).toBe(true);
    expect(await screen.findByText(strings.sitesStatusDraft)).toBeTruthy();
  });

  test("the create form previews the full address once the domain is known", async () => {
    replies = [config];
    ui("/sites");
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));
    fireEvent.change(screen.getByLabelText(strings.sitesFieldSubdomain), {
      target: { value: "Acme" },
    });
    // The preview composes the typed label (lowercased) with the domain.
    expect(await screen.findByText(strings.sitesAddressPreview("acme.alosites.com"))).toBeTruthy();
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
    ui("/sites/site-1");
    expect(await screen.findByText("Alpha Bakery")).toBeTruthy();
    expect(screen.getByText(strings.sitesStatusLive)).toBeTruthy();
    expect(screen.getByText("Welcome")).toBeTruthy();
    expect(screen.getByText(strings.sitesHomeBadge)).toBeTruthy();
    expect(screen.getByText("About us")).toBeTruthy();
    expect(screen.getByText("/about")).toBeTruthy();
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
    ui("/sites/site-2");
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
});
