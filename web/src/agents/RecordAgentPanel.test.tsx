// The record panel's contract (A8.4): the origin is said in words from what
// the record view carries; the verbs offered are exactly the directory's ∩
// the catalogue's — never one the boundary would refuse; a verb opens the
// agent's one-to-one with the words pre-filled rather than running anything;
// an ask is posted as the person and the agent's reply shown in place; and a
// record with no origin says so, offering nothing it cannot do.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { RecordAgentPanel } from "./RecordAgentPanel";

interface Call {
  url: string;
  method: string;
  body: string | null;
}
const calls: Call[] = [];

/** Answers by URL fragment and method, so a test states only what it cares
 *  about. */
let answers: {
  match: string;
  method?: string;
  status?: number;
  body: unknown;
}[] = [];

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({
    url,
    method,
    body: typeof init?.body === "string" ? init.body : null,
  });
  const hit = answers.find(
    (a) => url.includes(a.match) && (a.method ?? "GET") === method,
  );
  return new Response(JSON.stringify(hit?.body ?? {}), {
    status: hit?.status ?? 200,
    headers: { "content-type": "application/json" },
  });
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

const navigateSpy = vi.fn();
vi.mock("react-router-dom", () => ({
  useNavigate: () => navigateSpy,
}));

/** The directory as the wire answers it: the Tasks agent offers two of the
 *  catalogue's four record verbs, so only those two may become buttons. */
const DIRECTORY = {
  agents: [
    {
      id: "agent-tasks",
      handle: "tasks",
      name: "Tasks",
      product: "tasks",
      disabled: false,
      tools: [
        { name: "task_lookup", effect: "read" },
        { name: "chase_task", effect: "write" },
        { name: "set_task_priority", effect: "write" },
      ],
    },
  ],
};

beforeEach(() => {
  calls.length = 0;
  answers = [{ match: "/chat/agents/directory", body: DIRECTORY }];
  navigateSpy.mockClear();
});

afterEach(cleanup);

test("the origin is said in words, and only the offered verbs become buttons", async () => {
  render(
    <RecordAgentPanel
      product="tasks"
      recordKind="task"
      recordId="task-1"
      recordLabel="Pricing sheet"
      origin={{ kind: "person", id: "u-1", label: "disan@alo.dev" }}
    />,
  );

  expect(
    await screen.findByText(strings.recordAgentOriginPerson("disan@alo.dev")),
  ).toBeTruthy();
  expect(
    await screen.findByText(strings.recordAgentVerbChaseTask),
  ).toBeTruthy();
  expect(screen.getByText(strings.recordAgentVerbSetTaskPriority)).toBeTruthy();
  // The directory did not offer completing or reassigning — no button that
  // would only earn a refusal at the boundary.
  expect(screen.queryByText(strings.recordAgentVerbCompleteTask)).toBeNull();
  expect(screen.queryByText(strings.recordAgentVerbReassignTask)).toBeNull();
});

test("a thread origin without a name is cited by the room's own name", async () => {
  answers.push({
    match: "/chat/channels/room-7",
    body: { id: "room-7", name: "friday" },
  });
  render(
    <RecordAgentPanel
      product="tasks"
      recordKind="task"
      recordId="task-1"
      recordLabel="Pricing sheet"
      origin={{ kind: "thread", id: "room-7", label: null }}
    />,
  );

  expect(
    await screen.findByText(strings.recordAgentOriginThread("friday")),
  ).toBeTruthy();
});

test("a verb opens the agent's one-to-one with the words pre-filled, running nothing", async () => {
  answers.push({
    match: "/chat/agents/agent-tasks/dm",
    method: "POST",
    body: { id: "dm-1" },
  });
  render(
    <RecordAgentPanel
      product="tasks"
      recordKind="task"
      recordId="task-1"
      recordLabel="Pricing sheet"
      origin={null}
    />,
  );

  fireEvent.click(await screen.findByText(strings.recordAgentVerbChaseTask));

  await waitFor(() =>
    expect(navigateSpy).toHaveBeenCalledWith(
      `/chat?channel=dm-1&draft=${encodeURIComponent(
        strings.recordAgentDraftChaseTask("Pricing sheet"),
      )}`,
    ),
  );
  // The panel opened the room and navigated — it executed nothing and said
  // nothing on the person's behalf.
  expect(calls.some((c) => c.url.includes("/messages"))).toBe(false);
  expect(calls.some((c) => c.url.includes("/execute"))).toBe(false);
});

