// Typing on the page (ADR 0042), and the property the whole design rests on:
// a person editing text in the preview and a model proposing the same rewrite
// produce the SAME change — same operation, same envelope, same door — so
// there is one diff, one review and one undo rather than two of each.
//
// The first block proves that equality on the request itself: identical bytes
// on the wire cannot produce a different result on the server. The rest drives
// the real editor with the real API client, faking only the network and the
// message the preview frame posts.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitesModule } from "./SitesModule";
import { SECTIONS_SCHEMA_VERSION } from "./sections";
import type { Section, SectionsEnvelope } from "./sections";
import type { SiteEditEnvelope } from "./types";
import {
  keyTarget,
  pointerText,
  readTextEditMessage,
  splitTextKey,
  textEditEnvelope,
  textEditOperation,
} from "./inlineText";
import {
  emptyEditHistory,
  invertEdit,
  recordEdit,
  redoEdit,
  undoEdit,
  type EditStep,
} from "./editHistory";

const HERO: Section = {
  type: "hero",
  heading: "Fresh bread daily",
  subheading: "Since 1962",
};
const FAQ: Section = {
  type: "faq",
  items: [{ question: "When are you open?", answer: "Every day." }],
};
const SECTIONS: Section[] = [HERO, FAQ];

function env(sections: Section[]): SectionsEnvelope {
  return { schema_version: SECTIONS_SCHEMA_VERSION, sections };
}

describe("one change shape for both paths", () => {
  /** Exactly what `POST …/ai-edits` answers as its `proposal` when the model
   *  is asked to rewrite this heading — the envelope the editor then sends
   *  back for approval. Recorded from the server contract
   *  (`alo_ai::site_edits`), not from the code under test. */
  const AI_PROPOSAL: SiteEditEnvelope = {
    schema_version: 1,
    operations: [
      {
        op: "rewrite_copy",
        target: { index: 0, type: "hero" },
        pointer: "/heading",
        text: "Bread worth the walk",
      },
    ],
  };

  test("a text edit typed on the page IS the operation the model proposes", () => {
    const operation = textEditOperation(
      SECTIONS,
      "0/heading",
      "Bread worth the walk",
    );
    expect(operation).not.toBeNull();
    const typed = textEditEnvelope(operation as NonNullable<typeof operation>);

    expect(typed).toEqual(AI_PROPOSAL);
    // Same bytes on the wire, therefore the same diff after it: there is no
    // room left for the two paths to differ on the server.
    expect(JSON.stringify(typed)).toBe(JSON.stringify(AI_PROPOSAL));
  });

  test("a nested item's text carries the same pointer the model would use", () => {
    expect(
      textEditOperation(SECTIONS, "1/items/0/question", "What time do you open?"),
    ).toEqual({
      op: "rewrite_copy",
      target: { index: 1, type: "faq" },
      pointer: "/items/0/question",
      text: "What time do you open?",
    });
  });
});

describe("the coordinate is resolved here, never trusted", () => {
  test("a key is a section index followed by a JSON pointer", () => {
    expect(splitTextKey("2/items/0/title")).toEqual({
      index: 2,
      pointer: "/items/0/title",
    });
    for (const bad of ["", "/heading", "heading", "x/heading", "-1/heading"]) {
      expect(splitTextKey(bad)).toBeNull();
    }
  });

  test("a pointer answers only existing text", () => {
    expect(pointerText(HERO, "/heading")).toBe("Fresh bread daily");
    expect(pointerText(FAQ, "/items/0/answer")).toBe("Every day.");
    // Absent, not a string, and off the end of an array: all refused, because
    // `rewrite_copy` rewrites text that is already there.
    expect(pointerText(HERO, "/image")).toBeNull();
    expect(pointerText(FAQ, "/items")).toBeNull();
    expect(pointerText(FAQ, "/items/7/answer")).toBeNull();
  });

  test("a stale coordinate is refused rather than aimed at whatever moved in", () => {
    // The hero was deleted; index 0 is now the FAQ, and "0/heading" would
    // otherwise rewrite a property of a section nobody was looking at.
    expect(keyTarget([FAQ], "0/heading")).toBeNull();
    expect(textEditOperation([FAQ], "0/heading", "anything")).toBeNull();
    expect(textEditOperation(SECTIONS, "9/heading", "anything")).toBeNull();
  });

  test("only this editor's own preview frame is listened to", () => {
    const message = { alo: "site-text-edit", key: "0/heading", text: "New" };
    expect(readTextEditMessage(message, true)).toEqual({
      key: "0/heading",
      text: "New",
    });
    // The same message from any other window is nothing at all.
    expect(readTextEditMessage(message, false)).toBeNull();
    for (const junk of [
      null,
      "0/heading",
      { alo: "something-else", key: "0/heading", text: "New" },
      { alo: "site-text-edit", key: 0, text: "New" },
      { alo: "site-text-edit", key: "0/heading" },
      { alo: "site-text-edit", key: "0/heading", text: "x".repeat(5001) },
    ]) {
      expect(readTextEditMessage(junk, true)).toBeNull();
    }
  });
});

