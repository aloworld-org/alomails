// Resizing a section within its own constraints (ADR 0042, S3.01c).
//
// The first blocks are the rule the whole item rests on: this editor can only
// ever ask for a value the *server declared*. Nothing between two declared
// values is expressible — not a percentage, not a pixel, not a fraction — and
// the gesture on the page carries a direction rather than a size, so even a
// hostile preview document cannot name one.
//
// The rest drives the real editor with the real API client, faking only the
// network and the message the preview frame posts, and proves that a resize
// travels as the same `set_prop` an approved AI proposal carries, is announced,
// and is taken back by one ⌘Z.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitesModule } from "./SitesModule";
import { SECTIONS_SCHEMA_VERSION } from "./sections";
import type { Section, SectionsEnvelope } from "./sections";
import {
  controlsFor,
  currentValue,
  layoutOperation,
  readLayoutStepMessage,
  readSectionLayouts,
  steppedValue,
  type SectionLayouts,
} from "./sectionLayout";
import { emptyEditHistory, invertEdit, recordEdit, undoEdit } from "./editHistory";

/** The declaration exactly as `GET /sites/config` serves it — the shape the
 *  store's `site_layout` module writes. Kept whole here so a change on the
 *  Rust side that this build cannot handle shows up as a failing test rather
 *  than as an editor that quietly offers nothing. */
const LAYOUTS_JSON = {
  hero: [
    {
      key: "shape",
      pointer: "/image/shape",
      values: ["wide", "natural", "square", "tall"],
      default: "natural",
    },
  ],
  text_image: [
    {
      key: "split",
      pointer: "/split",
      values: ["wide_image", "half", "wide_text"],
      default: "half",
    },
    {
      key: "shape",
      pointer: "/image/shape",
      values: ["wide", "natural", "square", "tall"],
      default: "natural",
    },
  ],
  features: [
    { key: "columns", pointer: "/columns", values: ["two", "three"], default: "three" },
  ],
  gallery: [
    {
      key: "columns",
      pointer: "/columns",
      values: ["two", "three", "four"],
      default: "three",
    },
  ],
  team: [
    {
      key: "columns",
      pointer: "/columns",
      values: ["two", "three", "four"],
      default: "three",
    },
  ],
};

const LAYOUTS: SectionLayouts = readSectionLayouts(LAYOUTS_JSON);

const IMAGE = { blob_id: "9hK3vQ2mR8pT1xWz4bC5dg", alt: "The roastery" };
const TEXT_IMAGE: Section = {
  type: "text_image",
  body: "A 1962 Probat drum, rebuilt by hand.",
  image: IMAGE,
  image_side: "left",
};
const HERO_NO_IMAGE: Section = { type: "hero", heading: "Fresh bread daily" };
const FAQ: Section = {
  type: "faq",
  items: [{ question: "When are you open?", answer: "Every day." }],
};
const SECTIONS: Section[] = [TEXT_IMAGE, HERO_NO_IMAGE, FAQ];

function env(sections: Section[]): SectionsEnvelope {
  return { schema_version: SECTIONS_SCHEMA_VERSION, sections };
}

describe("the editor offers exactly what the server declared", () => {
  test("a section type with no declaration has no handles", () => {
    expect(controlsFor(LAYOUTS, FAQ)).toEqual([]);
    expect(controlsFor(LAYOUTS, undefined)).toEqual([]);
    expect(controlsFor({}, TEXT_IMAGE)).toEqual([]);
  });

  test("a control whose property has no parent is not offered", () => {
    // A hero with no image has no shape to choose — and offering one would
    // offer a change the edit door refuses.
    expect(controlsFor(LAYOUTS, HERO_NO_IMAGE)).toEqual([]);
    const withImage: Section = { ...HERO_NO_IMAGE, image: IMAGE };
    expect(controlsFor(LAYOUTS, withImage).map((c) => c.key)).toEqual(["shape"]);
  });

  test("a malformed declaration costs the handles, never the editor", () => {
    expect(readSectionLayouts(null)).toEqual({});
    expect(readSectionLayouts("half")).toEqual({});
    expect(
      readSectionLayouts({
        gallery: [{ key: "columns", pointer: "columns", values: ["two"], default: "two" }],
      }),
    ).toEqual({});
    expect(
      readSectionLayouts({ gallery: [{ key: "columns", pointer: "/columns", values: [] }] }),
    ).toEqual({});
  });

  test("the value shown is the stored one, or the declared default", () => {
    const control = controlsFor(LAYOUTS, TEXT_IMAGE)[0];
    expect(control).toBeDefined();
    expect(currentValue(TEXT_IMAGE, control!)).toBe("half");
    expect(currentValue({ ...TEXT_IMAGE, split: "wide_text" }, control!)).toBe("wide_text");
    // A value this build's declaration does not offer reads as the default
    // rather than as a fourth state the buttons cannot show.
    expect(currentValue({ ...TEXT_IMAGE, split: "diagonal" }, control!)).toBe("half");
  });

  test("stepping walks the declared list and stops at both ends", () => {
    const control = controlsFor(LAYOUTS, TEXT_IMAGE)[0]!;
    expect(steppedValue(control, "half", 1)).toBe("wide_text");
    expect(steppedValue(control, "half", -1)).toBe("wide_image");
    expect(steppedValue(control, "wide_text", 1)).toBeNull();
    expect(steppedValue(control, "wide_image", -1)).toBeNull();
    expect(steppedValue(control, "diagonal", 1)).toBeNull();
  });
});

