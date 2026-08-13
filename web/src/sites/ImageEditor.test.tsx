// The image editor (S2.07c): framing a photograph, describing it, and the
// AI draft of that description.
//
// Same harness as the other sites suites — the real client, the real views,
// only the network faked. Pointer drags are deliberately NOT tested here:
// jsdom lays nothing out, so every `getBoundingClientRect()` is zero and a
// simulated drag would prove only that zero arithmetic works. The rules a
// drag relies on live in `imageGeometry.test.ts`, where they can be tested
// for real; what this file pins is the wiring, the keyboard path (which is
// the accessible one, and the one a layout-free DOM can honestly exercise)
// and the propose-then-approve contract.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitesModule } from "./SitesModule";
import { SECTIONS_SCHEMA_VERSION } from "./sections";
import type { Section, SectionImage, SectionsEnvelope } from "./sections";

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
  const answer = index === -1 ? { status: 200, body: {} } : (replies.splice(index, 1)[0] as Reply);
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

const env = (sections: Section[]): SectionsEnvelope => ({
  schema_version: SECTIONS_SCHEMA_VERSION,
  sections,
});

const photo = (over: Partial<SectionImage> = {}): SectionImage => ({
  blob_id: "Ph0t0aaaaaaaaaaaaaaaa1",
  alt: "Loaves cooling on a rack",
  ...over,
});

const heroWith = (image: SectionImage): Section => ({
  type: "hero",
  heading: "Fresh bread daily",
  image,
});

