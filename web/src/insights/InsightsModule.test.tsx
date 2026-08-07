// What the Insights screens promise, proven against a recorded network: that a
// board is the tiles the server sent, that every figure on screen is one the
// server computed (in the currency it stated, and never summed here), that a
// chart's numbers are also in the document for a reader who cannot see the
// canvas, that a tile pinned by a newer version of alo still renders and is
// never asked for figures it cannot answer, and that rearranging a board is one
// request that moves one tile.
//
// Only the network and the chart engine are fake. The real router, the real
// module routes, the real client, the real grid and the real dialogs all run:
// the point of the item is that these screens agree with the API, and a test
// against stubs could not tell.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { InsightsModule } from "./InsightsModule";
import type { Dashboard, Series, Tile } from "./types";

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

/** Queues one answer for the next request whose URL contains `urlPart`. */
function reply(urlPart: string, method: string, body: unknown, status = 200) {
  replies.push({ match: (url, m) => url.includes(urlPart) && m === method, status, body });
}

function board(id: string, name: string, systemKey: string | null = null): Dashboard {
  return {
    id,
    name,
    systemKey,
    seeded: systemKey !== null,
    createdBy: "u-1",
    createdAt: "2026-08-07T09:00:00Z",
    updatedAt: "2026-08-07T09:00:00Z",
  };
}

const OVERVIEW = board("dash-1", "Business overview", "business_overview");
const CASH = board("dash-2", "Cash");

function tile(id: string, title: string, viz: Tile["viz"], position: number, span = 1): Tile {
  return {
    id,
    dashboardId: OVERVIEW.id,
    title,
    spec: { schema_version: 1 },
    readable: true,
    specError: null,
    viz,
    position,
    span,
    createdAt: "2026-08-07T09:00:00Z",
    updatedAt: "2026-08-07T09:00:00Z",
  };
}

const OUTSTANDING = tile("tile-1", "Outstanding", "number", 1);
const REVENUE = tile("tile-2", "Revenue by month", "bar", 2, 2);
/** A tile this build cannot read: pinned by a newer alo. */
const FUTURE: Tile = {
  ...tile("tile-3", "Later", null, 3),
  spec: { schema_version: 2 },
  readable: false,
  specError: "unsupported chart schema_version 2",
};
const TILES = [OUTSTANDING, REVENUE, FUTURE];

/** One figure, in euro — what is owed right now. */
const OWED: Series = {
  unit: { kind: "money", currency: "EUR" },
  series: [
    {
      key: "EUR",
      label: { kind: "raw", text: "EUR" },
      points: [{ bucket: "total", value: 4_200_000 }],
    },
  ],
  notes: [],
  truncated: false,
};

/** Three months of billing, the middle one quiet. */
const BY_MONTH: Series = {
  unit: { kind: "money", currency: "EUR" },
  series: [
    {
      key: "EUR",
      label: { kind: "raw", text: "EUR" },
      points: [
        { bucket: "2026-06", value: 1_000_000 },
        { bucket: "2026-07", value: 0 },
        { bucket: "2026-08", value: 2_500_000 },
      ],
    },
  ],
  notes: [{ code: "unconverted_documents", count: 2 }],
  truncated: false,
};

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({
    url,
    method,
    body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
  });
  const index = replies.findIndex((r) => r.match(url, method));
  const answer = index === -1 ? fallback(url, method) : (replies.splice(index, 1)[0] as Reply);
  return new Response(JSON.stringify(answer.body), {
    status: answer.status,
    headers: { "content-type": "application/json" },
  });
});

/** The ready-made questions the server offers, in the shape it sends them: a
 *  key the client translates, and the question itself — never a caption. */
const GALLERY = {
  entries: [
    {
      key: "outstanding",
      module: "billing",
      viz: "number",
      span: 1,
      spec: { schema_version: 1, dataset: "billing.receivables" },
    },
    {
      key: "pipeline_by_stage",
      module: "crm",
      viz: "bar",
      span: 2,
      spec: { schema_version: 1, dataset: "crm.deals" },
    },
  ],
  overview: ["outstanding"],
};