test("an ask goes to the agent with the record named, and the answer shows in place", async () => {
  answers.push(
    {
      match: "/chat/agents/agent-tasks/dm",
      method: "POST",
      body: { id: "dm-1" },
    },
    {
      match: "/chat/channels/dm-1/messages",
      method: "POST",
      body: { seq: 3 },
    },
    {
      match: "/chat/channels/dm-1/messages",
      body: {
        messages: [
          {
            seq: 4,
            authorKind: "agent",
            kind: "text",
            body: "It is two days overdue; the assignee was reminded last Friday.",
          },
        ],
      },
    },
  );
  render(
    <RecordAgentPanel
      product="tasks"
      recordKind="task"
      recordId="task-1"
      recordLabel="Pricing sheet"
      origin={null}
    />,
  );

  fireEvent.change(
    await screen.findByLabelText(strings.recordAgentAskPlaceholder("tasks")),
    { target: { value: "where has this got to?" } },
  );
  fireEvent.click(screen.getByText(strings.recordAgentAsk));

  expect(
    (
      await screen.findByText(
        "It is two days overdue; the assignee was reminded last Friday.",
      )
    ).textContent,
  ).toBeTruthy();
  // The words were posted as the person, with the record named as context.
  const posted = calls.find(
    (c) => c.method === "POST" && c.url.includes("/chat/channels/dm-1/messages"),
  );
  expect(posted?.body).toContain(
    JSON.stringify(
      strings.recordAgentAskAbout("Pricing sheet", "where has this got to?"),
    ).slice(1, -1),
  );
  expect(screen.getByText(strings.recordAgentOpenConversation)).toBeTruthy();
});

test("a verb bound to a record kind is offered on that kind and no other", async () => {
  const finance = {
    agents: [
      {
        id: "agent-finance",
        handle: "finance",
        name: "Finance",
        product: "finance",
        disabled: false,
        tools: [
          { name: "approve_expense", effect: "write" },
          { name: "categorise_transactions", effect: "write" },
        ],
      },
    ],
  };
  answers = [{ match: "/chat/agents/directory", body: finance }];
  render(
    <RecordAgentPanel
      product="finance"
      recordKind="approval"
      recordId="exp-1"
      recordLabel="Bakkerij Van Damme"
      origin={{ kind: "person", id: "u-2", label: "amara@alo.dev" }}
    />,
  );

  // Approving is the approvals queue's verb; suggesting categories reads the
  // asker's own claims and is the expense editor's — not this record's.
  expect(
    await screen.findByText(strings.recordAgentVerbApproveExpense),
  ).toBeTruthy();
  expect(
    screen.queryByText(strings.recordAgentVerbSuggestCategories),
  ).toBeNull();

  cleanup();
  render(
    <RecordAgentPanel
      product="finance"
      recordKind="expense"
      recordId="exp-2"
      recordLabel="NS International"
      origin={null}
    />,
  );
  expect(
    await screen.findByText(strings.recordAgentVerbSuggestCategories),
  ).toBeTruthy();
  expect(
    screen.queryByText(strings.recordAgentVerbApproveExpense),
  ).toBeNull();
});

