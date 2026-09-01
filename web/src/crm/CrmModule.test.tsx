// What the CRM screens promise, proven against a recorded network: that the
// board is the stages the server sent, that a drag is one move request (and a
// losing column asks why *before* one is made), that the list's filters are
// asked of the server rather than applied to a loaded page, and that the drawer
// shows a deal's log, its next steps and the conversations it belongs to —
// offering to open only the ones this reader actually holds.
//
// Since B2.08 it also covers closing a deal: the lost-reason picker (which
// fills a free-text field rather than replacing it, and refuses a blank), the
// handoff that raises a draft quote or invoice in billing (asking only what the
// deal cannot answer, and sending a rate as basis points), and the report —
// where every figure on screen is the server's and the open board and the
// period's outcomes are kept visibly apart.
//
// Only the network is fake. The real router, the real module routes, the real
// client, the real drawer and the real dialogs all run: the point of the item
// is that these screens agree with the API, and a test against stubs could not
// tell.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import type { Task } from "../jmap";
import { CrmModule } from "./CrmModule";
import type {
  CrmDeal,
  CrmPipeline,
  CrmStage,
  DealActivity,
  DealThread,
  PipelineReport,
  PipelineTally,
} from "./types";

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

const PIPELINE: CrmPipeline = {
  id: "pip-1",
  name: "Sales",
  description: "",
  archived: false,
  archivedAt: null,
  createdBy: "u-1",
  createdAt: "2026-08-07T09:00:00Z",
  updatedAt: "2026-08-07T09:00:00Z",
};

function stage(id: string, name: string, position: number, flags: Partial<CrmStage> = {}): CrmStage {
  return {
    id,
    pipelineId: PIPELINE.id,
    name,
    position,
    isWon: false,
    isLost: false,
    closed: false,
    archived: false,
    archivedAt: null,
    createdAt: "2026-08-07T09:00:00Z",
    updatedAt: "2026-08-07T09:00:00Z",
    ...flags,
  };
}

const NEW = stage("stg-new", "New", 1);
const QUALIFIED = stage("stg-qual", "Qualified", 2);
const WON = stage("stg-won", "Won", 4, { isWon: true, closed: true });
const LOST = stage("stg-lost", "Lost", 5, { isLost: true, closed: true });
const STAGES = [NEW, QUALIFIED, WON, LOST];

const DEAL: CrmDeal = {
  id: "deal-1",
  pipelineId: PIPELINE.id,
  stageId: NEW.id,
  title: "40 seats — Acme GmbH",
  customerId: null,
  contactId: null,
  companyName: "Acme GmbH",
  contactName: "Ada",
  contactEmail: "ada@acme.test",
  valueCents: 2_500_000,
  currency: "EUR",
  expectedClose: "2026-09-30",
  ownerUserId: "u-1",
  source: "Referral",
  position: 1,
  state: "open",
  closed: false,
  lostReason: null,
  closedAt: null,
  createdBy: "u-1",
  createdAt: "2026-08-07T09:00:00Z",
  updatedAt: "2026-08-07T09:00:00Z",
};

const NOTE: DealActivity = {
  id: "act-1",
  dealId: DEAL.id,
  kind: "call",
  body: "Ada wants 40 seats quoted.",
  happenedAt: "2026-08-07T14:05:00Z",
  authorUserId: "u-1",
  createdAt: "2026-08-07T18:00:00Z",
};

/** One conversation this reader holds, and one a colleague linked that they do
 *  not — the asymmetry the whole thread-link design turns on. */
const MINE: DealThread = {
  threadId: "thr-1",
  subject: "Quote for 40 seats",
  readable: true,
  linkedBy: "u-1",
  linkedAt: "2026-08-07T15:00:00Z",
};
const THEIRS: DealThread = {
  threadId: "thr-2",
  subject: "renewal",
  readable: false,
  linkedBy: "sam@alo.test",
  linkedAt: "2026-08-07T16:00:00Z",
};