function pageReply(sections: Section[]): Reply {
  return {
    match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/pages/page-1"),
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

function ui() {
  return render(
    <MemoryRouter initialEntries={["/sites/site-1/pages/page-1"]}>
      <Routes>
        <Route path="/sites/*" element={<SitesModule />} />
      </Routes>
    </MemoryRouter>,
  );
}

const lastWrite = (): Call | undefined => calls.filter((c) => c.method !== "GET").at(-1);

/** Opens the prop form of the page's only section. */
async function openTheSection() {
  await screen.findByText(strings.sitesSectionHero);
  fireEvent.click(sectionControls("edit")[0]!);
  await screen.findByLabelText(strings.sitesFieldImageAlt);
}

/** Answers the next section write with `stored`, so the dialog closes. */
function acceptWrite(stored: Section) {
  replies = [
    {
      match: (url, method) =>
        method === "PUT" && url.endsWith("/sites/site-1/pages/page-1/sections/0"),
      status: 200,
      body: { sections: env([stored]) },
    },
  ];
}

const savedSection = (): Section => (lastWrite()!.body as { section: Section }).section;

beforeEach(() => {
  calls.length = 0;
  replies = [];
  fakeFetch.mockClear();
  // jsdom has no object URLs; the editor asks before using them, so the test
  // says yes and gets the full control rather than the no-preview state.
  Object.defineProperty(URL, "createObjectURL", { value: () => "blob:photo", configurable: true });
  Object.defineProperty(URL, "revokeObjectURL", { value: () => undefined, configurable: true });
});

afterEach(cleanup);

describe("framing a picture", () => {
  test("the control reads the stored frame back as percentages of the picture", async () => {
    replies = [
      pageReply([
        heroWith(photo({ crop: { x_bp: 1_250, y_bp: 0, width_bp: 7_500, height_bp: 10_000 } })),
      ]),
    ];
    ui();
    await openTheSection();

    expect(screen.getByLabelText(strings.sitesImageFrameWidth)).toHaveProperty("value", "75");
    expect(screen.getByLabelText(strings.sitesImageFrameLeft)).toHaveProperty("value", "13");
    // The frame itself says where it is, for anyone who cannot see it.
    expect(screen.getByRole("button", { name: strings.sitesImageFrameAt(75, 100, 13, 0) })).toBeTruthy();
  });

  test("a typed width is saved as a crop the server would accept", async () => {
    replies = [pageReply([heroWith(photo())])];
    ui();
    await openTheSection();

    fireEvent.change(screen.getByLabelText(strings.sitesImageFrameWidth), {
      target: { value: "50" },
    });
    const framed = heroWith(photo({ crop: { x_bp: 0, y_bp: 0, width_bp: 5_000, height_bp: 10_000 } }));
    acceptWrite(framed);
    fireEvent.click(screen.getByRole("button", { name: strings.sitesSaveSection }));

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(savedSection()).toEqual(framed);
  });

  test("arrow keys move the frame, and the readout follows", async () => {
    replies = [
      pageReply([
        heroWith(photo({ crop: { x_bp: 0, y_bp: 0, width_bp: 5_000, height_bp: 5_000 } })),
      ]),
    ];
    ui();
    await openTheSection();

    const frame = screen.getByRole("button", { name: strings.sitesImageFrameAt(50, 50, 0, 0) });
    fireEvent.keyDown(frame, { key: "ArrowRight" });
    fireEvent.keyDown(frame, { key: "ArrowDown" });
    // Left edge cannot go past the picture, so this one changes nothing.
    fireEvent.keyDown(frame, { key: "ArrowLeft" });
    fireEvent.keyDown(frame, { key: "ArrowLeft" });

    acceptWrite(heroWith(photo()));
    fireEvent.click(screen.getByRole("button", { name: strings.sitesSaveSection }));
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(savedSection()).toMatchObject({
      image: { crop: { x_bp: 0, y_bp: 100, width_bp: 5_000, height_bp: 5_000 } },
    });
  });

  test("the focal point is set with the keyboard and stays inside its frame", async () => {
    replies = [
      pageReply([
        heroWith(photo({ crop: { x_bp: 0, y_bp: 0, width_bp: 2_000, height_bp: 10_000 } })),
      ]),
    ];
    ui();
    await openTheSection();

    // Unstated, it reads as the centre of the crop — not of the source.
    const marker = screen.getByRole("button", { name: strings.sitesImageFocalAt(10, 50) });
    fireEvent.keyDown(marker, { key: "ArrowRight" });
    fireEvent.keyDown(marker, { key: "ArrowRight" });

    acceptWrite(heroWith(photo()));
    fireEvent.click(screen.getByRole("button", { name: strings.sitesSaveSection }));
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(savedSection()).toMatchObject({ image: { focal: { x_bp: 1_200, y_bp: 5_000 } } });
  });

  test("using the whole picture stores no geometry at all", async () => {
    const framed = photo({
      crop: { x_bp: 1_000, y_bp: 1_000, width_bp: 5_000, height_bp: 5_000 },
      focal: { x_bp: 2_000, y_bp: 2_000 },
    });
    replies = [pageReply([heroWith(framed)])];
    ui();
    await openTheSection();

    fireEvent.click(screen.getByRole("button", { name: strings.sitesImageWholePicture }));
    acceptWrite(heroWith(photo()));
    fireEvent.click(screen.getByRole("button", { name: strings.sitesSaveSection }));

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    // Absent, not "the whole picture written out as numbers".
    expect(savedSection()).toEqual(heroWith(photo()));
  });

  test("replacing the picture drops the frame of the one before it", async () => {
    replies = [
      pageReply([
        heroWith(photo({ crop: { x_bp: 0, y_bp: 0, width_bp: 4_000, height_bp: 4_000 } })),
      ]),
    ];
    ui();
    await openTheSection();

    fireEvent.change(screen.getByLabelText(strings.sitesFieldImageId), {
      target: { value: "Oth3rPh0t0aaaaaaaaaaa2" },
    });
    acceptWrite(heroWith(photo()));
    fireEvent.click(screen.getByRole("button", { name: strings.sitesSaveSection }));

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(savedSection()).toEqual(heroWith(photo({ blob_id: "Oth3rPh0t0aaaaaaaaaaa2" })));
  });

  test("a picture that cannot be shown still frames by the numbers", async () => {
    // The one state where the surface is gone: no object URLs at all.
    Object.defineProperty(URL, "createObjectURL", { value: undefined, configurable: true });
    replies = [pageReply([heroWith(photo())])];
    ui();
    await openTheSection();

    expect(screen.getByText(strings.sitesImageNoPreview)).toBeTruthy();
    expect(screen.getByLabelText(strings.sitesImageFrameWidth)).toHaveProperty("value", "100");
  });
});

describe("describing a picture", () => {
  test("an undescribed picture says so", async () => {
    replies = [pageReply([heroWith(photo({ alt: "" }))])];
    ui();
    await openTheSection();
    expect(screen.getByText(strings.sitesImageAltMissing)).toBeTruthy();
  });

  test("marking a picture decorative clears its description and locks the field", async () => {
    replies = [pageReply([heroWith(photo())])];
    ui();
    await openTheSection();

    fireEvent.click(screen.getByLabelText(strings.sitesImageDecorative));
    const alt = screen.getByLabelText(strings.sitesFieldImageAlt);
    expect(alt).toHaveProperty("value", "");
    expect(alt).toHaveProperty("disabled", true);
    // No description to write, so nothing offers to write one.
    expect(screen.queryByText(strings.sitesImageAltMissing)).toBeNull();
    expect(screen.queryByRole("button", { name: strings.sitesAiAltWrite })).toBeNull();

    const decorative = heroWith(photo({ alt: "", decorative: true }));
    acceptWrite(decorative);
    fireEvent.click(screen.getByRole("button", { name: strings.sitesSaveSection }));
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(savedSection()).toEqual(decorative);
  });
});

describe("the AI draft of a description", () => {
  const proposal = {
    schema_version: 1,
    operations: [
      {
        op: "rewrite_copy",
        target: { index: 0, type: "hero" },
        pointer: "/image/alt",
        text: "Bread cooling after the morning bake",
      },
    ],
  };

  test("it names the exact field, is only a proposal, and says it has not seen the picture", async () => {
    replies = [pageReply([heroWith(photo({ alt: "" }))])];
    ui();
    await openTheSection();

    replies = [
      {
        match: (url, method) => method === "POST" && url.endsWith("/pages/page-1/ai-edits"),
        status: 200,
        body: { proposal, previewHtml: "<html></html>" },
      },
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.sitesAiAltWrite }));

    expect(await screen.findByText("Bread cooling after the morning bake")).toBeTruthy();
    expect(screen.getByText(strings.sitesAiAltUnseen)).toBeTruthy();
    expect(lastWrite()).toMatchObject({
      method: "POST",
      body: {
        copy: {
          target: { index: 0, type: "hero" },
          pointer: "/image/alt",
          action: "alt_text",
        },
      },
    });
    // Proposing wrote nothing: the only call so far is the proposal itself.
    expect(calls.filter((c) => c.method === "PUT")).toHaveLength(0);
  });

  test("discarding leaves the picture undescribed; approving is what writes it", async () => {
    replies = [pageReply([heroWith(photo({ alt: "" }))])];
    ui();
    await openTheSection();

    replies = [
      {
        match: (url, method) => method === "POST" && url.endsWith("/pages/page-1/ai-edits"),
        status: 200,
        body: { proposal, previewHtml: "<html></html>" },
      },
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.sitesAiAltWrite }));
    await screen.findByText("Bread cooling after the morning bake");

    fireEvent.click(screen.getByRole("button", { name: strings.sitesAiDiscard }));
    expect(screen.queryByText("Bread cooling after the morning bake")).toBeNull();
    expect(calls.filter((c) => c.method === "PUT")).toHaveLength(0);

    replies = [
      {
        match: (url, method) => method === "POST" && url.endsWith("/pages/page-1/ai-edits"),
        status: 200,
        body: { proposal, previewHtml: "<html></html>" },
      },
      {
        match: (url, method) => method === "PUT" && url.endsWith("/pages/page-1/ai-edits"),
        status: 200,
        body: {
          sections: env([heroWith(photo({ alt: "Bread cooling after the morning bake" }))]),
        },
      },
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.sitesAiAltWrite }));
    await screen.findByText("Bread cooling after the morning bake");
    fireEvent.click(screen.getByRole("button", { name: strings.sitesAiApprove }));

    await waitFor(() => expect(lastWrite()?.method).toBe("PUT"));
    expect(lastWrite()!.body).toEqual({ proposal });
  });

  test("a section being added offers no draft — there is no stored field to aim at", async () => {
    replies = [pageReply([])];
    ui();
    fireEvent.click((await screen.findAllByRole("button", { name: strings.sitesAddSection }))[0]!);
    fireEvent.click(
      screen.getByRole("button", {
        name: `${strings.sitesSectionHero} ${strings.sitesSectionHeroDesc}`,
      }),
    );

    await screen.findByLabelText(strings.sitesFieldImageAlt);
    expect(screen.queryByRole("button", { name: strings.sitesAiAltWrite })).toBeNull();
  });
});