describe("nothing between the declared values is expressible", () => {
  test("a free value never becomes an operation", () => {
    for (const free of ["37%", "0.37", "1.5fr", "40", "", "half ", "wide_texT"]) {
      expect(layoutOperation(SECTIONS, LAYOUTS, 0, "split", free)).toBeNull();
    }
    // …nor a value that belongs to another control, or another section type.
    expect(layoutOperation(SECTIONS, LAYOUTS, 0, "split", "four")).toBeNull();
    expect(layoutOperation(SECTIONS, LAYOUTS, 0, "columns", "two")).toBeNull();
    expect(layoutOperation(SECTIONS, LAYOUTS, 2, "columns", "two")).toBeNull();
    expect(layoutOperation(SECTIONS, LAYOUTS, 9, "split", "half")).toBeNull();
  });

  test("a declared value becomes the same operation an AI proposal carries", () => {
    expect(layoutOperation(SECTIONS, LAYOUTS, 0, "split", "wide_text")).toEqual({
      op: "set_prop",
      target: { index: 0, type: "text_image" },
      pointer: "/split",
      value: "wide_text",
    });
  });

  test("the page's gesture carries a direction, never a size", () => {
    expect(readLayoutStepMessage({ alo: "site-section-layout", index: 1, step: -1 }, true))
      .toEqual({ index: 1, step: -1 });
    // The same message from any other window is nothing at all.
    expect(readLayoutStepMessage({ alo: "site-section-layout", index: 1, step: 1 }, false))
      .toBeNull();
    for (const junk of [
      null,
      "1",
      { alo: "site-section-move", index: 0, step: 1 },
      { alo: "site-section-layout", index: 0, step: 2 },
      { alo: "site-section-layout", index: 0, step: 0 },
      { alo: "site-section-layout", index: 0, step: "1" },
      { alo: "site-section-layout", index: 0, step: 0.5 },
      { alo: "site-section-layout", index: -1, step: 1 },
      { alo: "site-section-layout", index: 0, value: "wide_text" },
      { alo: "site-section-layout", index: 0, pointer: "/split", value: "37%" },
    ]) {
      expect(readLayoutStepMessage(junk, true)).toBeNull();
    }
  });
});

describe("a resize is a step in the same history typing and moving use", () => {
  test("its inverse is the resize back", () => {
    const step = {
      kind: "layout" as const,
      index: 0,
      key: "split",
      before: "half",
      after: "wide_text",
    };
    const undone = undoEdit(recordEdit(emptyEditHistory, step));
    expect(undone?.step).toEqual(step);
    expect(invertEdit(undone!.step)).toEqual({ ...step, before: "wide_text", after: "half" });
  });
});