const STEP: Task = {
  id: "tsk-1",
  projectId: "proj-1",
  title: "Send the renewal quote",
  description: null,
  status: "todo",
  position: 1,
  assigneeId: "u-1",
  assignee: "me@alo.test",
  dueAt: "2026-08-14T09:00:00Z",
  priority: "none",
  state: "active",
  sourceKind: "deal",
  sourceId: DEAL.id,
  subtaskDone: 0,
  subtaskTotal: 0,
  commentCount: 0,
  completedAt: null,
  createdAt: "2026-08-07T09:00:00Z",
};

/** A quarter of the board: 25 000 open in New, one deal won and one lost. Every
 *  figure is the server's — the screen must not add up a column of its own. */
const REPORT: PipelineReport = {
  pipelineId: PIPELINE.id,
  pipelineName: "Sales",
  from: "2026-07-01",
  to: "2026-09-30",
  openAsOf: "2026-08-07T09:00:00Z",
  currencies: [
    {
      currency: "EUR",
      stages: [
        { stageId: NEW.id, name: "New", isWon: false, isLost: false, open: tally(1, 2_500_000) },
        { stageId: QUALIFIED.id, name: "Qualified", isWon: false, isLost: false, open: tally(0, 0) },
        { stageId: LOST.id, name: "Lost", isWon: false, isLost: true, open: tally(0, 0) },
      ],
      open: tally(1, 2_500_000),
      won: tally(1, 900_000),
      lost: tally(1, 50_000),
      winRateBp: 5_000,
    },
  ],
};

function tally(dealCount: number, valueCents: number): PipelineTally {
  return { dealCount, valueCents };
}

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

/** What a screen reads before anything interesting happens. */
function fallback(url: string, method: string): Reply {
  const body =
    method !== "GET"
      ? {}
      : url.includes("/crm/pipelines/")
        ? { stages: STAGES }
        : url.includes("/crm/pipelines")
          ? { pipelines: [PIPELINE] }
          : url.includes("/activities")
            ? { activities: [] }
            : url.includes("/next-steps")
              ? { nextSteps: [] }
              : url.includes("/thread-suggestions")
                ? { suggestions: [] }
                : url.includes("/threads")
                  ? { threads: [] }
                  : url.includes("/crm/reports/pipeline")
                    ? { report: REPORT }
                    : url.includes("/crm/deals/")
                      ? { deal: DEAL }
                      : { deals: [DEAL] };
  return { match: () => true, status: 200, body };
}

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch, identity: { sub: "u-1", email: "", name: "" } }),
}));

