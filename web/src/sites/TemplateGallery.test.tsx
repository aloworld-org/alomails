// The manual creation path (S2.11b): that the gallery really renders the
// catalog the server ships, that choosing is one click AND a keyboard-only
// arc, that a choice previews the server-rendered template, that Create goes
// through the instantiate route and lands in the new site's editor, and that
// a catalog which will not load still leaves a complete way to start.
//
// Same harness as the sibling suites: the REAL client and views run over one
// recording fake fetch; only the network is fake.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitesModule } from "./SitesModule";
import type { SiteTemplate } from "./types";

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
  if (typeof answer.body === "string") {
    return new Response(answer.body, {
      status: answer.status,
      headers: { "content-type": "text/html; charset=utf-8" },
    });
  }
  return new Response(JSON.stringify(answer.body), {
    status: answer.status,
    headers: { "content-type": "application/json" },
  });
});

/** The reads a screen makes before anything interesting happens. */
function fallback(url: string): Reply {
  const body = url.includes("/translation-readiness")
    ? { defaultLocale: "en", totalPages: 0, languages: [] }
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

vi.mock("../jmap/useJmapClient", () => ({
  useJmapClient: () => ({
    driveCreateDoc: vi.fn(),
    driveTrashNode: vi.fn(),
    driveUploadBlob: vi.fn(),
  }),
}));

const CONSULTANCY: SiteTemplate = {
  id: "consultancy",
  version: 1,
  kind: "services",
  name: "Consultancy",
  summary: "Three pages for advisers and independent professionals.",
  themePreset: "north",
  pages: [
    {
      title: "Home",
      slug: "",
      home: true,
      path: "/",
      sectionKinds: ["nav", "hero", "features", "footer"],
    },
    {
      title: "How we work",
      slug: "how-we-work",
      home: false,
      path: "/how-we-work",
      sectionKinds: ["nav", "text_image", "footer"],
    },
    {
      title: "Contact",
      slug: "contact",
      home: false,
      path: "/contact",
      sectionKinds: ["nav", "contact_form", "footer"],
    },
  ],
};

const RESTAURANT: SiteTemplate = {
  id: "restaurant",
  version: 1,
  kind: "hospitality",
  name: "Restaurant or café",
  summary: "A room, a menu and directions.",
  themePreset: "terra",
  pages: [
    { title: "Home", slug: "", home: true, path: "/", sectionKinds: ["nav", "hero", "footer"] },
    {
      title: "Menu",
      slug: "menu",
      home: false,
      path: "/menu",
      sectionKinds: ["nav", "text_image", "footer"],
    },
  ],
};

/** GET /sites/templates — consumable once, so make as many as needed. */
function catalogReply(templates: SiteTemplate[] = [CONSULTANCY, RESTAURANT]): Reply {
  return {
    match: (url, method) => method === "GET" && url.endsWith("/sites/templates"),
    status: 200,
    body: { templates },
  };
}

/** GET /sites/templates/{id}/preview — the rendered document, as text. */
function previewReply(id: string, html: string): Reply {
  return {
    match: (url, method) =>
      method === "GET" && url.includes(`/sites/templates/${id}/preview`),
    status: 200,
    body: html,
  };
}

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{location.pathname}</output>;
}

function ui(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <LocationProbe />
      <Routes>
        <Route path="/sites/*" element={<SitesModule />} />
      </Routes>
    </MemoryRouter>,
  );
}

/** Opens New website and switches to the manual path. */
async function openGallery() {
  ui("/sites");
  fireEvent.click(await screen.findByRole("button", { name: strings.sitesNewSite }));
  fireEvent.click(screen.getByRole("button", { name: strings.sitesTemplateChoice }));
}

/** The card of one template, as a radio in the gallery's group. */
function card(name: string): HTMLElement {
  return screen.getByRole("radio", { name: new RegExp(name, "i") });
}