test("a Drive folder is offered what takes a folder, and a file what takes a file", async () => {
  const drive = {
    agents: [
      {
        id: "agent-drive",
        handle: "drive",
        name: "Drive",
        product: "drive",
        disabled: false,
        tools: [
          { name: "list_folder", effect: "read" },
          { name: "file_rename", effect: "write" },
          { name: "file_move", effect: "write" },
        ],
      },
    ],
  };
  answers = [{ match: "/chat/agents/directory", body: drive }];
  render(
    <RecordAgentPanel
      product="drive"
      recordKind="folder"
      recordId="node-1"
      recordLabel="Contracts"
      origin={null}
    />,
  );

  expect(
    await screen.findByText(strings.recordAgentVerbListFolder),
  ).toBeTruthy();
  expect(screen.queryByText(strings.recordAgentVerbRenameFile)).toBeNull();
  expect(screen.queryByText(strings.recordAgentVerbMoveFile)).toBeNull();

  cleanup();
  render(
    <RecordAgentPanel
      product="drive"
      recordKind="file"
      recordId="node-2"
      recordLabel="Delaunay quote.pdf"
      // The file was saved out of an email, which is what Drive keeps of it.
      origin={{ kind: "message", id: "msg-9", label: null }}
    />,
  );
  expect(
    await screen.findByText(strings.recordAgentOriginEmail, { exact: false }),
  ).toBeTruthy();
  expect(screen.getByText(strings.recordAgentVerbRenameFile)).toBeTruthy();
  expect(screen.getByText(strings.recordAgentVerbMoveFile)).toBeTruthy();
  expect(screen.queryByText(strings.recordAgentVerbListFolder)).toBeNull();
});

test("a meeting's verbs are the meeting's, and its draft names it", async () => {
  answers = [
    {
      match: "/chat/agents/directory",
      body: {
        agents: [
          {
            id: "agent-agenda",
            handle: "agenda",
            name: "Agenda",
            product: "agenda",
            disabled: false,
            tools: [
              { name: "meeting_prep", effect: "read" },
              { name: "cancel_event", effect: "write" },
            ],
          },
        ],
      },
    },
    { match: "/chat/agents/agent-agenda/dm", method: "POST", body: { id: "dm-2" } },
  ];
  render(
    <RecordAgentPanel
      product="agenda"
      recordKind="event"
      recordId="ev-1"
      recordLabel="Delaunay review"
      origin={null}
    />,
  );

  expect(
    await screen.findByText(strings.recordAgentVerbMeetingPrep),
  ).toBeTruthy();
  expect(screen.getByText(strings.recordAgentVerbCancelEvent)).toBeTruthy();
  // Rescheduling was not offered by this directory, so it is not a button.
  expect(screen.queryByText(strings.recordAgentVerbRescheduleEvent)).toBeNull();

  fireEvent.click(screen.getByText(strings.recordAgentVerbCancelEvent));
  await waitFor(() =>
    expect(navigateSpy).toHaveBeenCalledWith(
      `/chat?channel=dm-2&draft=${encodeURIComponent(
        strings.recordAgentDraftCancelEvent("Delaunay review"),
      )}`,
    ),
  );
  // Cancelling a meeting emails every guest: the panel proposes it in the
  // room and cancels nothing itself.
  expect(calls.some((c) => c.url.includes("/execute"))).toBe(false);
});

test("an imported record cites the file it was read from", async () => {
  render(
    <RecordAgentPanel
      product="finance"
      recordKind="bankStatement"
      recordId="st-1"
      recordLabel="2026-07"
      origin={{ kind: "import", id: "st-1", label: "CAMT" }}
    />,
  );
  expect(
    await screen.findByText(strings.recordAgentOriginImport("CAMT")),
  ).toBeTruthy();
});

test("mail's two records are offered their own verbs, and a message says who sent it", async () => {
  // One agent works in the mail and the address book (ADR 0034), so the two
  // record kinds share a directory entry and must not share verbs.
  answers = [
    {
      match: "/chat/agents/directory",
      body: {
        agents: [
          {
            id: "agent-mail",
            handle: "mail",
            name: "Mail",
            product: "mail",
            disabled: false,
            tools: [
              { name: "draft_reply", effect: "write" },
              { name: "thread_lookup", effect: "read" },
              { name: "correspondence", effect: "read" },
              { name: "draft_email", effect: "write" },
            ],
          },
        ],
      },
    },
  ];
  render(
    <RecordAgentPanel
      product="mail"
      recordKind="message"
      recordId="msg-1"
      recordLabel="Delivery on Friday"
      origin={{ kind: "sender", id: "msg-1", label: "Ilse Vermeer" }}
    />,
  );

  expect(
    await screen.findByText(strings.recordAgentOriginSender("Ilse Vermeer")),
  ).toBeTruthy();
  expect(screen.getByText(strings.recordAgentVerbDraftReply)).toBeTruthy();
  expect(screen.getByText(strings.recordAgentVerbThreadLookup)).toBeTruthy();
  // Writing to somebody is a card's verb, not a conversation's.
  expect(screen.queryByText(strings.recordAgentVerbWriteToThem)).toBeNull();
  expect(screen.queryByText(strings.recordAgentVerbCorrespondence)).toBeNull();

  cleanup();
  render(
    <RecordAgentPanel
      product="mail"
      recordKind="contact"
      recordId="c-1"
      recordLabel="Ilse Vermeer"
      origin={null}
    />,
  );
  expect(
    await screen.findByText(strings.recordAgentVerbCorrespondence),
  ).toBeTruthy();
  expect(screen.getByText(strings.recordAgentVerbWriteToThem)).toBeTruthy();
  expect(screen.queryByText(strings.recordAgentVerbDraftReply)).toBeNull();
});