/** The module as it is really mounted: at `/crm/*`, routing itself. */
function ui(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <DialogProvider>
        <Routes>
          <Route path="/crm/*" element={<CrmModule />} />
          {/* Where "open in mail" hands off to. */}
          <Route path="/mail" element={<p>the mail module</p>} />
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

beforeEach(() => {
  calls.length = 0;
  replies = [];
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the board", () => {
  test("uses canonical tab URLs that cannot append the current route", async () => {
    ui("/crm/board");

    const boardLink = await screen.findByRole("link", { name: strings.crmBoard });
    expect(boardLink.getAttribute("href")).toBe("/crm/board");
    expect(screen.getByRole("link", { name: strings.crmList }).getAttribute("href")).toBe("/crm/list");
    expect(screen.getByRole("link", { name: strings.crmReport }).getAttribute("href")).toBe("/crm/report");
  });

  test("repairs a stale nested board URL instead of redirecting in a loop", async () => {
    ui("/crm/board/board/board");

    expect(await screen.findByText(strings.crmFocusTitle)).toBeTruthy();
  });

  test("is the columns the server sent, with the deal's own stored value", async () => {
    ui("/crm/board");

    expect(await screen.findByText("40 seats — Acme GmbH")).toBeTruthy();
    for (const name of ["New", "Qualified", "Lost"]) {
      expect(screen.getByRole("list", { name })).toBeTruthy();
    }
    // The card is in the column the server put it in, and shows the value it
    // stored — no column here adds anything up.
    const column = within(screen.getByRole("list", { name: "New" }));
    expect(column.getByText("40 seats — Acme GmbH")).toBeTruthy();
    expect(column.getByText("Acme GmbH")).toBeTruthy();
    expect(column.getByRole("button", { name: /Acme GmbH/ }).className).toContain("!p-4");
    expect(column.getByText("€25,000.00")).toBeTruthy();
    expect(within(screen.getByRole("list", { name: "Qualified" })).queryByText(DEAL.title)).toBeNull();
    expect(screen.getByRole("heading", { name: strings.crmFocusTitle })).toBeTruthy();
    expect(screen.getByText(strings.crmFocusOpen)).toBeTruthy();
  });

  test("a drag into another column is one move, and asks nothing", async () => {
    ui("/crm/board");
    await screen.findByText(DEAL.title);

    reply("/crm/deals/deal-1/stage", "POST", { deal: { ...DEAL, stageId: QUALIFIED.id } });
    const card = screen.getByText(DEAL.title).closest("[draggable]");
    fireEvent.dragStart(card as HTMLElement);
    fireEvent.drop(screen.getByRole("list", { name: "Qualified" }));

    await waitFor(() => expect(writes().length).toBe(1));
    const move = writes()[0] as Call;
    expect(move.url).toContain("/crm/deals/deal-1/stage");
    expect(move.body).toEqual({ stageId: QUALIFIED.id, position: 1 });
  });

  test("a drag into a losing column asks why first, and sends the reason", async () => {
    ui("/crm/board");
    await screen.findByText(DEAL.title);

    reply("/crm/deals/deal-1/stage", "POST", {
      deal: { ...DEAL, stageId: LOST.id, state: "lost", lostReason: "Price" },
    });
    fireEvent.dragStart(screen.getByText(DEAL.title).closest("[draggable]") as HTMLElement);
    fireEvent.drop(screen.getByRole("list", { name: "Lost" }));

    // Nothing has been sent yet: the question comes before the request, so the
    // server is never asked to refuse a move we know it will refuse.
    const asked = await screen.findByRole("dialog", { name: strings.crmLostTitle });
    expect(writes().length).toBe(0);

    fireEvent.change(within(asked).getByRole("textbox"), { target: { value: "Price" } });
    fireEvent.click(screen.getByRole("button", { name: strings.crmLostConfirm }));

    await waitFor(() => expect(writes().length).toBe(1));
    expect((writes()[0] as Call).body).toEqual({
      stageId: LOST.id,
      position: 1,
      lostReason: "Price",
    });
  });

  test("cancelling that question leaves the deal where it was", async () => {
    ui("/crm/board");
    await screen.findByText(DEAL.title);

    fireEvent.dragStart(screen.getByText(DEAL.title).closest("[draggable]") as HTMLElement);
    fireEvent.drop(screen.getByRole("list", { name: "Lost" }));
    const asked = await screen.findByRole("dialog", { name: strings.crmLostTitle });
    // The footer's Cancel — the header's close button carries the same label.
    const backOut = within(asked).getAllByRole("button", { name: strings.crmCancel });
    fireEvent.click(backOut[backOut.length - 1] as HTMLElement);

    await waitFor(() =>
      expect(within(screen.getByRole("list", { name: "New" })).getByText(DEAL.title)).toBeTruthy(),
    );
    expect(writes()).toEqual([]);
  });
});

describe("the list", () => {
  test("asks the server for the narrowed set rather than filtering a loaded page", async () => {
    ui("/crm/list");
    await screen.findByText(DEAL.title);
    const before = reads("/crm/deals").length;

    fireEvent.change(screen.getByLabelText(strings.crmFilterState), { target: { value: "won" } });
    await waitFor(() => expect(reads("/crm/deals").length).toBeGreaterThan(before));
    expect(reads("/crm/deals").at(-1)?.url).toContain("state=won");

    fireEvent.click(screen.getByLabelText(strings.crmFilterMine));
    // The owner filter is the signed-in user's own id, exactly as the server
    // stores it on a deal.
    await waitFor(() => expect(reads("/crm/deals").at(-1)?.url).toContain("ownerUserId=u-1"));
  });

  test("the search box says it only matches what is already on screen", async () => {
    ui("/crm/list");
    await screen.findByText(DEAL.title);
    const before = reads("/crm/deals").length;

    fireEvent.change(screen.getByLabelText(strings.crmSearchDeals), {
      target: { value: "nothing like this" },
    });

    expect(await screen.findByText(strings.crmNoMatches)).toBeTruthy();
    // No request: a text match over loaded rows is not a question for the API.
    expect(reads("/crm/deals").length).toBe(before);
  });
});

describe("the deal drawer", () => {
  test("moves stage from the visible branded stage choices", async () => {
    ui(`/crm/board?deal=${DEAL.id}`);
    await screen.findByText(strings.crmActivityEmpty);

    reply("/crm/deals/deal-1/stage", "POST", { deal: { ...DEAL, stageId: QUALIFIED.id } });
    fireEvent.click(await screen.findByRole("radio", { name: QUALIFIED.name }));

    await waitFor(() => expect(writes().length).toBe(1));
    expect(writes()[0]?.body).toEqual({ stageId: QUALIFIED.id });
  });

  test("shows the log, the next steps, and only opens the conversations this reader holds", async () => {
    reply("/activities", "GET", { activities: [NOTE] });
    reply("/next-steps", "GET", { nextSteps: [STEP] });
    reply("/threads", "GET", { threads: [MINE, THEIRS] });
    ui(`/crm/board?deal=${DEAL.id}`);

    expect(await screen.findByText(NOTE.body)).toBeTruthy();
    expect(screen.getByRole("dialog", { name: DEAL.title }).getAttribute("aria-modal")).toBe("true");
    const log = within(screen.getByRole("list", { name: strings.crmActivityTitle }));
    // The entry is dated when it HAPPENED, and says what kind it was.
    expect(log.getByText(strings.crmKindCall)).toBeTruthy();
    expect(within(screen.getByRole("list", { name: strings.crmNextStepsTitle })).getByText(STEP.title)).toBeTruthy();
    const conversations = within(screen.getByRole("list", { name: strings.crmThreadsTitle }));
    expect(conversations.getByText(MINE.subject)).toBeTruthy();
    expect(conversations.getByText(THEIRS.subject)).toBeTruthy();

    // One conversation is this reader's and one is not: exactly one "open in
    // mail", and a sentence naming who linked the other.
    expect(screen.getAllByRole("button", { name: strings.crmThreadOpenInMail }).length).toBe(1);
    expect(screen.getByText(strings.crmThreadNotYours)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: strings.crmThreadOpenInMail }));
    expect(await screen.findByText("the mail module")).toBeTruthy();
  });

  test("a note is written with the kind chosen, and the log re-read", async () => {
    ui(`/crm/board?deal=${DEAL.id}`);
    await screen.findByText(strings.crmActivityEmpty);

    fireEvent.click(screen.getByRole("radio", { name: strings.crmKindMeeting }));
    fireEvent.change(screen.getByLabelText(strings.crmActivityPlaceholder), {
      target: { value: "Walked them through the seats." },
    });
    reply("/activities", "POST", { activity: { ...NOTE, kind: "meeting" } });
    reply("/activities", "GET", { activities: [{ ...NOTE, kind: "meeting" }] });
    fireEvent.click(screen.getByRole("button", { name: strings.crmActivityAdd }));

    await waitFor(() => expect(writes().length).toBe(1));
    const written = writes()[0] as Call;
    expect(written.url).toContain(`/crm/deals/${DEAL.id}/activities`);
    // No `happenedAt`: an entry nobody dated happened now, on the server's
    // clock rather than the browser's.
    expect(written.body).toEqual({ kind: "meeting", body: "Walked them through the seats." });
    const log = await screen.findByRole("list", { name: strings.crmActivityTitle });
    expect(within(log).getByText(strings.crmKindMeeting)).toBeTruthy();
  });

  test("suggestions are asked for, never read on open, and link only on a click", async () => {
    reply("/threads", "GET", { threads: [] });
    ui(`/crm/board?deal=${DEAL.id}`);
    await screen.findByText(strings.crmThreadsEmpty);
    expect(reads("/thread-suggestions")).toEqual([]);

    reply("/thread-suggestions", "GET", {
      suggestions: [
        {
          threadId: "thr-9",
          subject: "Seats for next year",
          reason: "address",
          matchedAddress: "ada@acme.test",
          lastMessageAt: "2026-08-06T10:00:00Z",
        },
      ],
    });
    fireEvent.click(screen.getByRole("button", { name: strings.crmThreadSuggest }));

    expect(await screen.findByText("Seats for next year")).toBeTruthy();
    expect(screen.getByText(strings.crmSuggestionAddress("ada@acme.test"))).toBeTruthy();
    // Reading a proposal writes nothing.
    expect(writes()).toEqual([]);

    reply("/threads", "POST", { thread: { ...MINE, threadId: "thr-9" }, created: true });
    reply("/threads", "GET", { threads: [{ ...MINE, threadId: "thr-9" }] });
    fireEvent.click(screen.getByRole("button", { name: strings.crmThreadLink }));

    await waitFor(() => expect(writes().length).toBe(1));
    expect((writes()[0] as Call).body).toEqual({ threadId: "thr-9" });
  });

  test("a next step is a task with a due date, and links back into Tasks", async () => {
    reply("/next-steps", "GET", { nextSteps: [STEP] });
    ui(`/crm/board?deal=${DEAL.id}`);
    await screen.findByText(STEP.title);

    expect(screen.getByRole("button", { name: strings.crmOpenInTasks })).toBeTruthy();

    fireEvent.change(screen.getByLabelText(strings.crmNextStepPlaceholder), {
      target: { value: "Chase the PO" },
    });
    reply("/next-steps", "POST", { nextStep: { ...STEP, id: "tsk-2", title: "Chase the PO" } });
    fireEvent.click(screen.getByRole("button", { name: strings.crmNextStepAdd }));

    await waitFor(() => expect(writes().length).toBe(1));
    // No source link is sent: the deal in the path is the source, always.
    expect(writes()[0]?.body).toEqual({ title: "Chase the PO" });
  });

  test("the deal it shows is the stored one, re-read rather than taken from the board", async () => {
    ui(`/crm/board?deal=${DEAL.id}`);

    await waitFor(() =>
      expect(
        reads(`/crm/deals/${DEAL.id}`).filter(
          (call) => new URL(call.url).pathname.endsWith(`/crm/deals/${DEAL.id}`),
        ).length,
      ).toBe(1),
    );
    expect(await screen.findByText("€25,000.00")).toBeTruthy();
    expect(screen.getByText(strings.crmStateOpen)).toBeTruthy();
  });

  test("the deal's agent panel is on the record, citing the deal's source", async () => {
    ui(`/crm/board?deal=${DEAL.id}`);

    expect(await screen.findByText(strings.recordAgentTitle)).toBeTruthy();
    // The record's own words for where it came from ("Referral").
    expect(
      screen.getByText(strings.recordAgentOriginFrom(DEAL.source)),
    ).toBeTruthy();
  });
});