describe("undo and redo, over the history every gesture shares", () => {
  const first: EditStep = {
    kind: "text",
    key: "0/heading",
    before: "Fresh bread daily",
    after: "Bread",
  };
  const second: EditStep = {
    kind: "text",
    key: "0/subheading",
    before: "Since 1962",
    after: "Since 1962.",
  };

  test("undo walks back, redo walks forward, and a new edit ends the branch", () => {
    let history = recordEdit(recordEdit(emptyEditHistory, first), second);

    const undone = undoEdit(history);
    expect(undone?.step).toEqual(second);
    // Undo is the inverse gesture, not a restored snapshot: applying it writes
    // the text the page had before.
    expect(invertEdit((undone as NonNullable<typeof undone>).step)).toEqual({
      kind: "text",
      key: "0/subheading",
      before: "Since 1962.",
      after: "Since 1962",
    });
    history = (undone as NonNullable<typeof undone>).history;

    const redone = redoEdit(history);
    expect(redone?.step).toEqual(second);
    history = (redone as NonNullable<typeof redone>).history;
    expect(redoEdit(history)).toBeNull();

    // Undo once, then type something new: the abandoned redo is gone.
    history = (undoEdit(history) as { history: typeof history }).history;
    history = recordEdit(history, {
      kind: "text",
      key: "1/items/0/answer",
      before: "Every day.",
      after: "Every day except Sunday.",
    });
    expect(history.future).toHaveLength(0);
    expect(history.past).toHaveLength(2);
  });

  test("an empty history offers nothing to undo or redo", () => {
    expect(undoEdit(emptyEditHistory)).toBeNull();
    expect(redoEdit(emptyEditHistory)).toBeNull();
  });
});

// ---- the editor, driven by a message from its own preview frame ------------

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
  // The edit door answers the stored envelope, exactly as the wire does.
  if (method === "PUT" && url.endsWith("/ai-edits")) {
    const proposal = (body as { proposal: SiteEditEnvelope }).proposal;
    const operation = proposal.operations[0];
    if (operation !== undefined && operation.op === "rewrite_copy") {
      const target = sections[operation.target.index] as unknown as Record<
        string,
        unknown
      >;
      sections = sections.map((section, index) =>
        index === operation.target.index
          ? ({
              ...target,
              [operation.pointer.slice(1)]: operation.text,
            } as unknown as Section)
          : section,
      );
    }
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

/** Posts what the preview document posts when someone finishes typing.
 *  `own` decides whether it comes from this editor's frame or from a window
 *  that merely knows the message shape. */
function postFromPreview(key: string, text: string, own = true) {
  const frame = document.querySelector("iframe");
  const event = new MessageEvent("message", {
    data: { alo: "site-text-edit", key, text },
  });
  Object.defineProperty(event, "source", {
    value: own ? frame?.contentWindow : window,
  });
  window.dispatchEvent(event);
}

/** Waits until the editor is holding the page — the stack has a row per
 *  section — so a message is answered against the real coordinates rather
 *  than against a stack that has not arrived yet. */
async function stackLoaded() {
  await waitFor(() =>
    expect(
      document.querySelectorAll('[data-section-control="edit"]'),
    ).toHaveLength(SECTIONS.length),
  );
}

function edits(): Call[] {
  return calls.filter((call) => call.method === "PUT" && call.url.endsWith("/ai-edits"));
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

  test("a finished text edit becomes one guarded rewrite, undoable and redoable", async () => {
    ui();
    await stackLoaded();

    postFromPreview("0/heading", "Bread worth the walk");
    await waitFor(() => expect(edits()).toHaveLength(1));
    expect(edits()[0]?.body).toEqual({
      proposal: {
        schema_version: 1,
        operations: [
          {
            op: "rewrite_copy",
            target: { index: 0, type: "hero" },
            pointer: "/heading",
            text: "Bread worth the walk",
          },
        ],
      },
    });

    // Undo is offered only once there is something to take back, and it puts
    // the previous text through the identical door.
    const undo = await screen.findByRole<HTMLButtonElement>("button", {
      name: strings.sitesUndoEdit,
    });
    await waitFor(() => expect(undo.disabled).toBe(false));
    fireEvent.click(undo);
    await waitFor(() => expect(edits()).toHaveLength(2));
    expect(edits()[1]?.body).toEqual({
      proposal: {
        schema_version: 1,
        operations: [
          {
            op: "rewrite_copy",
            target: { index: 0, type: "hero" },
            pointer: "/heading",
            text: "Fresh bread daily",
          },
        ],
      },
    });

    const redo = screen.getByRole<HTMLButtonElement>("button", {
      name: strings.sitesRedoEdit,
    });
    await waitFor(() => expect(redo.disabled).toBe(false));
    fireEvent.click(redo);
    await waitFor(() => expect(edits()).toHaveLength(3));
    expect(
      (edits()[2]?.body as { proposal: SiteEditEnvelope }).proposal.operations[0],
    ).toMatchObject({ text: "Bread worth the walk" });
  });

  test("a message from another window changes nothing", async () => {
    ui();
    await stackLoaded();

    postFromPreview("0/heading", "Injected", false);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(edits()).toHaveLength(0);
    expect(
      screen.getByRole<HTMLButtonElement>("button", {
        name: strings.sitesUndoEdit,
      }).disabled,
    ).toBe(true);
  });

  test("text that did not change is not a write", async () => {
    ui();
    await stackLoaded();

    postFromPreview("0/heading", "Fresh bread daily");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(edits()).toHaveLength(0);
  });
});
