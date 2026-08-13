// The section stack, from the keyboard (S2.16b2).
//
// Found by opening the page editor in a real browser at 360px and driving it
// with Tab and Enter, which is the half of the wave review S2.16b could not
// do by reading source:
//
//   1. A stack of five sections was twenty buttons called "Move up", "Move
//      down", "Edit section" and "Delete section" — four names, repeated,
//      with nothing in any of them to say WHICH section they act on. Tabbing
//      through it, or listing the buttons in a screen-reader rotor, gives no
//      way to tell the hero's delete from the footer's.
//   2. Pressing "move down" moved the section and then dropped focus on
//      `<body>`: a move replaces the whole list with the server's answer, so
//      React unmounts the row that had focus. Measured in the browser, moving
//      one section two places cost ten Tab presses to get back to the button
//      each time — a reorder nobody can do twice.
//   3. Nothing said the move had happened. Reordering is the one edit on this
//      screen whose only result is the stack reflowing, which a reader who
//      cannot see it never learns about.
//
// Same harness as PageEditor.test.tsx: the real client and views run, only
// the network is faked.
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
import { kindLabel } from "./sectionInfo";

interface Reply {
  match: (url: string, method: string) => boolean;
  status: number;
  body: unknown;
}

let replies: Reply[] = [];

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
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

const HERO: Section = { type: "hero", heading: "Fresh bread daily" };
const FAQ: Section = {
  type: "faq",
  items: [{ question: "When?", answer: "Every day." }],
};
const FOOTER: Section = { type: "footer", links: [] };

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

/** The server's answer to "move the section at `from` to `to`". */
function moveReply(from: number, sections: Section[]): Reply {
  return {
    match: (url, method) =>
      method === "POST" &&
      url.endsWith(`/sites/site-1/pages/page-1/sections/${from}/move`),
    status: 200,
    body: { sections: env(sections) },
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

function controls(control: string): HTMLElement[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      `[data-section-control="${control}"]`,
    ),
  ];
}

beforeEach(() => {
  replies = [];
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the section stack names its controls", () => {
  test("every button says which section it acts on", async () => {
    replies = [pageReply([HERO, FAQ, FOOTER])];
    ui();
    await screen.findByText(strings.sitesSectionHero);

    // The name of each control, row by row: no two rows share one.
    for (const control of ["up", "down", "edit", "delete"]) {
      const names = controls(control).map((el) =>
        el.getAttribute("aria-label"),
      );
      expect(names.length).toBe(3);
      expect(new Set(names).size).toBe(3);
    }
    expect(
      screen.getByLabelText(strings.sitesDeleteSection(kindLabel("hero"))),
    ).toBeTruthy();
    expect(
      screen.getByLabelText(strings.sitesMoveDown(kindLabel("footer"))),
    ).toBeTruthy();
  });
});

describe("reordering from the keyboard", () => {
  test("focus follows the section that moved, not the row it left", async () => {
    replies = [pageReply([HERO, FAQ, FOOTER])];
    ui();
    await screen.findByText(strings.sitesSectionHero);

    const pressed = controls("down")[0]!;
    pressed.focus();
    replies = [moveReply(0, [FAQ, HERO, FOOTER])];
    fireEvent.click(pressed);

    await waitFor(() =>
      expect(
        controls("down")[0]!.getAttribute("aria-label"),
      ).toBe(strings.sitesMoveDown(kindLabel("faq"))),
    );
    // The hero is now the second row, and the caret is on ITS move-down —
    // so a second press moves the same section again.
    await waitFor(() =>
      expect(document.activeElement?.getAttribute("aria-label")).toBe(
        strings.sitesMoveDown(kindLabel("hero")),
      ),
    );
  });

  test("the last row has no move-down, so focus falls to its sibling", async () => {
    replies = [pageReply([HERO, FAQ, FOOTER])];
    ui();
    await screen.findByText(strings.sitesSectionHero);

    // Moving the middle section down puts it last, where the button that was
    // pressed is disabled.
    const pressed = controls("down")[1]!;
    pressed.focus();
    replies = [moveReply(1, [HERO, FOOTER, FAQ])];
    fireEvent.click(pressed);

    await waitFor(() =>
      expect(document.activeElement?.getAttribute("aria-label")).toBe(
        strings.sitesMoveUp(kindLabel("faq")),
      ),
    );
    expect(document.activeElement).toHaveProperty("disabled", false);
  });

  test("the move is announced, with the section and where it landed", async () => {
    replies = [pageReply([HERO, FAQ, FOOTER])];
    ui();
    await screen.findByText(strings.sitesSectionHero);

    replies = [moveReply(0, [FAQ, HERO, FOOTER])];
    fireEvent.click(controls("down")[0]!);

    const said = strings.sitesSectionMoved(kindLabel("hero"), 2, 3);
    await waitFor(() => expect(screen.getByText(said)).toBeTruthy());
    // In the accessibility tree and out of the picture: a live region that is
    // drawn would be a second, silent copy of the stack's own state.
    const region = screen.getByText(said);
    expect(region.getAttribute("role")).toBe("status");
  });
});