// ---- closing a deal (B2.08) ---------------------------------------------------

describe("the lost reason", () => {
  test("is a picker that fills the field, and the field is what is sent", async () => {
    ui("/crm/board");
    await screen.findByText(DEAL.title);

    reply("/crm/deals/deal-1/stage", "POST", {
      deal: { ...DEAL, stageId: LOST.id, state: "lost", lostReason: "Timing" },
    });
    fireEvent.dragStart(screen.getByText(DEAL.title).closest("[draggable]") as HTMLElement);
    fireEvent.drop(screen.getByRole("list", { name: "Lost" }));
    const asked = await screen.findByRole("dialog", { name: strings.crmLostTitle });

    // A suggestion is one click, and it lands in the text field rather than
    // replacing it: the stored reason is free text the whole way.
    fireEvent.click(within(asked).getByRole("button", { name: strings.crmLostReasonTiming }));
    expect((within(asked).getByRole("textbox") as HTMLInputElement).value).toBe(
      strings.crmLostReasonTiming,
    );
    // It can still be typed over.
    fireEvent.change(within(asked).getByRole("textbox"), { target: { value: "  Timing  " } });
    fireEvent.click(screen.getByRole("button", { name: strings.crmLostConfirm }));

    await waitFor(() => expect(writes().length).toBe(1));
    expect((writes()[0] as Call).body).toEqual({
      stageId: LOST.id,
      position: 1,
      lostReason: "Timing",
    });
  });

  test("cannot be submitted blank, because a blank reason is not a reason", async () => {
    ui("/crm/board");
    await screen.findByText(DEAL.title);
    fireEvent.dragStart(screen.getByText(DEAL.title).closest("[draggable]") as HTMLElement);
    fireEvent.drop(screen.getByRole("list", { name: "Lost" }));
    const asked = await screen.findByRole("dialog", { name: strings.crmLostTitle });

    expect(screen.getByRole("button", { name: strings.crmLostConfirm }).hasAttribute("disabled")).toBe(
      true,
    );
    fireEvent.change(within(asked).getByRole("textbox"), { target: { value: "   " } });
    expect(screen.getByRole("button", { name: strings.crmLostConfirm }).hasAttribute("disabled")).toBe(
      true,
    );
    expect(writes()).toEqual([]);
  });
});

