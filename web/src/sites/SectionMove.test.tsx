// Moving a section on the page (ADR 0042, S3.01b).
//
// The first block is the arithmetic the preview document deliberately does not
// do: it reports the neighbour a section was dropped above, and turning that
// into the destination index of a splice is the one place this can go subtly
// wrong — off by one only when dragging downwards, which is exactly the kind
// of bug that ships. Every case is pinned against a plain splice of the same
// list.
//
// The rest drives the real editor with the real API client, faking only the
// network and the message the preview frame posts, and proves the three
// properties the item is for: the gesture stores one move, it is the same
// request the stack's own buttons make, and one ⌘Z takes it back.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitesModule } from "./SitesModule";
import { SECTIONS_SCHEMA_VERSION } from "./sections";
import type { Section, SectionsEnvelope } from "./sections";
import {
  moveDestination,
  readSectionMoveMessage,
  withSectionMoved,
} from "./sectionMove";
import {
  emptyEditHistory,
  invertEdit,
  recordEdit,
  undoEdit,
} from "./editHistory";

const HERO: Section = { type: "hero", heading: "Fresh bread daily" };
const FAQ: Section = {
  type: "faq",
  items: [{ question: "When are you open?", answer: "Every day." }],
};
const CTA: Section = {
  type: "cta",
  heading: "Come by",
  button: { label: "Visit us", href: "/visit" },
};
const SECTIONS: Section[] = [HERO, FAQ, CTA];

function env(sections: Section[]): SectionsEnvelope {
  return { schema_version: SECTIONS_SCHEMA_VERSION, sections };
}

describe("the neighbour a section was dropped above becomes a destination", () => {
  /** The same splice both doors perform, written out independently of the
   *  code under test. */
  function spliced(from: number, to: number): string[] {
    const kinds: string[] = SECTIONS.map((section) => section.type);
    const [moved] = kinds.splice(from, 1) as [string];
    kinds.splice(to, 0, moved);
    return kinds;
  }

  test("dropping above a later section lands one earlier, because removal shifted it", () => {
    // The hero (0) dropped above the cta (2): after taking the hero out, the
    // cta sits at 1, so the hero goes back in at 1 — between faq and cta.
    expect(moveDestination(3, 0, 2)).toBe(1);
    expect(spliced(0, 1)).toEqual(["faq", "hero", "cta"]);
  });

  test("dropping above an earlier section lands exactly there", () => {
    expect(moveDestination(3, 2, 0)).toBe(0);
    expect(spliced(2, 0)).toEqual(["cta", "hero", "faq"]);
  });

  test("dropping at the end lands at the last position of the shortened list", () => {
    expect(moveDestination(3, 0, null)).toBe(2);
    expect(spliced(0, 2)).toEqual(["faq", "cta", "hero"]);
  });

  test("a gesture that ends where it started is nothing at all", () => {
    expect(moveDestination(3, 1, 1)).toBeNull();
    // Dropped above the section that already follows it: same arrangement.
    expect(moveDestination(3, 1, 2)).toBeNull();
    expect(moveDestination(3, 2, null)).toBeNull();
  });

  test("a position this page does not have is refused, never clamped", () => {
    for (const [from, before] of [
      [3, 0],
      [-1, 0],
      [0, 3],
      [0, -1],
      [1.5, 0],
    ] as const) {
      expect(moveDestination(3, from, before)).toBeNull();
    }
  });

  test("the local splice matches the destination it was given", () => {
    expect(withSectionMoved(SECTIONS, 0, 2)).toEqual([FAQ, CTA, HERO]);
    expect(withSectionMoved(SECTIONS, 2, 0)).toEqual([CTA, HERO, FAQ]);
    expect(withSectionMoved(SECTIONS, 0, 3)).toBeNull();
  });
});

describe("only this editor's own preview frame is listened to", () => {
  test("a move message is read, and anything else is not", () => {
    const message = { alo: "site-section-move", from: 0, before: 2 };
    expect(readSectionMoveMessage(message, true)).toEqual({ from: 0, before: 2 });
    expect(readSectionMoveMessage({ alo: "site-section-move", from: 1, before: null }, true))
      .toEqual({ from: 1, before: null });
    // The same message from any other window is nothing at all.
    expect(readSectionMoveMessage(message, false)).toBeNull();
    for (const junk of [
      null,
      "0",
      { alo: "site-text-edit", from: 0, before: 2 },
      { alo: "site-section-move", from: "0", before: 2 },
      { alo: "site-section-move", from: 0, before: "2" },
      { alo: "site-section-move", before: 2 },
    ]) {
      expect(readSectionMoveMessage(junk, true)).toBeNull();
    }
  });
});

describe("a move is a step in the same history typing uses", () => {
  test("its inverse is the move back", () => {
    const history = recordEdit(emptyEditHistory, { kind: "move", from: 0, to: 2 });
    const undone = undoEdit(history);
    expect(undone?.step).toEqual({ kind: "move", from: 0, to: 2 });
    expect(invertEdit((undone as NonNullable<typeof undone>).step)).toEqual({
      kind: "move",
      from: 2,
      to: 0,
    });
  });
});

// ---- the editor, driven by a drop reported by its own preview frame --------

interface Call {
  url: string;
  method: string;
  body: unknown;
}

