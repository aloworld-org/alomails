// The page editor's wiring: that the stack renders exactly what the server
// stored, that every gesture (add via the picker, edit, reorder, delete)
// sends the precise section op the wire-verified API expects, that props the
// forms do not offer (a contact form's form_id, a hero's untouched
// subheading) survive an edit untouched, and that a refusal is shown in the
// dialog instead of swallowed. Same harness as SitesModule.test.tsx: the
// REAL client and views run, only the network is fake.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitesModule } from "./SitesModule";
import { SECTIONS_SCHEMA_VERSION, SECTION_KINDS } from "./sections";
import type { Section, SectionsEnvelope } from "./sections";

/** The stack's controls are named after the section they act on (S2.16b2), so
 *  a test that wants "the first Edit" asks for the marker the editor puts on
 *  them rather than for a name that is now different on every row. */
function sectionControls(control: string): HTMLElement[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      `[data-section-control="${control}"]`,
    ),
  ];
}

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
  const answer =
    index === -1
      ? { status: 200, body: {} }
      : (replies.splice(index, 1)[0] as Reply);
  // A string body is a document (the draft preview), not JSON.
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

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

function env(sections: Section[]): SectionsEnvelope {
  return { schema_version: SECTIONS_SCHEMA_VERSION, sections };
}

const HERO: Section = {
  type: "hero",
  heading: "Fresh bread daily",
  subheading: "Since 1962",
};
const CONTACT: Section = {
  type: "contact_form",
  heading: "Write to us",
  form_id: "f-1",
};
const FAQ: Section = {
  type: "faq",
  items: [{ question: "When?", answer: "Every day." }],
};

/** The page GET the editor loads first. */
function pageReply(sections: Section[]): Reply {
  return {
    match: (url, method) =>
      method === "GET" && url.endsWith("/sites/site-1/pages/page-1"),
    status: 200,
    body: {
      id: "page-1",
      slug: "",
      title: "Welcome",
      home: true,
      seoTitle: null,
      seoDescription: null,
      sections: env(sections),
    },
  };
}