describe("the handoff to billing", () => {
  test("asks only for what the deal cannot answer, and sends basis points", async () => {
    ui(`/crm/board?deal=${DEAL.id}`);
    await screen.findByText("€25,000.00");

    fireEvent.click(screen.getByRole("button", { name: strings.crmRaiseInvoice }));
    const form = await screen.findByRole("dialog", {
      name: strings.crmRaiseTitle(strings.crmDocumentDraft("invoice")),
    });
    // This deal is priced and is still a lead, so both questions are asked.
    // `exact: false` because a Field's label element also carries its hint.
    const rate = within(form).getByLabelText(strings.crmFieldVatRate, { exact: false });
    const country = within(form).getByLabelText(strings.crmFieldCountry, { exact: false });
    // Nothing is sent until both are answerable.
    expect(screen.getByRole("button", { name: strings.crmRaiseConfirm }).hasAttribute("disabled")).toBe(
      true,
    );

    fireEvent.change(rate, { target: { value: "19" } });
    fireEvent.change(country, { target: { value: "de" } });
    reply("/crm/deals/deal-1/invoice", "POST", {
      invoice: {
        id: "inv-1",
        status: "draft",
        currency: "EUR",
        totals: { grossCents: 2_975_000 },
      },
      deal: { ...DEAL, customerId: "cus-1" },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.crmRaiseConfirm }));

    await waitFor(() => expect(writes().length).toBe(1));
    const written = writes()[0] as Call;
    expect(written.url).toContain(`/crm/deals/${DEAL.id}/invoice`);
    // 19 % is 1900 basis points — a rate never crosses the wire as a float, and
    // the country is upper-cased by the field the user typed into.
    expect(written.body).toEqual({ vatRateBp: 1900, country: "DE" });

    // The answer says what was raised, at the server's own total, and offers to
    // open it in Billing rather than re-rendering a document here.
    expect(
      await screen.findByText(strings.crmRaisedWorth("€29,750.00"), { exact: false }),
    ).toBeTruthy();
    expect(screen.getByRole("link", { name: strings.crmOpenInBilling }).getAttribute("href")).toBe(
      "/billing/invoices/inv-1",
    );
  });

  test("does not ask for a country when the deal already names a customer", async () => {
    reply(`/crm/deals/${DEAL.id}`, "GET", { deal: { ...DEAL, customerId: "cus-1" } });
    ui(`/crm/board?deal=${DEAL.id}`);
    await screen.findByText("€25,000.00");

    fireEvent.click(screen.getByRole("button", { name: strings.crmRaiseQuote }));
    const form = await screen.findByRole("dialog", {
      name: strings.crmRaiseTitle(strings.crmDocumentDraft("quote")),
    });
    expect(within(form).queryByLabelText(strings.crmFieldCountry, { exact: false })).toBeNull();

    fireEvent.change(within(form).getByLabelText(strings.crmFieldVatRate, { exact: false }), {
      target: { value: "21" },
    });
    reply("/crm/deals/deal-1/quote", "POST", {
      quote: { id: "quo-1", status: "draft", currency: "EUR", totals: { grossCents: 3_025_000 } },
      deal: { ...DEAL, customerId: "cus-1" },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.crmRaiseConfirm }));

    await waitFor(() => expect(writes().length).toBe(1));
    expect((writes()[0] as Call).body).toEqual({ vatRateBp: 2100 });
  });

  test("is not offered on a lost deal, which the server would refuse anyway", async () => {
    reply(`/crm/deals/${DEAL.id}`, "GET", {
      deal: { ...DEAL, state: "lost", closed: true, lostReason: "Price" },
    });
    ui(`/crm/board?deal=${DEAL.id}`);
    await screen.findByText(strings.crmLostBecause("Price"));

    expect(screen.queryByRole("button", { name: strings.crmRaiseInvoice })).toBeNull();
    expect(screen.queryByRole("button", { name: strings.crmRaiseQuote })).toBeNull();
  });
});

