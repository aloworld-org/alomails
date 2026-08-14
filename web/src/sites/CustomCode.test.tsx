// The custom-code block in the editor (S2.14b): that the picker offers it,
// that the form sends exactly the block the store's write gate expects — the
// three parts apart, the capabilities default-denied, the height authored —
// that a script and the permission that runs it are never saved apart, that
// the boundary is stated before anything is typed, and that a refusal is shown
// in the words the server used. Same harness as PageEditor.test.tsx: the REAL
// client and views run, only the network is fake.
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
import { SECTIONS_SCHEMA_VERSION } from "./sections";
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

/** A stored block, as the wire carries one back. */
const TIMER: Section = {
  type: "custom_code",
  heading: "Roast timer",
  title: "A timer counting down the current roast",
  html: '<p id="left">12:00</p>',
  css: "#left { font-size: 3rem; }",
  js: "document.getElementById('left');",
  capabilities: { scripts: true, inline_images: false },
  height_px: 220,
};

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

function ui() {
  return render(
    <MemoryRouter initialEntries={["/sites/site-1/pages/page-1"]}>
      <Routes>
        <Route path="/sites/*" element={<SitesModule />} />
      </Routes>
    </MemoryRouter>,
  );
}

function lastWrite(): Call | undefined {
  return calls.filter((c) => c.method !== "GET").at(-1);
}

/** Opens the palette and chooses the custom-code tile (whose code is never
 *  seeded, so it always opens the prop form). */
async function openBlockForm() {
  fireEvent.click(
    (await screen.findAllByRole("button", { name: strings.sitesAddSection }))[0]!,
  );
  const tile = document.querySelector<HTMLElement>(
    '[data-palette-tile="custom_code"]',
  );
  if (tile === null) throw new Error("no custom_code tile in the palette");
  fireEvent.click(tile);
}