function ui(path = "/sites/site-1/pages/page-1") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/sites/*" element={<SitesModule />} />
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
});

afterEach(cleanup);

describe("manual page translation", () => {
  const localizedFallback = {
    id: "page-1",
    slug: "",
    title: "Welcome",
    home: true,
    seoTitle: "Alpha Bakery",
    seoDescription: "Fresh bread daily.",
    sections: env([HERO, FAQ]),
    requestedLocale: "fr",
    resolvedLocale: "en",
    fallback: true,
  };

  function languageSiteReply(): Reply {
    return {
      match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
      status: 200,
      body: {
        id: "site-1",
        name: "Alpha Bakery",
        subdomain: "alpha",
        status: "draft",
        publish: null,
        theme: {},
        defaultLocale: "en",
        enabledLocales: ["en", "fr"],
      },
    };
  }

  test("a missing language is read-only until the owner copies the visible fallback", async () => {
    replies = [
      languageSiteReply(),
      {
        match: (url, method) =>
          method === "GET" && url.endsWith("/pages/page-1/locales/fr"),
        status: 200,
        body: localizedFallback,
      },
    ];
    ui("/sites/site-1/pages/page-1?locale=fr");

    expect(
      await screen.findByText(strings.sitesTranslationMissingTitle("FR")),
    ).toBeTruthy();
    expect(sectionControls("edit")[0]!).toHaveProperty("disabled", true);

    const savedFrench = {
      ...localizedFallback,
      requestedLocale: "fr",
      resolvedLocale: "fr",
      fallback: false,
    };
    replies = [
      {
        match: (url, method) =>
          method === "PUT" && url.endsWith("/pages/page-1/locales/fr"),
        status: 200,
        body: savedFrench,
      },
    ];
    fireEvent.click(
      screen.getByRole("button", {
        name: strings.sitesCopyTranslation("EN", "FR"),
      }),
    );

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()).toMatchObject({
      method: "PUT",
      body: {
        title: "Welcome",
        slug: "",
        seoTitle: "Alpha Bakery",
        seoDescription: "Fresh bread daily.",
        sections: env([HERO, FAQ]),
      },
    });
    expect(
      await screen.findByText(strings.sitesTranslationDetails),
    ).toBeTruthy();
    expect(sectionControls("edit")[0]!).toHaveProperty("disabled", false);
  });

  test("localized section changes replace only the selected language draft", async () => {
    const french = {
      ...localizedFallback,
      title: "Bienvenue",
      requestedLocale: "fr",
      resolvedLocale: "fr",
      fallback: false,
    };
    replies = [
      languageSiteReply(),
      {
        match: (url, method) =>
          method === "GET" && url.endsWith("/pages/page-1/locales/fr"),
        status: 200,
        body: french,
      },
    ];
    ui("/sites/site-1/pages/page-1?locale=fr");
    await screen.findByText(strings.sitesTranslationDetails);

    replies = [
      {
        match: (url, method) =>
          method === "PUT" && url.endsWith("/pages/page-1/locales/fr"),
        status: 200,
        body: { ...french, sections: env([FAQ, HERO]) },
      },
    ];
    fireEvent.click(sectionControls("down")[0]!);

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()).toMatchObject({
      method: "PUT",
      body: { title: "Bienvenue", sections: env([FAQ, HERO]) },
    });
    expect(lastWrite()?.url.endsWith("/pages/page-1/locales/fr")).toBe(true);
  });
});

describe("the section stack", () => {
  test("renders the stored sections in order, each with its type and words", async () => {
    replies = [pageReply([HERO, CONTACT, FAQ])];
    ui();
    expect(await screen.findByText("Welcome")).toBeTruthy();
    expect(screen.getByText(strings.sitesSectionHero)).toBeTruthy();
    expect(screen.getByText("Fresh bread daily")).toBeTruthy();
    expect(screen.getByText(strings.sitesSectionContactForm)).toBeTruthy();
    expect(screen.getByText(strings.sitesSectionFaq)).toBeTruthy();
    // The FAQ has no heading, so its card counts its entries.
    expect(screen.getByText(strings.sitesCountEntries(1))).toBeTruthy();
  });

  test("a foreign or stale page reads as an error with the way back", async () => {
    replies = [
      {
        match: (url, method) =>
          method === "GET" && url.endsWith("/sites/site-1/pages/page-1"),
        status: 404,
        body: { detail: "no such page" },
      },
    ];
    ui();
    expect(await screen.findByText("no such page")).toBeTruthy();
    expect(screen.getByText(strings.sitesBackToSite)).toBeTruthy();
  });
});

describe("reviewed page changes", () => {
  const proposal = {
    schema_version: 1,
    operations: [
      {
        op: "rewrite_copy",
        target: { index: 0, type: "hero" },
        pointer: "/heading",
        text: "A clearer welcome",
      },
    ],
  } as const;

  test("a proposal is readable and writes only after Approve", async () => {
    replies = [
      pageReply([HERO]),
      {
        match: (url, method) => method === "GET" && url.endsWith("/preview"),
        status: 200,
        body: "<!doctype html><p>Before copy</p>",
      },
      {
        match: (url, method) => method === "POST" && url.endsWith("/ai-edits"),
        status: 200,
        body: { proposal, previewHtml: "<!doctype html><p>After copy</p>" },
      },
    ];
    ui();
    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesShowPreview }),
    );
    const frame = (await screen.findByTitle(
      strings.sitesPreviewTitle,
    )) as HTMLIFrameElement;
    await waitFor(() =>
      expect(frame.getAttribute("srcdoc")).toContain("Before copy"),
    );
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesAiChanges }),
    );
    fireEvent.change(await screen.findByLabelText(strings.sitesAiInstruction), {
      target: { value: "Make the welcome clearer" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesAiPropose }),
    );

    expect(
      await screen.findByText(
        strings.sitesAiCopyChange(strings.sitesSectionHero),
      ),
    ).toBeTruthy();
    expect(lastWrite()).toMatchObject({
      method: "POST",
      body: { instruction: "Make the welcome clearer" },
    });
    await waitFor(() =>
      expect(frame.getAttribute("srcdoc")).toContain("After copy"),
    );
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesAiPreviewBefore }),
    );
    expect(frame.getAttribute("srcdoc")).toContain("Before copy");
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesAiPreviewAfter }),
    );
    expect(frame.getAttribute("srcdoc")).toContain("After copy");
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesAiDiscard }),
    );
    expect(calls.filter((call) => call.method === "PUT")).toHaveLength(0);
    expect(frame.getAttribute("srcdoc")).toContain("Before copy");
    expect(
      screen.queryByRole("button", { name: strings.sitesAiPreviewAfter }),
    ).toBeNull();

    replies = [
      {
        match: (url, method) => method === "POST" && url.endsWith("/ai-edits"),
        status: 200,
        body: { proposal, previewHtml: "<!doctype html><p>After copy</p>" },
      },
      {
        match: (url, method) => method === "PUT" && url.endsWith("/ai-edits"),
        status: 200,
        body: {
          sections: env([{ ...HERO, heading: "A clearer welcome" }]),
        },
      },
    ];
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesAiPropose }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesAiApprove }),
    );

    await waitFor(() =>
      expect(screen.getByText("A clearer welcome")).toBeTruthy(),
    );
    expect(lastWrite()).toMatchObject({ method: "PUT", body: { proposal } });
  });
});

describe("search and sharing details", () => {
  test("the visible page action saves both overrides through the page route", async () => {
    replies = [pageReply([HERO])];
    ui();
    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesSeoAction }),
    );
    expect(screen.getByLabelText(strings.sitesSeoPreview)).toBeTruthy();

    fireEvent.change(screen.getByLabelText(strings.sitesSeoFieldTitle), {
      target: { value: "  Bread delivered today  " },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesSeoFieldDescription), {
      target: { value: "  Warm loaves from our neighbourhood bakery.  " },
    });
    replies = [
      {
        match: (url, method) =>
          method === "PUT" && url.endsWith("/sites/site-1/pages/page-1"),
        status: 200,
        body: { status: "ok" },
      },
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.sitesSeoSave }));

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()!.body).toEqual({
      seoTitle: "Bread delivered today",
      seoDescription: "Warm loaves from our neighbourhood bakery.",
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  test("a server refusal keeps the entered details visible with its reason", async () => {
    replies = [pageReply([])];
    ui();
    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesSeoAction }),
    );
    fireEvent.change(screen.getByLabelText(strings.sitesSeoFieldTitle), {
      target: { value: "A title the server refuses" },
    });
    replies = [
      {
        match: (url, method) =>
          method === "PUT" && url.endsWith("/sites/site-1/pages/page-1"),
        status: 422,
        body: { detail: "SEO title must be at most 200 characters" },
      },
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.sitesSeoSave }));

    expect(
      await screen.findByText("SEO title must be at most 200 characters"),
    ).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(
      (screen.getByLabelText(strings.sitesSeoFieldTitle) as HTMLInputElement)
        .value,
    ).toBe("A title the server refuses");
  });
});

/** One tile of the open palette (S3.01d) — the add path every section takes. */
function paletteTile(kind: string): HTMLElement {
  const found = document.querySelector<HTMLElement>(
    `[data-palette-tile="${kind}"]`,
  );
  if (found === null) throw new Error(`no ${kind} tile in the palette`);
  return found;
}