describe("the handoff to Projects", () => {
  test("a won deal is reviewed before one linked project is created", async () => {
    reply(`/crm/deals/${DEAL.id}`, "GET", {
      deal: { ...DEAL, stageId: WON.id, state: "won", closed: true },
    });
    reply(`/crm/deals/${DEAL.id}/project`, "GET", { project: null });
    ui(`/crm/board?deal=${DEAL.id}`);

    fireEvent.click(await screen.findByRole("button", { name: strings.crmCreateProject }));
    expect(screen.getByRole("heading", { name: strings.crmProjectCreateTitle })).toBeTruthy();
    expect((screen.getByLabelText(strings.crmProjectName) as HTMLInputElement).value).toBe(
      DEAL.title,
    );

    reply(`/crm/deals/${DEAL.id}/project`, "POST", {
      project: {
        dealId: DEAL.id,
        projectId: "project-1",
        projectName: DEAL.title,
        createdBy: "u-1",
        createdAt: "2026-09-01T10:00:00Z",
      },
    });
    fireEvent.click(
      within(screen.getByRole("dialog", { name: strings.crmProjectCreateTitle })).getByRole(
        "button",
        { name: strings.crmProjectCreateConfirm },
      ),
    );

    await screen.findByText(strings.crmDeliveryProject);
    const creation = writes().find((call) => call.url.includes(`/project`));
    expect(creation?.body).toEqual({ name: DEAL.title });
  });
});

