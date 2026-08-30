// The section palette (ADR 0042 §4, S3.01d).
//
// Three properties are worth a test here, and they are the three the item is
// for. A block can be dragged from the palette onto a position in the page.
// The same block can be placed with a keyboard alone, through the identical
// request. And **the editor composes nothing**: the section it stores is
// byte-for-byte the one the server seeded out of the tenant's own website, so
// there is nowhere for a placeholder sentence to enter on the way.
//
// The first block is the pure wire reading; the rest drive the real editor
// with the real API client, faking only the network.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitesModule } from "./SitesModule";
import { SECTIONS_SCHEMA_VERSION, SECTION_KINDS } from "./sections";
import type { Section, SectionsEnvelope } from "./sections";
import { insertionIndex, readPalette, unseededPalette } from "./palette";

// ---- the wire ---------------------------------------------------------------

describe("the palette is read defensively", () => {
  test("a tile is a known kind with either a section or a reason", () => {
    const tiles = readPalette({
      items: [
        { kind: "hero", ready: true, section: { type: "hero", heading: "Us" } },
        { kind: "pricing", ready: false, needs: "writing" },
        { kind: "gallery", ready: false, needs: "picture" },
      ],
    });
    expect(tiles).toHaveLength(3);
    expect(tiles[0]?.section).toEqual({ type: "hero", heading: "Us" });
    expect(tiles[1]).toEqual({ kind: "pricing", section: null, needs: "writing" });
    expect(tiles[2]?.needs).toBe("picture");
  });

  test("anything this build cannot make sense of is dropped, never guessed", () => {
    const tiles = readPalette({
      items: [
        { kind: "parallax", ready: true, section: { type: "parallax" } },
        { kind: "hero", ready: true },
        { kind: "cta", ready: false, needs: "vibes" },
        "hero",
        null,
      ],
    });
    // A `ready` tile with no section is not ready; an unknown reason is no
    // reason; an unknown kind is nothing at all.
    expect(tiles.map((tile) => tile.kind)).toEqual(["hero", "cta"]);
    expect(tiles[0]?.section).toBeNull();
    expect(tiles[1]?.needs).toBeNull();
  });

  test("a body that is not a palette falls back to every block, unseeded", () => {
    for (const junk of [null, {}, { items: "hero" }, { items: [] }]) {
      expect(readPalette(junk)).toEqual(unseededPalette());
    }
    expect(unseededPalette()).toHaveLength(SECTION_KINDS.length);
    expect(unseededPalette().every((tile) => tile.section === null)).toBe(true);
  });

  test("a position the page does not have puts the block at the end", () => {
    expect(insertionIndex(3, 0)).toBe(0);
    expect(insertionIndex(3, 2)).toBe(2);
    expect(insertionIndex(3, 3)).toBe(3);
    for (const wanted of [4, -1, 1.5, Number.NaN]) {
      expect(insertionIndex(3, wanted)).toBe(3);
    }
  });
});

// ---- the editor -------------------------------------------------------------

const HERO: Section = { type: "hero", heading: "Fresh bread daily" };
const FAQ: Section = {
  type: "faq",
  items: [{ question: "When are you open?", answer: "Every day." }],
};

/** What the server seeds a contact block with on THIS website — the tenant's
 *  own words, and no `form_id` (the write path makes the form). */
const SEEDED_CONTACT: Section = {
  type: "contact_form",
  heading: "Say hello",
  success_message: "We answer within a day.",
};

const PREVIEW_HTML =
  "<!doctype html><html><body><h2>Say hello</h2></body></html>";

function env(sections: Section[]): SectionsEnvelope {
  return { schema_version: SECTIONS_SCHEMA_VERSION, sections };
}

interface Call {
  url: string;
  method: string;
  body: unknown;
}

const calls: Call[] = [];
let sections: Section[] = [HERO, FAQ];