describe("adding a section", () => {
  test("the palette offers every type; saving the form POSTs exactly the typed section", async () => {
    replies = [pageReply([])];
    ui();
    // Empty page → empty state; the header control opens the palette.
    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesAddSection }),
    );

    for (const kind of SECTION_KINDS) {
      expect(paletteTile(kind)).toBeTruthy();
    }

    replies = [
      {
        match: (url, method) =>
          method === "POST" &&
          url.endsWith("/sites/site-1/pages/page-1/sections"),
        status: 200,
        body: {
          sections: env([{ type: "hero", heading: "Big and warm" }]),
        },
      },
    ];
    // Nothing seeded here (the palette request has no reply), so the tile
    // opens the prop form — the pre-palette behaviour.
    fireEvent.click(paletteTile("hero"));
    fireEvent.change(screen.getByLabelText(strings.sitesFieldHeading), {
      target: { value: "  Big and warm " },
    });
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesSaveSection }),
    );

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    // Exactly the typed section: trimmed, untouched optionals ABSENT — the
    // stored JSON never grows blank keys — plus the position the palette
    // chose for it (an empty page has only the top).
    expect(lastWrite()!.body).toEqual({
      section: { type: "hero", heading: "Big and warm" },
      index: 0,
    });
    // The stack renders the envelope the server answered.
    expect(await screen.findByText("Big and warm")).toBeTruthy();
  });

  test("a list section sends every entry the user added", async () => {
    replies = [pageReply([])];
    ui();
    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesAddSection }),
    );
    fireEvent.click(paletteTile("faq"));
    // The form starts with one blank entry; fill it, add a second.
    fireEvent.change(screen.getByLabelText(strings.sitesFieldQuestion), {
      target: { value: "When?" },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesFieldAnswer), {
      target: { value: "Every day." },
    });
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesAddQuestion }),
    );
    fireEvent.change(screen.getAllByLabelText(strings.sitesFieldQuestion)[1]!, {
      target: { value: "Where?" },
    });
    fireEvent.change(screen.getAllByLabelText(strings.sitesFieldAnswer)[1]!, {
      target: { value: "At the harbour." },
    });

    replies = [
      {
        match: (url, method) =>
          method === "POST" &&
          url.endsWith("/sites/site-1/pages/page-1/sections"),
        status: 200,
        body: { sections: env([FAQ]) },
      },
    ];
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesSaveSection }),
    );
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()!.body).toEqual({
      section: {
        type: "faq",
        items: [
          { question: "When?", answer: "Every day." },
          { question: "Where?", answer: "At the harbour." },
        ],
      },
      index: 0,
    });
  });

  test("the server's refusal is shown in the form, which stays open", async () => {
    replies = [pageReply([])];
    ui();
    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesAddSection }),
    );
    fireEvent.click(paletteTile("cta"));
    replies = [
      {
        match: (url, method) =>
          method === "POST" &&
          url.endsWith("/sites/site-1/pages/page-1/sections"),
        status: 422,
        body: { detail: "cta section: heading must not be blank" },
      },
    ];
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesSaveSection }),
    );
    expect(
      await screen.findByText("cta section: heading must not be blank"),
    ).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});