/** What a board reads before anything interesting happens. */
function fallback(url: string, method: string): Reply {
  const body =
    method !== "GET"
      ? {}
      : url.includes("/insights/gallery")
        ? GALLERY
        : url.includes("/insights/tiles/tile-1/data")
          ? OWED
          : url.includes("/insights/tiles/tile-2/data")
            ? BY_MONTH
            : url.includes("/insights/dashboards/dash-1")
              ? { dashboard: OVERVIEW, tiles: TILES }
              : url.includes("/insights/dashboards/")
                ? { dashboard: CASH, tiles: [] }
                : { dashboards: [OVERVIEW, CASH] };
  return { match: () => true, status: 200, body };
}

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch, identity: { sub: "u-1", email: "", name: "" } }),
}));

// The chart engine is a canvas, and jsdom has none. The wrapper is mocked so
// the *module* is what these tests exercise; the drawable model behind it is
// pure and tested on its own (`chart/model.test.ts`).
vi.mock("./chart", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./chart")>();
  return {
    ...actual,
    Chart: ({ label }: { label: string }) => <div data-testid="chart">{label}</div>,
  };
});

/** The module as it is really mounted: at `/insights/*`, routing itself. */
function ui(path = "/insights") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <DialogProvider>
        <Routes>
          <Route path="/insights/*" element={<InsightsModule />} />
        </Routes>
      </DialogProvider>
    </MemoryRouter>,
  );
}

function writes(): Call[] {
  return calls.filter((c) => c.method !== "GET");
}

function reads(part: string): Call[] {
  return calls.filter((c) => c.method === "GET" && c.url.includes(part));
}

/** Opens a tile's action menu. */
function openMenu(title: string) {
  fireEvent.click(screen.getByRole("button", { name: strings.insightsTileActions(title) }));
}

beforeEach(() => {
  calls.length = 0;
  replies = [];
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the boards", () => {
  test("are the tab strip, and the first one opens without a click", async () => {
    ui();

    expect(await screen.findByRole("heading", { name: OVERVIEW.name })).toBeTruthy();
    const strip = within(screen.getByRole("navigation", { name: strings.insightsBoards }));
    expect(strip.getByText(OVERVIEW.name)).toBeTruthy();
    expect(strip.getByText(CASH.name)).toBeTruthy();
    // The board that opened is the first one, and it was read once.
    await waitFor(() => expect(reads("/insights/dashboards/dash-1").length).toBe(1));
  });

  test("a new one is created and opened, and nothing is invented before the server answers", async () => {
    ui();
    await screen.findByRole("heading", { name: OVERVIEW.name });

    const made = board("dash-3", "VAT");
    reply("/insights/dashboards", "POST", { dashboard: made });
    replies.push({
      match: (url, m) => url.includes("/insights/dashboards") && m === "GET" && !url.includes("dash-"),
      status: 200,
      body: { dashboards: [OVERVIEW, CASH, made] },
    });
    reply("/insights/dashboards/dash-3", "GET", { dashboard: made, tiles: [] });

    fireEvent.click(screen.getByRole("button", { name: strings.insightsNewBoard }));
    fireEvent.change(await screen.findByRole("textbox"), { target: { value: " VAT " } });
    fireEvent.click(screen.getByRole("button", { name: strings.dialogConfirm }));

    await waitFor(() => expect(writes().length).toBe(1));
    const created = writes()[0] as Call;
    expect(created.url).toContain("/insights/dashboards");
    expect(created.body).toEqual({ name: "VAT" });
    // And the board that was asked for is the one now on screen.
    expect(await screen.findByRole("heading", { name: "VAT" })).toBeTruthy();
  });
});

describe("the zero-setup board", () => {
  test("is what a first visit lands on, and its language is asked for", async () => {
    ui();

    // The board that opens with no click is the seeded overview, and it is the
    // server that seeded it: the read that lists boards carries the interface
    // language, because that read is the one that writes the board.
    expect(await screen.findByRole("heading", { name: OVERVIEW.name })).toBeTruthy();
    const list = reads("/insights/dashboards?")[0] as Call;
    expect(list.url).toContain("lang=");
    // And the figures on it are the server's, with nothing asked of the user.
    expect(await screen.findByText("€42,000.00")).toBeTruthy();
  });
});

