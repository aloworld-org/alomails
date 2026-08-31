// The theme flow's wiring (S1.14): the dialog renders the presets the API
// answered, applying PUTs exactly the typed envelope (absent logo/favicon as
// absent keys), an upload feeds the blob id into the envelope, a refusal
// shows the server's sentence in the open dialog, and — in the editor — an
// applied theme refetches the preview, which depends on the theme. Same
// harness as the sibling suites: the REAL client and views run over a
// recording fake fetch; only Drive uploads are faked at the jmap-client seam.
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
import type { ThemePreset } from "./types";

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
  /** Answers every time instead of being consumed once.
   *
   * For the background reads a screen makes that no test here asserts on. A
   * one-shot reply means each test has to know how many times `SiteView` will
   * fetch its readiness — which is a fact about code these tests are not
   * about, and it changed under them the moment somebody added a refetch. */
  persist?: boolean;
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
  const found = index === -1 ? undefined : (replies[index] as Reply);
  if (found !== undefined && found.persist !== true) replies.splice(index, 1);
  const answer = found ?? { status: 200, body: {} };
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

// The Drive upload seam: the dialog needs a blob id back, nothing more.
const driveUploadBlob = vi.fn(async () => ({
  id: "node-1",
  blobId: "blob-9",
  size: 3,
}));
vi.mock("../jmap", () => ({
  useJmapClient: () => ({ driveUploadBlob }),
}));

const PRESETS: ThemePreset[] = [
  {
    id: "north",
    name: "North",
    palette: {
      background: "#ffffff",
      surface: "#f2f5f8",
      text: "#17212b",
      mutedText: "#4c5866",
      primary: "#1d4ed8",
      onPrimary: "#ffffff",
      border: "#dde3e9",
    },
    typography: {
      headingFamily: "system-ui",
      bodyFamily: "system-ui",
      headingWeight: 700,
    },
  },
  {
    id: "terra",
    name: "Terra",
    palette: {
      background: "#faf6ef",
      surface: "#f2eadd",
      text: "#38291d",
      mutedText: "#6e5844",
      primary: "#9c3d1e",
      onPrimary: "#ffffff",
      border: "#e4d8c6",
    },
    typography: {
      headingFamily: "Georgia",
      bodyFamily: "system-ui",
      headingWeight: 700,
    },
  },
];

/** GET /sites/theme-presets — ambient, so it answers however often it is asked. */
function presetsReply(): Reply {
  return {
    match: (url, method) =>
      method === "GET" && url.endsWith("/sites/theme-presets"),
    status: 200,
    body: { presets: PRESETS },
    persist: true,
  };
}

/** GET /sites/site-1 with a stored theme. */
/** What `/translation-readiness` answers for a one-language site.
 *
 * Stubbed even though these tests are about the theme dialog, because
 * `SiteView` loads the site, its pages and its readiness together and renders
 * all three: an unstubbed endpoint falls through to the catch-all `{}`, and
 * reducing over a `languages` key that is not there takes the whole view down
 * before the dialog can open.
 *
 * Ambient for the same reason it is stubbed at all — the count of times the
 * view reads it is not this file's business. */
function readinessReply(): Reply {
  return {
    match: (url, method) =>
      method === "GET" && url.endsWith("/translation-readiness"),
    status: 200,
    body: { defaultLocale: "en", totalPages: 1, languages: [] },
    persist: true,
  };
}

function siteReply(theme: Record<string, unknown>): Reply {
  return {
    match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
    status: 200,
    body: {
      id: "site-1",
      name: "Alpha Bakery",
      subdomain: "alpha",
      status: "draft",
      // Required by `SiteDetail`, and omitting them is what made these four
      // tests fail: `SiteView` maps over `enabledLocales` while rendering, so
      // a fixture missing it threw before the theme dialog ever opened. The
      // error read as "unable to find the text Theme", which is the failure a
      // crashed render always looks like.
      defaultLocale: "en",
      enabledLocales: ["en"],
      canManageCollaborators: true,
      theme,
      publish: null,
    },
    // Ambient: the view loads the site, the dialog loads it again, and both
    // want the same body. Every GET fixture in this file is ambient for that
    // reason — only the writes below stay one-shot, where the order is the
    // thing under test.
    persist: true,
  };
}