describe("editing a section", () => {
  test("copy tools propose one selected field and write only after approval", async () => {
    replies = [pageReply([HERO])];
    ui();
    await screen.findByText(strings.sitesSectionHero);
    fireEvent.click(sectionControls("edit")[0]!);

    fireEvent.click(
      screen.getAllByRole("button", { name: strings.sitesAiImproveCopy })[0]!,
    );
    expect(
      screen.getByRole("button", { name: strings.sitesAiRewrite }),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: strings.sitesAiShorter }),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: strings.sitesAiLonger }),
    ).toBeTruthy();

    const copyProposal = {
      schema_version: 1,
      operations: [
        {
          op: "rewrite_copy",
          target: { index: 0, type: "hero" },
          pointer: "/heading",
          text: "Fresh bread",
        },
      ],
    } as const;
    replies = [
      {
        match: (url, method) => method === "POST" && url.endsWith("/ai-edits"),
        status: 200,
        body: {
          proposal: copyProposal,
          previewHtml: "<!doctype html><p>Fresh bread</p>",
        },
      },
    ];
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesAiShorter }),
    );

    expect(await screen.findByText(strings.sitesAiCopyBefore)).toBeTruthy();
    expect(screen.getByText(strings.sitesAiCopyAfter)).toBeTruthy();
    expect(screen.getAllByText("Fresh bread daily")).toHaveLength(2);
    expect(screen.getByText("Fresh bread")).toBeTruthy();
    expect(lastWrite()).toMatchObject({
      method: "POST",
      body: {
        copy: {
          target: { index: 0, type: "hero" },
          pointer: "/heading",
          action: "shorter",
        },
      },
    });
    expect(calls.filter((call) => call.method === "PUT")).toHaveLength(0);

    replies = [
      {
        match: (url, method) => method === "PUT" && url.endsWith("/ai-edits"),
        status: 200,
        body: { sections: env([{ ...HERO, heading: "Fresh bread" }]) },
      },
    ];
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesAiApprove }),
    );

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(await screen.findByText("Fresh bread")).toBeTruthy();
    expect(lastWrite()).toMatchObject({
      method: "PUT",
      body: { proposal: copyProposal },
    });
  });

  test("the form opens prefilled and PUTs to the section's index", async () => {
    replies = [pageReply([HERO, CONTACT])];
    ui();
    await screen.findByText(strings.sitesSectionHero);
    fireEvent.click(sectionControls("edit")[0]!);
    const heading = screen.getByLabelText(
      strings.sitesFieldHeading,
    ) as HTMLInputElement;
    expect(heading.value).toBe("Fresh bread daily");

    replies = [
      {
        match: (url, method) =>
          method === "PUT" &&
          url.endsWith("/sites/site-1/pages/page-1/sections/0"),
        status: 200,
        body: {
          sections: env([{ ...HERO, heading: "Warm bread daily" }, CONTACT]),
        },
      },
    ];
    fireEvent.change(heading, { target: { value: "Warm bread daily" } });
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesSaveSection }),
    );

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    // The untouched subheading rode along — an edit never strips what the
    // user did not change.
    expect(lastWrite()!.body).toEqual({
      section: {
        type: "hero",
        heading: "Warm bread daily",
        subheading: "Since 1962",
      },
    });
    expect(await screen.findByText("Warm bread daily")).toBeTruthy();
  });

  test("props the form does not offer (form_id) survive an edit untouched", async () => {
    replies = [pageReply([CONTACT])];
    ui();
    await screen.findByText(strings.sitesSectionContactForm);
    fireEvent.click(sectionControls("edit")[0]!);

    replies = [
      {
        match: (url, method) =>
          method === "PUT" &&
          url.endsWith("/sites/site-1/pages/page-1/sections/0"),
        status: 200,
        body: { sections: env([{ ...CONTACT, heading: "Talk to us" }]) },
      },
    ];
    fireEvent.change(screen.getByLabelText(strings.sitesFieldHeading), {
      target: { value: "Talk to us" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesSaveSection }),
    );

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()!.body).toEqual({
      section: { type: "contact_form", heading: "Talk to us", form_id: "f-1" },
    });
  });

  test("how an image is framed survives an edit of the text beside it", async () => {
    // The image form offers the crop and the focal point now (S2.07c), but
    // this is the case that does not go near it: editing the text beside a
    // photo must not silently unframe it.
    const framed: Section = {
      type: "hero",
      heading: "Fresh bread daily",
      image: {
        blob_id: "blob-1",
        alt: "Loaves cooling on a rack",
        crop: { x_bp: 1250, y_bp: 0, width_bp: 7500, height_bp: 10000 },
        focal: { x_bp: 4000, y_bp: 3500 },
      },
    };
    replies = [pageReply([framed])];
    ui();
    await screen.findByText(strings.sitesSectionHero);
    fireEvent.click(sectionControls("edit")[0]!);

    replies = [
      {
        match: (url, method) =>
          method === "PUT" &&
          url.endsWith("/sites/site-1/pages/page-1/sections/0"),
        status: 200,
        body: { sections: env([{ ...framed, heading: "Warm bread daily" }]) },
      },
    ];
    fireEvent.change(screen.getByLabelText(strings.sitesFieldHeading), {
      target: { value: "Warm bread daily" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesSaveSection }),
    );

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()!.body).toEqual({
      section: { ...framed, heading: "Warm bread daily" },
    });
  });
});