// ---- the editor, driven by its own preview frame and its own buttons -------

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
  if (method === "GET" && url.endsWith("/sites/config")) {
    return json({ domain: "alosites.com", sectionLayouts: LAYOUTS_JSON });
  }
  // The edit door: applies the operation and answers the stored envelope,
  // exactly as the wire does.
  if (method === "PUT" && url.endsWith("/ai-edits")) {
    const edit = (body as {
      proposal: {
        operations: { target: { index: number }; pointer: string; value: string }[];
      };
    }).proposal;
    const operation = edit.operations[0]!;
    const at = operation.target.index;
    const property = operation.pointer.slice(1);
    sections = sections.map((section, i) =>
      i === at ? ({ ...section, [property]: operation.value } as Section) : section,
    );
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

/** Posts what the preview document posts when Alt and an arrow key resize the
 *  focused section: a direction, and the position it was on. */
function stepFromPreview(index: number, step: -1 | 1, own = true) {
  const frame = document.querySelector("iframe");
  const event = new MessageEvent("message", {
    data: { alo: "site-section-layout", index, step },
  });
  Object.defineProperty(event, "source", {
    value: own ? frame?.contentWindow : window,
  });
  window.dispatchEvent(event);
}

async function stackLoaded() {
  await waitFor(() =>
    expect(document.querySelectorAll('[data-section-control="edit"]')).toHaveLength(
      SECTIONS.length,
    ),
  );
}

async function openPreview() {
  fireEvent.click(
    await screen.findByRole("button", { name: strings.sitesShowPreview }),
  );
  await waitFor(() => expect(document.querySelector("iframe")).not.toBeNull());
}

function edits(): Call[] {
  return calls.filter((call) => call.method === "PUT" && call.url.includes("/ai-edits"));
}

beforeEach(() => {
  calls.length = 0;
  sections = SECTIONS;
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the page editor resizes only within the declaration", () => {
  function ui() {
    return render(
      <MemoryRouter initialEntries={["/sites/site-1/pages/page-1"]}>
        <Routes>
          <Route path="/sites/*" element={<SitesModule />} />
        </Routes>
      </MemoryRouter>,
    );
  }

  test("the choices on screen are the declared ones, and only for sections that have them", async () => {
    ui();
    await stackLoaded();
    await waitFor(() =>
      expect(document.querySelectorAll("[data-layout-choice]").length).toBeGreaterThan(0),
    );
    const offered = [...document.querySelectorAll("[data-layout-choice]")].map((node) =>
      node.getAttribute("data-layout-choice"),
    );
    // The text_image's three splits and its image's four shapes; the hero has
    // no image and the faq declares nothing, so neither offers anything.
    expect(offered).toEqual([
      "split/wide_image",
      "split/half",
      "split/wide_text",
      "shape/wide",
      "shape/natural",
      "shape/square",
      "shape/tall",
    ]);
    const half = document.querySelector('[data-layout-choice="split/half"]');
    expect(half?.getAttribute("aria-checked")).toBe("true");
  });

  test("choosing one is a set_prop through the edit door, announced and undoable", async () => {
    ui();
    await stackLoaded();
    const wider = await waitFor(() => {
      const node = document.querySelector('[data-layout-choice="split/wide_text"]');
      expect(node).toBeTruthy();
      return node as HTMLButtonElement;
    });
    fireEvent.click(wider);

    await waitFor(() => expect(edits()).toHaveLength(1));
    expect(edits()[0]?.body).toEqual({
      proposal: {
        schema_version: 1,
        operations: [
          {
            op: "set_prop",
            target: { index: 0, type: "text_image" },
            pointer: "/split",
            value: "wide_text",
          },
        ],
      },
    });
    const announced = strings.sitesSectionResized(
      strings.sitesSectionTextImage,
      strings.sitesLayoutSplitWideText,
    );
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
    // Undo is the inverse gesture through the same door — back to the value
    // the section was on, never a restored snapshot.
    await waitFor(() => expect(edits()).toHaveLength(2));
    expect(edits()[1]?.body).toMatchObject({
      proposal: { operations: [{ pointer: "/split", value: "half" }] },
    });
  });

  test("a step reported by the preview moves one place along the declared list", async () => {
    ui();
    await stackLoaded();
    await openPreview();
    await waitFor(() =>
      expect(document.querySelectorAll("[data-layout-choice]").length).toBeGreaterThan(0),
    );

    stepFromPreview(0, 1);
    await waitFor(() => expect(edits()).toHaveLength(1));
    expect(edits()[0]?.body).toMatchObject({
      proposal: { operations: [{ pointer: "/split", value: "wide_text" }] },
    });

    // …and at the end of the list it is nothing at all, rather than a wrap
    // around to the narrowest.
    stepFromPreview(0, 1);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(edits()).toHaveLength(1);
  });

  test("a step on a section with nothing to resize writes nothing", async () => {
    ui();
    await stackLoaded();
    await openPreview();
    stepFromPreview(2, 1);
    stepFromPreview(1, -1);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(edits()).toHaveLength(0);
  });

  test("a step from another window changes nothing", async () => {
    ui();
    await stackLoaded();
    await openPreview();
    stepFromPreview(0, 1, false);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(edits()).toHaveLength(0);
  });
});