test("a board is offered the board's verb and a chart the chart's", async () => {
  answers = [
    {
      match: "/chat/agents/directory",
      body: {
        agents: [
          {
            id: "agent-insights",
            handle: "insights",
            name: "Insights",
            product: "insights",
            disabled: false,
            tools: [
              { name: "insight_change", effect: "read" },
              { name: "pin_chart", effect: "write" },
            ],
          },
        ],
      },
    },
  ];
  render(
    <RecordAgentPanel
      product="insights"
      recordKind="board"
      recordId="board-1"
      recordLabel="Sales"
      origin={null}
    />,
  );
  expect(await screen.findByText(strings.recordAgentVerbPinChart)).toBeTruthy();
  expect(screen.queryByText(strings.recordAgentVerbInsightChange)).toBeNull();

  cleanup();
  render(
    <RecordAgentPanel
      product="insights"
      recordKind="tile"
      recordId="tile-1"
      recordLabel="Revenue by month"
      origin={null}
    />,
  );
  expect(
    await screen.findByText(strings.recordAgentVerbInsightChange),
  ).toBeTruthy();
  expect(screen.queryByText(strings.recordAgentVerbPinChart)).toBeNull();
});

test("a room's verb proposes in the agent's room and reads nothing on its own", async () => {
  answers = [
    {
      match: "/chat/agents/directory",
      body: {
        agents: [
          {
            id: "agent-chat",
            handle: "chat",
            name: "Chat",
            product: "chat",
            disabled: false,
            tools: [{ name: "catch_up_room", effect: "read" }],
          },
        ],
      },
    },
    { match: "/chat/agents/agent-chat/dm", method: "POST", body: { id: "dm-3" } },
  ];
  render(
    <RecordAgentPanel
      product="chat"
      recordKind="room"
      recordId="room-1"
      recordLabel="release"
      origin={{ kind: "person", id: "u-3", label: "disan@alo.dev" }}
    />,
  );

  fireEvent.click(await screen.findByText(strings.recordAgentVerbCatchUpRoom));
  await waitFor(() =>
    expect(navigateSpy).toHaveBeenCalledWith(
      `/chat?channel=dm-3&draft=${encodeURIComponent(
        strings.recordAgentDraftCatchUpRoom("release"),
      )}`,
    ),
  );
  // Finding was not offered by this directory, so it is not a button.
  expect(screen.queryByText(strings.recordAgentVerbFindInRoom)).toBeNull();
  expect(calls.some((c) => c.url.includes("/chat/search"))).toBe(false);
});

test("a record with no origin says so, and a product without an agent is offered nothing", async () => {
  answers = [{ match: "/chat/agents/directory", body: { agents: [] } }];
  render(
    <RecordAgentPanel
      product="tasks"
      recordKind="task"
      recordId="task-1"
      recordLabel="Pricing sheet"
      origin={null}
    />,
  );

  expect(
    await screen.findByText(strings.recordAgentOriginNone),
  ).toBeTruthy();
  await waitFor(() =>
    expect(
      screen.queryByLabelText(strings.recordAgentAskPlaceholder("tasks")),
    ).toBeNull(),
  );
  expect(screen.queryByText(strings.recordAgentVerbChaseTask)).toBeNull();
});