describe("reordering and deleting", () => {
  test("the move-down button asks the server to move the section", async () => {
    replies = [pageReply([HERO, FAQ])];
    ui();
    await screen.findByText(strings.sitesSectionHero);

    replies = [
      {
        match: (url, method) =>
          method === "POST" &&
          url.endsWith("/sites/site-1/pages/page-1/sections/0/move"),
        status: 200,
        body: { sections: env([FAQ, HERO]) },
      },
    ];
    fireEvent.click(sectionControls("down")[0]!);
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()!.body).toEqual({ to: 1 });
  });

  test("deleting takes two clicks; one alone changes nothing", async () => {
    replies = [pageReply([HERO, FAQ])];
    ui();
    await screen.findByText(strings.sitesSectionFaq);

    // The first click only arms the confirmation.
    fireEvent.click(sectionControls("delete")[1]!);
    expect(lastWrite()).toBeUndefined();

    replies = [
      {
        match: (url, method) =>
          method === "DELETE" &&
          url.endsWith("/sites/site-1/pages/page-1/sections/1"),
        status: 200,
        body: { sections: env([HERO]) },
      },
    ];
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesConfirmDelete }),
    );
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()!.method).toBe("DELETE");
    await waitFor(() =>
      expect(screen.queryByText(strings.sitesSectionFaq)).toBeNull(),
    );
  });
});