beforeEach(() => {
  calls.length = 0;
  replies = [];
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the template gallery", () => {
  test("shows the shipped catalog with what each template contains", async () => {
    replies = [catalogReply()];
    await openGallery();

    expect(await screen.findByText(CONSULTANCY.name)).toBeTruthy();
    expect(screen.getByText(CONSULTANCY.summary)).toBeTruthy();
    expect(screen.getByText(RESTAURANT.name)).toBeTruthy();
    // The blank start is still offered, first, and is what a person who
    // ignores the gallery gets.
    const blank = card(strings.sitesBlankTemplate);
    expect(blank.getAttribute("aria-checked")).toBe("true");
    expect(screen.getByText(strings.sitesBlankPreviewNote)).toBeTruthy();
    // Each card says how many pages it brings, and names them.
    expect(screen.getAllByText(strings.sitesTemplatePageCount(3)).length).toBe(1);
    expect(screen.getByText("How we work")).toBeTruthy();
  });

  test("one click previews the chosen template, rendered by the server", async () => {
    replies = [
      catalogReply(),
      previewReply("consultancy", "<html><body><h1>Advice that lasts</h1></body></html>"),
    ];
    await openGallery();

    fireEvent.click(await screen.findByRole("radio", { name: /Consultancy/i }));
    const frame = (await screen.findByTitle(
      strings.sitesTemplatePreviewTitle(CONSULTANCY.name),
    )) as HTMLIFrameElement;
    expect(frame.getAttribute("sandbox")).toBe("allow-scripts");
    expect(frame.getAttribute("srcdoc")).toContain("Advice that lasts");
    expect(card(CONSULTANCY.name).getAttribute("aria-checked")).toBe("true");
    expect(card(strings.sitesBlankTemplate).getAttribute("aria-checked")).toBe("false");
  });

  test("the template's other pages are previewed from the tabs", async () => {
    replies = [
      catalogReply(),
      previewReply("consultancy", "<html><body><h1>Advice that lasts</h1></body></html>"),
    ];
    await openGallery();
    fireEvent.click(await screen.findByRole("radio", { name: /Consultancy/i }));
    await screen.findByTitle(strings.sitesTemplatePreviewTitle(CONSULTANCY.name));

    replies = [
      previewReply("consultancy", "<html><body><h1>Write to us</h1></body></html>"),
    ];
    fireEvent.click(screen.getByRole("button", { name: "Contact" }));
    await waitFor(() =>
      expect(
        (
          screen.getByTitle(
            strings.sitesTemplatePreviewTitle(CONSULTANCY.name),
          ) as HTMLIFrameElement
        ).getAttribute("srcdoc"),
      ).toContain("Write to us"),
    );
    expect(
      calls.some((call) => call.url.includes("/sites/templates/consultancy/preview?page=contact")),
    ).toBe(true);
  });

  test("the gallery is a radio group the arrow keys move through", async () => {
    replies = [
      catalogReply(),
      previewReply("consultancy", "<html><body><h1>Advice</h1></body></html>"),
      previewReply("restaurant", "<html><body><h1>Our room</h1></body></html>"),
    ];
    await openGallery();
    await screen.findByText(CONSULTANCY.name);

    const blank = card(strings.sitesBlankTemplate);
    // Roving focus: only the checked option is in the tab order.
    expect(blank.getAttribute("tabindex")).toBe("0");
    expect(card(CONSULTANCY.name).getAttribute("tabindex")).toBe("-1");

    blank.focus();
    fireEvent.keyDown(blank, { key: "ArrowRight" });
    expect(card(CONSULTANCY.name).getAttribute("aria-checked")).toBe("true");
    expect(document.activeElement).toBe(card(CONSULTANCY.name));

    fireEvent.keyDown(card(CONSULTANCY.name), { key: "End" });
    expect(card(RESTAURANT.name).getAttribute("aria-checked")).toBe("true");
    fireEvent.keyDown(card(RESTAURANT.name), { key: "ArrowLeft" });
    expect(card(CONSULTANCY.name).getAttribute("aria-checked")).toBe("true");
    // A key that means nothing here is left to the browser.
    fireEvent.keyDown(card(CONSULTANCY.name), { key: "a" });
    expect(card(CONSULTANCY.name).getAttribute("aria-checked")).toBe("true");
  });

  test("Create instantiates the template and opens the new site's Home page", async () => {
    replies = [
      catalogReply(),
      previewReply("consultancy", "<html><body><h1>Advice</h1></body></html>"),
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/config"),
        status: 200,
        body: { domain: "alosites.com" },
      },
      {
        match: (url, method) => method === "GET" && url.includes("/sites/subdomain-check"),
        status: 200,
        body: { subdomain: "north-advice", available: true },
      },
      {
        match: (url, method) =>
          method === "POST" && url.endsWith("/sites/templates/consultancy"),
        status: 200,
        body: {
          site: {
            id: "site-t",
            name: "North Advice",
            subdomain: "north-advice",
            status: "draft",
            defaultLocale: "en",
            enabledLocales: ["en"],
          },
          pages: [
            {
              id: "page-h",
              slug: "",
              title: "Home",
              home: true,
              seoTitle: null,
              seoDescription: null,
              sections: { schema_version: 1, sections: [] },
            },
            {
              id: "page-c",
              slug: "contact",
              title: "Contact",
              home: false,
              seoTitle: null,
              seoDescription: null,
              sections: { schema_version: 1, sections: [] },
            },
          ],
          template: { id: "consultancy", version: 1 },
        },
      },
      {
        match: (url, method) =>
          method === "GET" && url.endsWith("/sites/site-t/pages/page-h"),
        status: 200,
        body: {
          id: "page-h",
          slug: "",
          title: "Home",
          home: true,
          seoTitle: null,
          seoDescription: null,
          sections: { schema_version: 1, sections: [] },
        },
      },
    ];
    await openGallery();
    fireEvent.click(await screen.findByRole("radio", { name: /Consultancy/i }));
    fireEvent.change(screen.getByLabelText(strings.sitesFieldName), {
      target: { value: "North Advice" },
    });
    await waitFor(() => expect(screen.getByText(strings.sitesAddressAvailable)).toBeTruthy(), {
      timeout: 3000,
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesCreateSite }));

    await waitFor(() =>
      expect(screen.getByTestId("location").textContent).toBe(
        "/sites/site-t/pages/page-h",
      ),
    );
    // The address self-suggested from the name, and the write carried both.
    expect(
      calls.find(
        (call) => call.method === "POST" && call.url.endsWith("/sites/templates/consultancy"),
      )?.body,
    ).toEqual({ name: "North Advice", subdomain: "north-advice" });
    // Instantiating is one transaction: no separate site or page was created.
    expect(calls.some((call) => call.method === "POST" && call.url.endsWith("/sites"))).toBe(
      false,
    );
  });

  test("a refusal from the server is shown in the dialog, which stays open", async () => {
    replies = [
      catalogReply(),
      previewReply("consultancy", "<html><body><h1>Advice</h1></body></html>"),
      {
        match: (url, method) => method === "GET" && url.includes("/sites/subdomain-check"),
        status: 200,
        body: { subdomain: "north-advice", available: true },
      },
      {
        match: (url, method) =>
          method === "POST" && url.endsWith("/sites/templates/consultancy"),
        status: 422,
        body: { detail: "subdomain is already taken" },
      },
    ];
    await openGallery();
    fireEvent.click(await screen.findByRole("radio", { name: /Consultancy/i }));
    fireEvent.change(screen.getByLabelText(strings.sitesFieldName), {
      target: { value: "North Advice" },
    });
    await waitFor(() => expect(screen.getByText(strings.sitesAddressAvailable)).toBeTruthy(), {
      timeout: 3000,
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesCreateSite }));

    expect(await screen.findByText("subdomain is already taken")).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  test("a catalog that will not load leaves the blank path complete", async () => {
    replies = [
      {
        match: (url, method) => method === "GET" && url.endsWith("/sites/templates"),
        status: 500,
        body: {},
      },
      {
        match: (url, method) => method === "GET" && url.includes("/sites/subdomain-check"),
        status: 200,
        body: { subdomain: "blank-co", available: true },
      },
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites"),
        status: 200,
        body: {
          id: "site-b",
          name: "Blank Co",
          subdomain: "blank-co",
          status: "draft",
          defaultLocale: "en",
          enabledLocales: ["en"],
        },
      },
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites/site-b/pages"),
        status: 200,
        body: {
          id: "page-b",
          slug: "",
          title: strings.sitesHomePageTitle,
          home: true,
          seoTitle: null,
          seoDescription: null,
        },
      },
      {
        match: (url, method) =>
          method === "GET" && url.endsWith("/sites/site-b/pages/page-b"),
        status: 200,
        body: {
          id: "page-b",
          slug: "",
          title: strings.sitesHomePageTitle,
          home: true,
          seoTitle: null,
          seoDescription: null,
          sections: { schema_version: 1, sections: [] },
        },
      },
    ];
    await openGallery();

    expect(await screen.findByText(strings.sitesTemplatesLoadFailed)).toBeTruthy();
    fireEvent.change(screen.getByLabelText(strings.sitesFieldName), {
      target: { value: "Blank Co" },
    });
    await waitFor(() => expect(screen.getByText(strings.sitesAddressAvailable)).toBeTruthy(), {
      timeout: 3000,
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesCreateSite }));
    await waitFor(() =>
      expect(screen.getByTestId("location").textContent).toBe(
        "/sites/site-b/pages/page-b",
      ),
    );
  });
});