function json(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  const body =
    typeof init?.body === "string" ? (JSON.parse(init.body) as unknown) : undefined;
  calls.push({ url, method, body });
  if (url.includes("/palette/") && url.endsWith("/preview")) {
    return new Response(PREVIEW_HTML, {
      status: 200,
      headers: { "content-type": "text/html; charset=utf-8" },
    });
  }
  if (method === "GET" && url.endsWith("/palette")) {
    return json({
      items: [
        { kind: "contact_form", ready: true, section: SEEDED_CONTACT },
        { kind: "pricing", ready: false, needs: "writing" },
      ],
    });
  }
  if (url.endsWith("/preview")) {
    return new Response("<!doctype html><html><body></body></html>", {
      status: 200,
      headers: { "content-type": "text/html; charset=utf-8" },
    });
  }
  if (method === "POST" && url.endsWith("/pages/page-1/sections")) {
    const add = body as { section: Section; index?: number };
    const at = add.index ?? sections.length;
    sections = [...sections.slice(0, at), add.section, ...sections.slice(at)];
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

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

function ui() {
  return render(
    <MemoryRouter initialEntries={["/sites/site-1/pages/page-1"]}>
      <Routes>
        <Route path="/sites/*" element={<SitesModule />} />
      </Routes>
    </MemoryRouter>,
  );
}

async function stackLoaded() {
  await waitFor(() =>
    expect(document.querySelectorAll('[data-section-control="edit"]')).toHaveLength(
      sections.length,
    ),
  );
}

/** Opens the palette the way the screen does, and waits for the SERVER's
 *  tiles — the fake wire sends two, where the unseeded fallback the palette
 *  renders while loading has one per section type. Waiting on a kind alone
 *  would return before the seeded palette replaced the fallback. */
async function openPalette() {
  fireEvent.click(screen.getByRole("button", { name: strings.sitesAddSection }));
  await waitFor(() =>
    expect(document.querySelectorAll("[data-palette-tile]")).toHaveLength(2),
  );
}

function tile(kind: string): HTMLElement {
  const found = document.querySelector<HTMLElement>(`[data-palette-tile="${kind}"]`);
  if (found === null) throw new Error(`no ${kind} tile`);
  return found;
}

function adds(): Call[] {
  return calls.filter(
    (call) => call.method === "POST" && call.url.endsWith("/pages/page-1/sections"),
  );
}

beforeEach(() => {
  calls.length = 0;
  sections = [HERO, FAQ];
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the popup section library", () => {
  test("opens as a focused dialog with category navigation", async () => {
    ui();
    await stackLoaded();
    await openPalette();

    expect(
      screen.getByRole("dialog", { name: strings.sitesPaletteTitle }),
    ).toBeTruthy();
    expect(
      screen.getByRole("navigation", {
        name: strings.sitesPaletteCategories,
      }),
    ).toBeTruthy();
    fireEvent.click(
      screen.getByRole("tab", {
        name: strings.sitesPaletteCategoryEssentials,
      }),
    );
    expect(document.querySelector("[data-palette-tile]")).toBeNull();
    fireEvent.click(
      screen.getByRole("tab", { name: strings.sitesPaletteCategoryAll }),
    );
    expect(tile("contact_form")).toBeTruthy();
  });
});

describe("the same block can be placed without a mouse", () => {
  test("choosing a position and pressing a tile makes the identical request", async () => {
    ui();
    await stackLoaded();
    await openPalette();

    // The popup moves the caret inside, and the loaded palette then puts it on
    // the first block so a keyboard user can choose immediately.
    await waitFor(() =>
      expect(document.activeElement).toBe(tile("contact_form")),
    );

    fireEvent.change(screen.getByLabelText(strings.sitesPalettePosition), {
      target: { value: "1" },
    });
    fireEvent.click(tile("contact_form"));

    await waitFor(() => expect(adds()).toHaveLength(1));
    expect(adds()[0]?.body).toEqual({ section: SEEDED_CONTACT, index: 1 });
    // Where it landed is said out loud: the stack growing is invisible to a
    // reader who cannot see it.
    const announced = strings.sitesSectionAdded(
      strings.sitesSectionContactForm,
      2,
      3,
    );
    await waitFor(() =>
      expect(
        screen
          .getAllByRole("status")
          .some((region) => region.textContent?.includes(announced)),
      ).toBe(true),
    );
  });

  test("Escape closes the palette and gives the caret back to what opened it", async () => {
    ui();
    await stackLoaded();

    // Closed straight after opening — before the seeded tiles arrived. The
    // shared modal restores the exact button that opened it.
    fireEvent.click(screen.getByRole("button", { name: strings.sitesAddSection }));
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() =>
      expect(document.querySelector("[data-palette-tile]")).toBeNull(),
    );
    await waitFor(() =>
      expect(document.activeElement).toBe(
        screen.getByRole("button", { name: strings.sitesAddSection }),
      ),
    );
  });
});

describe("a tile shows the tenant's own content, or says it has none", () => {
  test("selecting a seeded tile renders it through the site's own renderer", async () => {
    ui();
    await stackLoaded();
    await openPalette();

    fireEvent.mouseEnter(tile("contact_form"));
    const frame = await screen.findByTitle<HTMLIFrameElement>(
      strings.sitesPalettePreviewTitle(strings.sitesSectionContactForm),
    );
    expect(frame.getAttribute("srcdoc")).toBe(PREVIEW_HTML);
    expect(
      calls.some((call) => call.url.endsWith("/palette/contact_form/preview")),
    ).toBe(true);
    expect(screen.getByText(strings.sitesPaletteOwnContent)).toBeTruthy();
  });

  test("a block with nothing of theirs in it says so and opens the form", async () => {
    ui();
    await stackLoaded();
    await openPalette();

    fireEvent.mouseEnter(tile("pricing"));
    // `findByText`, not `getByText`: hovering sets state, and the note it draws
    // arrives on a later render than the event that asked for it. Reading
    // synchronously straight after the event is a race that an idle machine
    // wins and a loaded CI runner loses.
    expect(await screen.findByText(strings.sitesPaletteNeedsWriting)).toBeTruthy();
    // Nothing is fetched to preview a block that has nothing to show.
    expect(
      calls.some((call) => call.url.endsWith("/palette/pricing/preview")),
    ).toBe(false);

    fireEvent.click(tile("pricing"));
    // The prop form opens instead of a section being stored, and the position
    // chosen in the palette rides through it.
    const dialog = await screen.findByRole("dialog", {
      name: strings.sitesAddSectionTitle(strings.sitesSectionPricing),
    });
    expect(dialog.textContent).toContain(strings.sitesSectionPricing);
    expect(adds()).toHaveLength(0);
  });
});