const calls: Call[] = [];
let sections: Section[] = SECTIONS;

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  const body =
    typeof init?.body === "string" ? (JSON.parse(init.body) as unknown) : undefined;
  calls.push({ url, method, body });
  if (url.endsWith("/preview")) {
    return new Response("<!doctype html><html><body></body></html>", {
      status: 200,
      headers: { "content-type": "text/html; charset=utf-8" },
    });
  }
  // The move door answers the stored envelope, exactly as the wire does.
  const move = /\/pages\/page-1\/sections\/(\d+)\/move$/.exec(url);
  if (method === "POST" && move !== null) {
    const from = Number(move[1]);
    const to = (body as { to: number }).to;
    sections = withSectionMoved(sections, from, to) ?? sections;
    return json({ sections: env(sections) });
  }
  if (method === "GET" && url.endsWith("/sites/site-1/pages/page-1")) {
    return json({
      id: "page-1",
      slug: "",
      title: "Welcome",
      home: true,
      seoTitle: null,
      seoDescription: null,
      sections: env(sections),
    });
  }
  return json({});
});

function json(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

/** Posts what the preview document posts when a section is dropped — or moved
 *  with Alt and an arrow key, which is the same message. `own` decides whether
 *  it comes from this editor's frame or from a window that merely knows the
 *  message shape. */
function dropFromPreview(from: number, before: number | null, own = true) {
  const frame = document.querySelector("iframe");
  const event = new MessageEvent("message", {
    data: { alo: "site-section-move", from, before },
  });
  Object.defineProperty(event, "source", {
    value: own ? frame?.contentWindow : window,
  });
  window.dispatchEvent(event);
}

async function stackLoaded() {
  await waitFor(() =>
    expect(
      document.querySelectorAll('[data-section-control="edit"]'),
    ).toHaveLength(SECTIONS.length),
  );
}

function moves(): Call[] {
  return calls.filter((call) => call.method === "POST" && call.url.includes("/move"));
}

beforeEach(() => {
  calls.length = 0;
  sections = SECTIONS;
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the page editor applies what the preview reports", () => {
  function ui() {
    return render(
      <MemoryRouter initialEntries={["/sites/site-1/pages/page-1"]}>
        <Routes>
          <Route path="/sites/*" element={<SitesModule />} />
        </Routes>
      </MemoryRouter>,
    );
  }

  test("a section dropped at the end is one move, undoable and announced", async () => {
    ui();
    await stackLoaded();

    dropFromPreview(0, null);
    await waitFor(() => expect(moves()).toHaveLength(1));
    // The same request the stack's own move buttons make — one door for the
    // gesture on the page and the button beside it.
    expect(moves()[0]?.url).toContain("/pages/page-1/sections/0/move");
    expect(moves()[0]?.body).toEqual({ to: 2 });
    // A reorder is invisible to a reader who cannot see the page reflow, so it
    // is said out loud as well as done.
    //
    // `waitFor`, because the move request going out and the live region saying
    // so are two different renders: the await above proves only the first. Read
    // synchronously, this is a race an idle machine wins and a loaded one loses
    // — which is exactly how it failed, under a full workspace build. The
    // assertion is unchanged; a move that announces nothing still fails it.
    const announced = strings.sitesSectionMoved(strings.sitesSectionHero, 3, 3);
    await waitFor(() =>
      expect(
        screen
          .getAllByRole("status")
          .some((region) => region.textContent?.includes(announced)),
      ).toBe(true),
    );

    const undo = await screen.findByRole<HTMLButtonElement>("button", {
      name: strings.sitesUndoEdit,
    });
    await waitFor(() => expect(undo.disabled).toBe(false));
    fireEvent.click(undo);
    // Undo is the inverse gesture through the same door: back out of 2, in at 0.
    await waitFor(() => expect(moves()).toHaveLength(2));
    expect(moves()[1]?.url).toContain("/pages/page-1/sections/2/move");
    expect(moves()[1]?.body).toEqual({ to: 0 });
  });

  test("a message from another window changes nothing", async () => {
    ui();
    await stackLoaded();

    dropFromPreview(0, null, false);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(moves()).toHaveLength(0);
  });

  test("a drop that changes no arrangement is not a write", async () => {
    ui();
    await stackLoaded();

    dropFromPreview(0, 1);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(moves()).toHaveLength(0);
  });

  test("a section that is no longer there is refused, not aimed at its successor", async () => {
    ui();
    await stackLoaded();

    dropFromPreview(7, null);
    await waitFor(() =>
      expect(screen.getByText(strings.sitesInlineTextStale)).toBeTruthy(),
    );
    expect(moves()).toHaveLength(0);
  });

  test("the frame is told what each section is called, in the editor's language", async () => {
    ui();
    await stackLoaded();

    const frame = document.querySelector("iframe");
    const post = vi.fn();
    Object.defineProperty(frame, "contentWindow", {
      configurable: true,
      value: { postMessage: post },
    });
    fireEvent.load(frame as HTMLIFrameElement);

    expect(post).toHaveBeenCalledTimes(1);
    expect(post.mock.calls[0]?.[0]).toEqual({
      alo: "site-edit-chrome",
      labels: [
        strings.sitesSectionOnPage(strings.sitesSectionHero, 1, 3),
        strings.sitesSectionOnPage(strings.sitesSectionFaq, 2, 3),
        strings.sitesSectionOnPage(strings.sitesSectionCta, 3, 3),
      ],
      focus: null,
    });
  });
});