/** GET /sites/site-1/pages/page-1 — the editor's load. */
function pageReply(): Reply {
  return {
    match: (url, method) =>
      method === "GET" && url.endsWith("/sites/site-1/pages/page-1"),
    status: 200,
    body: {
      id: "page-1",
      slug: "",
      title: "Welcome",
      home: true,
      sections: {
        schema_version: 1,
        sections: [{ type: "hero", heading: "Fresh bread daily" }],
      },
    },
    persist: true,
  };
}

function ui(path: string) {
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

async function openThemeDialogFromSiteView(theme: Record<string, unknown>) {
  replies.push(siteReply(theme));
  ui("/sites/site-1");
  fireEvent.click(await screen.findByText(strings.sitesTheme));
  expect(await screen.findByText(strings.brandingAccentsTitle)).toBeTruthy();
}

beforeEach(() => {
  calls.length = 0;
  // The two background reads every screen here makes, seeded rather than
  // pushed per test: a test that forgets one does not fail on the endpoint it
  // forgot, it fails on "unable to find the text Theme" from a crashed render,
  // which sends you looking in the dialog. Tests that assign to `replies`
  // wholesale keep them by spreading this in.
  replies = [readinessReply(), presetsReply()];
  fakeFetch.mockClear();
  driveUploadBlob.mockClear();
});

afterEach(cleanup);

const northWithWorkspaceBrand = {
  background: "#FFFFFF", text: "#17212B", border: "#DDE3E9",
  accent_1: "#E76F51", accent_2: "#102A43", accent_3: "#F2F5F8",
  accent_4: "#17212B", accent_5: "#FFFFFF",
};

const terraWithWorkspaceBrand = {
  background: "#FAF6EF", text: "#38291D", border: "#E4D8C6",
  accent_1: "#E76F51", accent_2: "#102A43", accent_3: "#F2EADD",
  accent_4: "#38291D", accent_5: "#FAF6EF",
};

describe("the theme dialog", () => {
  test("opens directly on brand colors without showing preset cards", async () => {
    await openThemeDialogFromSiteView({});
    expect(screen.queryByRole("radio", { name: "North" })).toBeNull();
    expect(screen.queryByRole("radio", { name: "Terra" })).toBeNull();
    expect(
      screen.queryByLabelText(strings.sitesThemeHexValue(strings.sitesThemeBackgroundColor)),
    ).toBeNull();
    expect(
      screen.queryByLabelText(strings.sitesThemeHexValue(strings.sitesThemeTextColor)),
    ).toBeNull();
    expect(
      screen.queryByLabelText(strings.sitesThemeHexValue(strings.sitesThemeBorderColor)),
    ).toBeNull();
    fireEvent.click(screen.getByText(strings.sitesThemeApply));
    await waitFor(() => {
      expect(lastWrite()).toBeTruthy();
    });
    const write = lastWrite();
    expect(write?.method).toBe("PUT");
    expect(write?.url.endsWith("/sites/site-1/theme")).toBe(true);
    // The exact envelope: absent logo/favicon are ABSENT keys, not nulls.
    expect(write?.body).toEqual({
      schema_version: 1,
      preset: "north",
      colors: northWithWorkspaceBrand,
    });
  });

  test("an uploaded logo goes through Drive and lands in the envelope", async () => {
    await openThemeDialogFromSiteView({});
    fireEvent.click(
      screen.getAllByText(strings.sitesThemeUpload)[0] as HTMLElement,
    );
    const file = new File(["png"], "logo.png", { type: "image/png" });
    const pickers = document.querySelectorAll('input[type="file"]');
    fireEvent.change(pickers[0] as HTMLInputElement, {
      target: { files: [file] },
    });
    expect(await screen.findByText(strings.sitesThemeSet)).toBeTruthy();
    expect(driveUploadBlob).toHaveBeenCalledWith(null, null, file);
    fireEvent.click(screen.getByText(strings.sitesThemeApply));
    await waitFor(() => {
      expect(lastWrite()).toBeTruthy();
    });
    expect(lastWrite()?.body).toEqual({
      schema_version: 1,
      preset: "north",
      logo: "blob-9",
      colors: northWithWorkspaceBrand,
    });
  });

  test("imports the full reusable workspace brand palette", async () => {
    await openThemeDialogFromSiteView({});
    fireEvent.click(screen.getByText(strings.sitesThemeApply));
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()?.body).toEqual({
      schema_version: 1,
      preset: "north",
      colors: {
        background: "#FFFFFF", text: "#17212B", border: "#DDE3E9",
        accent_1: "#E76F51", accent_2: "#102A43", accent_3: "#F2F5F8",
        accent_4: "#17212B", accent_5: "#FFFFFF",
      },
    });
  });

  test("a stored logo prefills; removing it drops the key from the envelope", async () => {
    await openThemeDialogFromSiteView({
      schema_version: 1,
      preset: "terra",
      logo: "blob-old",
    });
    expect(screen.getByText(strings.sitesThemeSet)).toBeTruthy();
    expect(screen.queryByRole("radio", { name: "Terra" })).toBeNull();
    fireEvent.click(screen.getByLabelText(strings.sitesThemeRemove));
    fireEvent.click(screen.getByText(strings.sitesThemeApply));
    await waitFor(() => {
      expect(lastWrite()).toBeTruthy();
    });
    expect(lastWrite()?.body).toEqual({
      schema_version: 1,
      preset: "terra",
      colors: terraWithWorkspaceBrand,
    });
  });

  test("a refusal shows the server's sentence and the dialog stays open", async () => {
    await openThemeDialogFromSiteView({});
    replies.push({
      match: (url, method) =>
        method === "PUT" && url.endsWith("/sites/site-1/theme"),
      status: 422,
      body: { detail: "theme: preset is not a shipped theme preset" },
    });
    fireEvent.click(screen.getByText(strings.sitesThemeApply));
    expect(
      await screen.findByText("theme: preset is not a shipped theme preset"),
    ).toBeTruthy();
    // Still open: the brand-color controls remain on screen.
    expect(screen.getByText(strings.brandingAccentsTitle)).toBeTruthy();
  });
});