describe("the gallery", () => {
  test("offers the server's questions in the reader's words, and pins the spec verbatim", async () => {
    ui();
    await screen.findByText("€42,000.00");

    fireEvent.click(screen.getByRole("button", { name: strings.insightsAddChart }));
    const dialog = within(await screen.findByRole("dialog"));
    // The words are ours; the server sent keys. Both modules are grouped.
    expect(dialog.getByText(strings.insightsGalleryOutstanding)).toBeTruthy();
    expect(dialog.getByText(strings.insightsGalleryOutstandingBody)).toBeTruthy();
    expect(dialog.getByText(strings.insightsGalleryPipelineByStage)).toBeTruthy();
    expect(dialog.getByText(strings.moduleBilling)).toBeTruthy();
    expect(dialog.getByText(strings.moduleCrm)).toBeTruthy();
    expect(writes()).toEqual([]);

    reply("/insights/dashboards/dash-1/tiles", "POST", { tile: OUTSTANDING });
    fireEvent.click(dialog.getByText(strings.insightsGalleryPipelineByStage));

    await waitFor(() => expect(writes().length).toBe(1));
    const pinned = writes()[0] as Call;
    expect(pinned.url).toContain("/insights/dashboards/dash-1/tiles");
    // The question is the server's own envelope, unchanged; the caption is the
    // one the reader was looking at; the width is what the entry asked for.
    expect(pinned.body).toEqual({
      title: strings.insightsGalleryPipelineByStage,
      spec: GALLERY.entries[1]?.spec,
      span: 2,
    });
    // And the board is re-read from the server rather than guessed at.
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(reads("/insights/dashboards/dash-1").length).toBeGreaterThan(1);
  });

  test("closing it without picking pins nothing", async () => {
    ui();
    await screen.findByText("€42,000.00");

    fireEvent.click(screen.getByRole("button", { name: strings.insightsAddChart }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: strings.insightsGalleryClose }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(writes()).toEqual([]);
  });

  test("a refusal to pin stays on screen and says why", async () => {
    ui();
    await screen.findByText("€42,000.00");

    fireEvent.click(screen.getByRole("button", { name: strings.insightsAddChart }));
    const dialog = within(await screen.findByRole("dialog"));
    reply(
      "/insights/dashboards/dash-1/tiles",
      "POST",
      { detail: "a dashboard may hold at most 40 tiles" },
      422,
    );
    fireEvent.click(dialog.getByText(strings.insightsGalleryOutstanding));

    expect(await screen.findByText("a dashboard may hold at most 40 tiles")).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});

describe("the ask", () => {
  /** What the server proposes for "what did we bill?": a spec it validated, the
   *  drawing, the width, and the figures it evaluated — all in one answer. */
  const PROPOSAL = {
    spec: { schema_version: 1, dataset: "billing.documents", measure: { id: "net", agg: "sum" } },
    viz: "number",
    span: 1,
    series: OWED,
    repaired: false,
  };

  /** Types a question into the open dialog and asks it. */
  function askFor(question: string) {
    fireEvent.change(screen.getByLabelText(strings.insightsAskLabel), {
      target: { value: question },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.insightsAskSubmit }));
  }

  test("previews the server's chart, and pins nothing until the reader says so", async () => {
    ui();
    await screen.findByText("€42,000.00");

    fireEvent.click(screen.getByRole("button", { name: strings.insightsAsk }));
    await screen.findByRole("dialog");

    reply("/insights/ask", "POST", PROPOSAL);
    askFor("  what did we bill?  ");

    // The question crossed trimmed, and nothing else was sent with it.
    await waitFor(() => expect(writes().length).toBe(1));
    const asked = writes()[0] as Call;
    expect(asked.url).toContain("/insights/ask");
    expect(asked.body).toEqual({ q: "what did we bill?" });

    // The preview is on screen, captioned with the reader's own question, and
    // showing the figure the server computed.
    const preview = within(await screen.findByRole("region", { name: strings.insightsAskPreview }));
    expect(preview.getByText("what did we bill?")).toBeTruthy();
    expect(preview.getByText("€42,000.00")).toBeTruthy();
    // Asking pinned nothing: the only write so far is the ask itself.
    expect(writes().length).toBe(1);

    reply("/insights/dashboards/dash-1/tiles", "POST", { tile: OUTSTANDING });
    fireEvent.click(screen.getByRole("button", { name: strings.insightsAskPin }));

    await waitFor(() => expect(writes().length).toBe(2));
    const pinned = writes()[1] as Call;
    expect(pinned.url).toContain("/insights/dashboards/dash-1/tiles");
    // The spec is the server's own envelope, unedited; the caption is the
    // reader's question; the width is the one the server proposed.
    expect(pinned.body).toEqual({
      title: "what did we bill?",
      spec: PROPOSAL.spec,
      span: 1,
    });
    // And the board is re-read from the server rather than guessed at.
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(reads("/insights/dashboards/dash-1").length).toBeGreaterThan(1);
  });

  test("discarding a proposal pins nothing", async () => {
    ui();
    await screen.findByText("€42,000.00");

    fireEvent.click(screen.getByRole("button", { name: strings.insightsAsk }));
    await screen.findByRole("dialog");
    reply("/insights/ask", "POST", PROPOSAL);
    askFor("what did we bill?");
    await screen.findByRole("region", { name: strings.insightsAskPreview });

    fireEvent.click(screen.getByRole("button", { name: strings.insightsAskDiscard }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // The ask itself, and nothing after it.
    expect(writes().length).toBe(1);
    expect((writes()[0] as Call).url).toContain("/insights/ask");
  });

  test("a question that could not be charted says so, with nothing drawn", async () => {
    ui();
    await screen.findByText("€42,000.00");

    fireEvent.click(screen.getByRole("button", { name: strings.insightsAsk }));
    await screen.findByRole("dialog");
    reply(
      "/insights/ask",
      "POST",
      { detail: "no chart could be built from that question: I cannot chart the weather." },
      422,
    );
    askFor("what is the weather tomorrow?");

    expect(await screen.findByText(/I cannot chart the weather/)).toBeTruthy();
    expect(screen.queryByRole("region", { name: strings.insightsAskPreview })).toBeNull();
    expect(screen.queryByRole("button", { name: strings.insightsAskPin })).toBeNull();
  });

  test("a workspace with no assistant is told in our words, not the server's code", async () => {
    ui();
    await screen.findByText("€42,000.00");

    fireEvent.click(screen.getByRole("button", { name: strings.insightsAsk }));
    await screen.findByRole("dialog");
    reply("/insights/ask", "POST", { detail: "ai-unavailable" }, 503);
    askFor("what did we bill?");

    expect(await screen.findByText(strings.insightsAskUnavailable)).toBeTruthy();
    expect(screen.queryByText("ai-unavailable")).toBeNull();
  });

  test("a corrected proposal says so on the preview", async () => {
    ui();
    await screen.findByText("€42,000.00");

    fireEvent.click(screen.getByRole("button", { name: strings.insightsAsk }));
    await screen.findByRole("dialog");
    reply("/insights/ask", "POST", { ...PROPOSAL, repaired: true });
    askFor("what did we bill?");

    expect(await screen.findByText(strings.insightsAskRepaired)).toBeTruthy();
  });
});

describe("a tile", () => {
  test("shows the figure the server computed, in the currency the server stated", async () => {
    ui();

    expect(await screen.findByText("€42,000.00")).toBeTruthy();
    // Each tile asked for its own figures, on its own route.
    await waitFor(() => expect(reads("/insights/tiles/tile-1/data").length).toBe(1));
  });

  test("money in two currencies is two figures, never one total", async () => {
    reply("/insights/tiles/tile-1/data", "GET", {
      unit: { kind: "money" },
      series: [
        {
          key: "EUR",
          label: { kind: "raw", text: "EUR" },
          points: [{ bucket: "total", value: 4_200_000 }],
        },
        {
          key: "USD",
          label: { kind: "raw", text: "USD" },
          points: [{ bucket: "total", value: 1_000_000 }],
        },
      ],
      notes: [],
      truncated: false,
    });
    ui();

    expect(await screen.findByText("€42,000.00")).toBeTruthy();
    expect(screen.getByText("$10,000.00")).toBeTruthy();
    // Nothing added the two together — 52 000 is a number nobody stated.
    expect(screen.queryByText("€52,000.00")).toBeNull();
  });

  test("draws a chart and puts the same figures in the document", async () => {
    ui();
    await screen.findByTestId("chart");

    // The drawn chart, and the same answer as rows for a reader who cannot see
    // it: the months the server bucketed, its figures, and its note.
    const table = within(screen.getByRole("table"));
    expect(table.getByText("Jun 2026")).toBeTruthy();
    expect(table.getByText("Jul 2026")).toBeTruthy();
    expect(table.getByText("€25,000.00")).toBeTruthy();
    expect(screen.getByText(strings.insightsNoteUnconverted(2))).toBeTruthy();
  });

  test("from a newer version renders its reason, and is never asked for figures", async () => {
    ui();
    await screen.findByText(FUTURE.specError as string);

    expect(screen.getByText(strings.insightsUnreadableTitle)).toBeTruthy();
    expect(reads("/insights/tiles/tile-3/data")).toEqual([]);
  });

  test("whose figures fail says so, and the rest of the board still renders", async () => {
    reply("/insights/tiles/tile-1/data", "GET", { detail: "period: this chart would read too much" }, 422);
    ui();

    expect(await screen.findByText("period: this chart would read too much")).toBeTruthy();
    expect(screen.getByTestId("chart")).toBeTruthy();
  });
});

describe("rearranging a board", () => {
  test("is one move request, landing between the two tiles it now sits between", async () => {
    ui();
    await screen.findByText("€42,000.00");

    reply("/insights/tiles/tile-1/move", "POST", { tile: { ...OUTSTANDING, position: 2.5 } });
    openMenu(OUTSTANDING.title);
    fireEvent.click(screen.getByRole("menuitem", { name: strings.insightsMoveRight }));

    await waitFor(() => expect(writes().length).toBe(1));
    const move = writes()[0] as Call;
    expect(move.url).toContain("/insights/tiles/tile-1/move");
    // Halfway between the tiles it lands between — one row changes, no other
    // tile is rewritten, and nothing about the tile itself is touched.
    expect(move.body).toEqual({ position: 2.5 });
  });

  test("cannot move the first tile earlier", async () => {
    ui();
    await screen.findByText("€42,000.00");

    openMenu(OUTSTANDING.title);
    expect(
      screen.getByRole("menuitem", { name: strings.insightsMoveLeft }).hasAttribute("disabled"),
    ).toBe(true);
  });

  test("resizing sends the width and nothing else", async () => {
    ui();
    await screen.findByText("€42,000.00");

    reply("/insights/tiles/tile-1", "PATCH", { tile: { ...OUTSTANDING, span: 2 } });
    openMenu(OUTSTANDING.title);
    fireEvent.click(screen.getByRole("menuitem", { name: strings.insightsWiden }));

    await waitFor(() => expect(writes().length).toBe(1));
    const patch = writes()[0] as Call;
    expect(patch.method).toBe("PATCH");
    expect(patch.body).toEqual({ span: 2 });
  });

  test("removing a tile asks first, and a refusal to confirm sends nothing", async () => {
    ui();
    await screen.findByText("€42,000.00");

    openMenu(OUTSTANDING.title);
    fireEvent.click(screen.getByRole("menuitem", { name: strings.insightsRemoveTile }));
    fireEvent.click(await screen.findByRole("button", { name: strings.dialogCancel }));
    expect(writes()).toEqual([]);

    openMenu(OUTSTANDING.title);
    fireEvent.click(screen.getByRole("menuitem", { name: strings.insightsRemoveTile }));
    reply("/insights/tiles/tile-1", "DELETE", { deleted: true });
    fireEvent.click(await screen.findByRole("button", { name: strings.insightsRemoveTile }));

    await waitFor(() => expect(writes().length).toBe(1));
    const removed = writes()[0] as Call;
    expect(removed.method).toBe("DELETE");
    expect(removed.url).toContain("/insights/tiles/tile-1");
  });
});