beforeEach(() => {
  calls.length = 0;
  replies = [];
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("writing a custom-code block", () => {
  test("the boundary is stated before the first field, not discovered later", async () => {
    replies = [pageReply([])];
    ui();
    await openBlockForm();

    expect(
      screen.getByText(strings.sitesCustomCodeBoundaryTitle),
    ).toBeTruthy();
    // The three facts that decide whether this block is the right tool at
    // all: it is sealed off, it has no network, and nobody vets it.
    expect(
      screen.getByText(strings.sitesCustomCodeBoundarySealed),
    ).toBeTruthy();
    expect(
      screen.getByText(strings.sitesCustomCodeBoundaryNoNetwork),
    ).toBeTruthy();
    expect(screen.getByText(strings.sitesCustomCodeBoundaryYours)).toBeTruthy();
  });

  test("saving POSTs the three parts apart, with both permissions denied", async () => {
    replies = [pageReply([])];
    ui();
    await openBlockForm();

    fireEvent.change(screen.getByLabelText(strings.sitesFieldHeading), {
      target: { value: "  Opening hours  " },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesCustomCodeFrameTitle), {
      target: { value: "This week's opening hours" },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesCustomCodeHtml), {
      target: { value: "<p>Open until six</p>\n" },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesCustomCodeCss), {
      target: { value: "p { font-weight: 700; }" },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesCustomCodeHeight), {
      target: { value: "180" },
    });

    replies = [
      {
        match: (url, method) =>
          method === "POST" &&
          url.endsWith("/sites/site-1/pages/page-1/sections"),
        status: 200,
        body: { sections: env([TIMER]) },
      },
    ];
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesSaveSection }),
    );

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    // A script field was never shown, so no `js` key; both capabilities are
    // sent as denied rather than left out, which is what the write gate reads
    // as least privilege.
    expect(lastWrite()!.body).toEqual({
      section: {
        type: "custom_code",
        heading: "Opening hours",
        title: "This week's opening hours",
        html: "<p>Open until six</p>",
        css: "p { font-weight: 700; }",
        capabilities: { scripts: false, inline_images: false },
        height_px: 180,
      },
      index: 0,
    });
  });

  test("a script travels with the permission that runs it, and never without", async () => {
    replies = [pageReply([])];
    ui();
    await openBlockForm();

    // The script field does not exist until the block is allowed to run one.
    expect(screen.queryByLabelText(strings.sitesCustomCodeJs)).toBeNull();
    fireEvent.change(screen.getByLabelText(strings.sitesCustomCodeFrameTitle), {
      target: { value: "A counter" },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesCustomCodeHtml), {
      target: { value: "<output id=n>0</output>" },
    });
    fireEvent.click(screen.getByLabelText(strings.sitesCustomCodeScripts));

    // Granted but empty: the form says so in the same words the server would.
    expect(screen.getByText(strings.sitesCustomCodeScriptMissing)).toBeTruthy();
    fireEvent.change(screen.getByLabelText(strings.sitesCustomCodeJs), {
      target: { value: "let n = 0;" },
    });
    expect(screen.queryByText(strings.sitesCustomCodeScriptMissing)).toBeNull();

    replies = [
      {
        match: (url, method) =>
          method === "POST" &&
          url.endsWith("/sites/site-1/pages/page-1/sections"),
        status: 200,
        body: { sections: env([TIMER]) },
      },
    ];
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesSaveSection }),
    );
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()!.body).toEqual({
      section: {
        type: "custom_code",
        title: "A counter",
        html: "<output id=n>0</output>",
        js: "let n = 0;",
        capabilities: { scripts: true, inline_images: false },
        height_px: 320,
      },
      index: 0,
    });
  });

  test("taking the permission away says the script goes with it, before saving", async () => {
    replies = [pageReply([TIMER])];
    ui();
    await screen.findByText(strings.sitesSectionCustomCode);
    fireEvent.click(sectionControls("edit")[0]!);

    // The stored block opens with its script and the permission that runs it.
    expect(
      (screen.getByLabelText(strings.sitesCustomCodeJs) as HTMLTextAreaElement)
        .value,
    ).toBe("document.getElementById('left');");
    fireEvent.click(screen.getByLabelText(strings.sitesCustomCodeScripts));
    expect(screen.getByText(strings.sitesCustomCodeScriptDropped)).toBeTruthy();

    replies = [
      {
        match: (url, method) =>
          method === "PUT" &&
          url.endsWith("/sites/site-1/pages/page-1/sections/0"),
        status: 200,
        body: { sections: env([TIMER]) },
      },
    ];
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesSaveSection }),
    );
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    // Saved without the script rather than with a script nothing may run.
    expect(lastWrite()!.body).toEqual({
      section: {
        type: "custom_code",
        heading: "Roast timer",
        title: "A timer counting down the current roast",
        html: '<p id="left">12:00</p>',
        css: "#left { font-size: 3rem; }",
        capabilities: { scripts: false, inline_images: false },
        height_px: 220,
      },
    });
  });

  test("the counter reads the block in bytes and says when it is too long", async () => {
    replies = [pageReply([])];
    ui();
    await openBlockForm();

    // Bytes, not characters — the cap the server enforces is a byte cap, and
    // one accented letter is two of them.
    fireEvent.change(screen.getByLabelText(strings.sitesCustomCodeHtml), {
      target: { value: "<p>café</p>" },
    });
    expect(screen.getByText(strings.sitesCustomCodeBytes(12, 16_384))).toBeTruthy();

    fireEvent.change(screen.getByLabelText(strings.sitesCustomCodeCss), {
      target: { value: "a".repeat(8_200) },
    });
    expect(
      screen.getByText(strings.sitesCustomCodeBytesOver(8_200, 8_192)),
    ).toBeTruthy();
    // Over budget is said, never enforced here: the save still goes, and the
    // server's own sentence is the authority.
    expect(
      (
        screen.getByRole("button", {
          name: strings.sitesSaveSection,
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
  });

  test("the server's refusal is shown verbatim, with everything typed still there", async () => {
    replies = [pageReply([])];
    ui();
    await openBlockForm();

    fireEvent.change(screen.getByLabelText(strings.sitesCustomCodeFrameTitle), {
      target: { value: "A video" },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesCustomCodeHtml), {
      target: { value: '<iframe src="https://video.example"></iframe>' },
    });
    const refusal =
      "custom_code section: html may not contain <iframe>: a block may not frame anything; it is already inside a frame";
    replies = [
      {
        match: (url, method) =>
          method === "POST" &&
          url.endsWith("/sites/site-1/pages/page-1/sections"),
        status: 422,
        body: { detail: refusal },
      },
    ];
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesSaveSection }),
    );

    expect(await screen.findByText(refusal)).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(
      (
        screen.getByLabelText(
          strings.sitesCustomCodeHtml,
        ) as HTMLTextAreaElement
      ).value,
    ).toBe('<iframe src="https://video.example"></iframe>');
  });

  test("a stored block reads on its card by the name visitors are told", async () => {
    replies = [pageReply([{ ...TIMER, heading: undefined }])];
    ui();
    await screen.findByText(strings.sitesSectionCustomCode);
    expect(
      screen.getByText("A timer counting down the current roast"),
    ).toBeTruthy();
  });
});