describe("the editor's preview after a theme change", () => {
  test("applying a theme refetches the preview document", async () => {
    replies.push(pageReply(), siteReply({}));
    ui("/sites/site-1/pages/page-1");
    expect(await screen.findByText("Welcome")).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesShowPreview }),
    );
    await waitFor(() => {
      expect(previewFetches()).toBe(1);
    });
    fireEvent.click(screen.getByText(strings.sitesTheme));
    expect(await screen.findByText(strings.brandingAccentsTitle)).toBeTruthy();
    fireEvent.click(screen.getByText(strings.sitesThemeApply));
    // The accepted envelope closes the dialog and the preview refetches.
    await waitFor(() => {
      expect(previewFetches()).toBe(2);
    });
  });

  function previewFetches(): number {
    return calls.filter((c) => c.method === "GET" && c.url.endsWith("/preview"))
      .length;
  }
});

describe("section image upload", () => {
  test("uploading in the hero form fills the blob id the section then saves", async () => {
    replies.push(pageReply(), siteReply({}));
    ui("/sites/site-1/pages/page-1");
    expect(await screen.findByText("Fresh bread daily")).toBeTruthy();
    fireEvent.click(sectionControls("edit")[0]!);
    expect(await screen.findByText(strings.sitesSaveSection)).toBeTruthy();
    fireEvent.click(screen.getByText(strings.sitesUploadImage));
    const file = new File(["png"], "drum.png", { type: "image/png" });
    const pickers = document.querySelectorAll('input[type="file"]');
    fireEvent.change(pickers[0] as HTMLInputElement, {
      target: { files: [file] },
    });
    await waitFor(() => {
      expect(driveUploadBlob).toHaveBeenCalledWith(null, null, file);
      expect(screen.getByDisplayValue("blob-9")).toBeTruthy();
    });
    replies.push({
      match: (url, method) =>
        method === "PUT" &&
        url.endsWith("/sites/site-1/pages/page-1/sections/0"),
      status: 200,
      body: { sections: { schema_version: 1, sections: [] } },
    });
    fireEvent.click(screen.getByText(strings.sitesSaveSection));
    await waitFor(() => {
      expect(lastWrite()).toBeTruthy();
    });
    const section = (
      lastWrite()?.body as { section: { image?: { blob_id: string } } }
    ).section;
    expect(section.image?.blob_id).toBe("blob-9");
  });
});
