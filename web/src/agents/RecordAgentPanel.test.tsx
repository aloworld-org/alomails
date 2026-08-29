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