describe("the report", () => {
  test("shows the server's figures, keeps the two questions apart, and never sums", async () => {
    ui("/crm/report");

    await waitFor(() => expect(reads("/crm/reports/pipeline").length).toBe(1));
    // The board and both ends of the period are always asked for.
    const asked = reads("/crm/reports/pipeline")[0] as Call;
    expect(asked.url).toContain(`pipelineId=${PIPELINE.id}`);
    expect(asked.url).toMatch(/from=\d{4}-\d{2}-\d{2}/);
    expect(asked.url).toMatch(/to=\d{4}-\d{2}-\d{2}/);

    const open = await screen.findByRole("table", {
      name: strings.crmReportOpenCaption("EUR"),
    });
    // The stage rows are the open board: the server's cents, formatted, never
    // re-added here.
    // Twice: the New row, and the footer's open total — which is the server's
    // own sum, not one this table added up.
    expect(within(open).getAllByText("€25,000.00").length).toBe(2);
    const closed = screen.getByRole("table", { name: strings.crmReportClosedCaption("EUR") });
    expect(within(closed).getByText("€9,000.00")).toBeTruthy();
    expect(within(closed).getByText("€500.00")).toBeTruthy();
    // And the win rate is the server's basis points, read as a percentage.
    expect(screen.getByText(strings.crmReportWinRate("50%", 1, 2))).toBeTruthy();
    // The report is a read: nothing is written by looking at it.
    expect(writes()).toEqual([]);
  });

  test("says nothing closed rather than drawing a zero win rate", async () => {
    reply("/crm/reports/pipeline", "GET", {
      report: {
        ...REPORT,
        currencies: [
          {
            ...REPORT.currencies[0],
            won: tally(0, 0),
            lost: tally(0, 0),
            winRateBp: null,
          },
        ],
      },
    });
    ui("/crm/report");
    expect(await screen.findByText(strings.crmReportNoWinRate)).toBeTruthy();
  });

  test("a board with nothing on it says so", async () => {
    reply("/crm/reports/pipeline", "GET", { report: { ...REPORT, currencies: [] } });
    ui("/crm/report");
    expect(await screen.findByText(strings.crmReportEmptyTitle)).toBeTruthy();
  });
});