describe("the live preview", () => {
  /** The server-rendered draft document the pane fetches. */
  function previewReply(html: string): Reply {
    return {
      match: (url, method) =>
        method === "GET" && url.endsWith("/sites/site-1/pages/page-1/preview"),
      status: 200,
      body: html,
    };
  }

  function previewCalls(): number {
    return calls.filter((c) => c.method === "GET" && c.url.endsWith("/preview"))
      .length;
  }

  function frame(): HTMLIFrameElement {
    return screen.getByTitle(strings.sitesPreviewTitle) as HTMLIFrameElement;
  }

  async function openPreview(): Promise<void> {
    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesShowPreview }),
    );
  }

  test("the preview stays out of the building workspace until requested", async () => {
    replies = [pageReply([HERO]), previewReply("<!doctype html><p>draft</p>")];
    ui();

    await screen.findByText(strings.sitesSectionHero);
    expect(screen.queryByTitle(strings.sitesPreviewTitle)).toBeNull();
    expect(previewCalls()).toBe(0);

    await openPreview();
    await waitFor(() =>
      expect(frame().getAttribute("srcdoc")).toContain("draft"),
    );
    expect(previewCalls()).toBe(1);

    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesHidePreview }),
    );
    expect(screen.queryByTitle(strings.sitesPreviewTitle)).toBeNull();
  });

  test("the pane shows the server-rendered draft in a sandboxed frame", async () => {
    replies = [
      pageReply([HERO]),
      previewReply("<!doctype html>\n<p>draft one</p>"),
    ];
    ui();
    await screen.findByText(strings.sitesSectionHero);
    await openPreview();
    await waitFor(() =>
      expect(frame().getAttribute("srcdoc")).toContain("draft one"),
    );
    // The draft document may run its menu script but never touches this
    // origin or navigates the app.
    expect(frame().getAttribute("sandbox")).toBe("allow-scripts");
  });

  test("a successful save refreshes the preview; a refused one does not", async () => {
    replies = [
      pageReply([HERO, FAQ]),
      previewReply("<!doctype html>\n<p>before</p>"),
    ];
    ui();
    await screen.findByText(strings.sitesSectionHero);
    await openPreview();
    await waitFor(() =>
      expect(frame().getAttribute("srcdoc")).toContain("before"),
    );

    replies = [
      {
        match: (url, method) =>
          method === "POST" &&
          url.endsWith("/sites/site-1/pages/page-1/sections/0/move"),
        status: 200,
        body: { sections: env([FAQ, HERO]) },
      },
      previewReply("<!doctype html>\n<p>after</p>"),
    ];
    fireEvent.click(sectionControls("down")[0]!);
    await waitFor(() =>
      expect(frame().getAttribute("srcdoc")).toContain("after"),
    );

    // A refused gesture leaves the sections untouched — and the pane still.
    const fetched = previewCalls();
    replies = [
      {
        match: (url, method) =>
          method === "POST" &&
          url.endsWith("/sites/site-1/pages/page-1/sections/0/move"),
        status: 422,
        body: { detail: "no section at index 9 (the page has 2)" },
      },
    ];
    fireEvent.click(sectionControls("down")[0]!);
    await screen.findByText("no section at index 9 (the page has 2)");
    expect(previewCalls()).toBe(fetched);
  });

  test("the width toggle flips between desktop and phone", async () => {
    replies = [pageReply([HERO]), previewReply("<!doctype html>\n<p>ok</p>")];
    ui();
    await screen.findByText(strings.sitesSectionHero);
    await openPreview();

    const desktop = screen.getByLabelText(strings.sitesPreviewDesktop);
    const phone = screen.getByLabelText(strings.sitesPreviewMobile);
    expect(desktop.getAttribute("aria-pressed")).toBe("true");
    expect(phone.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(phone);
    expect(desktop.getAttribute("aria-pressed")).toBe("false");
    expect(phone.getAttribute("aria-pressed")).toBe("true");
  });

  test("the desktop splitter adjusts both panels with the keyboard and resets", async () => {
    replies = [pageReply([HERO]), previewReply("<!doctype html><p>ok</p>")];
    ui();
    await screen.findByText(strings.sitesSectionHero);
    await openPreview();

    const splitter = screen.getByRole("separator", {
      name: strings.sitesResizeWorkspace,
    });
    expect(splitter.getAttribute("aria-valuenow")).toBe("34");
    fireEvent.keyDown(splitter, { key: "ArrowRight" });
    expect(splitter.getAttribute("aria-valuenow")).toBe("38");
    fireEvent.keyDown(splitter, { key: "End" });
    expect(splitter.getAttribute("aria-valuenow")).toBe("65");
    fireEvent.doubleClick(splitter);
    expect(splitter.getAttribute("aria-valuenow")).toBe("34");
  });

  test("a failed preview shows its own error while the editor keeps working", async () => {
    replies = [
      pageReply([HERO]),
      {
        match: (url, method) => method === "GET" && url.endsWith("/preview"),
        status: 500,
        body: {},
      },
    ];
    ui();
    await screen.findByText(strings.sitesSectionHero);
    await openPreview();
    expect(await screen.findByText(strings.sitesPreviewFailed)).toBeTruthy();
    // The stack is intact — the preview failing never blocks editing.
    expect(screen.getByText("Fresh bread daily")).toBeTruthy();
  });
});

describe("who can open the page", () => {
  /** The protection read the editor makes on load. */
  function protectionReply(body: unknown): Reply {
    return {
      match: (url, method) =>
        method === "GET" && url.endsWith("/sites/site-1/pages/page-1/password"),
      status: 200,
      body,
    };
  }

  test("the panel reads the page's protection and states it in plain words", async () => {
    replies = [pageReply([HERO]), protectionReply({ protected: false })];
    ui();

    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesPageAccess }),
    );
    expect(
      await screen.findByText(strings.sitesPagePasswordPublic),
    ).toBeTruthy();
    // The preview says nothing about a password while there is none.
    expect(screen.queryByText(strings.sitesPagePasswordPreviewNote)).toBeNull();
    expect(
      calls.some(
        (call) =>
          call.method === "GET" &&
          call.url.endsWith("/sites/site-1/pages/page-1/password"),
      ),
    ).toBe(true);
  });

  test("protecting sends the password on the wire-verified route, and the preview says so", async () => {
    replies = [pageReply([HERO]), protectionReply({ protected: false })];
    ui();
    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesShowPreview }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesPageAccess }),
    );
    await screen.findByText(strings.sitesPagePasswordPublic);

    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesPagePasswordProtect }),
    );
    fireEvent.change(screen.getByLabelText(strings.sitesPagePasswordField), {
      target: { value: "open-sesame-2026" },
    });
    replies = [
      {
        match: (url, method) =>
          method === "PUT" &&
          url.endsWith("/sites/site-1/pages/page-1/password"),
        status: 200,
        body: {
          protected: true,
          pageId: "page-1",
          createdAt: "2026-08-12T09:30:00Z",
          updatedAt: "2026-08-12T09:30:00Z",
        },
      },
    ];
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesPagePasswordProtect }),
    );

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()).toMatchObject({
      method: "PUT",
      body: { password: "open-sesame-2026" },
    });
    expect(lastWrite()?.url).toMatch(
      /\/sites\/site-1\/pages\/page-1\/password$/,
    );
    // The preview pane stops implying the page is simply online.
    expect(
      await screen.findByText(strings.sitesPagePasswordPreviewNote),
    ).toBeTruthy();
  });

  test("taking the password off sends a DELETE only after the second click", async () => {
    replies = [
      pageReply([HERO]),
      protectionReply({
        protected: true,
        pageId: "page-1",
        createdAt: "2026-08-12T09:30:00Z",
        updatedAt: "2026-08-12T09:30:00Z",
      }),
    ];
    ui();

    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesPageAccess }),
    );

    fireEvent.click(
      await screen.findByRole("button", {
        name: strings.sitesPagePasswordRemove,
      }),
    );
    expect(lastWrite()).toBeUndefined();

    replies = [
      {
        match: (url, method) =>
          method === "DELETE" &&
          url.endsWith("/sites/site-1/pages/page-1/password"),
        status: 200,
        body: { protected: false },
      },
    ];
    fireEvent.click(
      screen.getByRole("button", {
        name: strings.sitesPagePasswordRemoveConfirm,
      }),
    );

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()?.method).toBe("DELETE");
    expect(lastWrite()?.url).toMatch(
      /\/sites\/site-1\/pages\/page-1\/password$/,
    );
    expect(
      await screen.findByText(strings.sitesPagePasswordRemoved),
    ).toBeTruthy();
    expect(screen.queryByText(strings.sitesPagePasswordPreviewNote)).toBeNull();
  });
});
