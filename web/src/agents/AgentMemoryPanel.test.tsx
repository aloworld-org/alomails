// The What-I-remember panel's contract (A6.4): the list is drawn from the
// server's answer, the Forget button exists only where the server said it
// would be honoured (`canForget`), forgetting is a DELETE followed by a
// redraw from the server's fresh answer, and a refusal reaches the user in
// the server's own words.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { Agent, AgentMemory } from "../chat/types";
import { strings } from "../i18n";
import { AgentMemoryPanel } from "./AgentMemoryPanel";

interface Call {
  url: string;
  method: string;
}
const calls: Call[] = [];

/** Answers by URL fragment, so a test states only what it cares about. */
let answers: { match: string; status?: number; body: unknown }[] = [];

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  calls.push({ url, method: init?.method ?? "GET" });
  const hit = answers.find((a) => url.includes(a.match));
  return new Response(JSON.stringify(hit?.body ?? {}), {
    status: hit?.status ?? 200,
    headers: { "content-type": "application/json" },
  });
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

const BILLING: Agent = {
  id: "agent-billing",
  handle: "billing",
  name: "Billing",
  description: null,
  disabled: false,
  answers: 3,
  actions: 1,
  lastAt: null,
};

function memory(over: Partial<AgentMemory>): AgentMemory {
  return {
    id: "mem-1",
    fact: "Northstar invoices are net 30",
    learnedFrom: "explicit",
    createdAt: "2026-08-29T10:00:00Z",
    canForget: true,
    ...over,
  };
}

beforeEach(() => {
  calls.length = 0;
  answers = [];
});

afterEach(cleanup);

test("the panel lists the facts, with Forget only where the server will honour it", async () => {
  answers = [
    {
      match: "/agents/agent-billing/memories",
      body: {
        memories: [
          memory({ id: "mem-1", fact: "the X100 ships from Ghent" }),
          memory({
            id: "mem-2",
            fact: "Northstar invoices are net 30",
            canForget: false,
          }),
        ],
      },
    },
  ];
  render(
    <AgentMemoryPanel
      channel="room-1"
      agent={BILLING}
      aboutYou={false}
      onClose={() => {}}
    />,
  );

  expect(await screen.findByText("the X100 ships from Ghent")).toBeTruthy();
  expect(screen.getByText("Northstar invoices are net 30")).toBeTruthy();
  // One fact is the reader's to forget, the other is not — no button that
  // would only earn a 403.
  expect(
    screen.getByLabelText(
      strings.agentMemoryForgetFact("the X100 ships from Ghent"),
    ),
  ).toBeTruthy();
  expect(
    screen.queryByLabelText(
      strings.agentMemoryForgetFact("Northstar invoices are net 30"),
    ),
  ).toBeNull();
});

test("forgetting asks the server and redraws from its fresh answer", async () => {
  const first = {
    memories: [
      memory({ id: "mem-1", fact: "the X100 ships from Ghent" }),
      memory({ id: "mem-2", fact: "Northstar invoices are net 30" }),
    ],
  };
  answers = [{ match: "/agents/agent-billing/memories", body: first }];
  render(
    <AgentMemoryPanel
      channel="room-1"
      agent={BILLING}
      aboutYou={false}
      onClose={() => {}}
    />,
  );
  await screen.findByText("the X100 ships from Ghent");

  // After the DELETE the reload must see the shorter list.
  answers = [
    { match: "/chat/memories/mem-1", body: {} },
    {
      match: "/agents/agent-billing/memories",
      body: {
        memories: [memory({ id: "mem-2", fact: "Northstar invoices are net 30" })],
      },
    },
  ];
  fireEvent.click(
    screen.getByLabelText(
      strings.agentMemoryForgetFact("the X100 ships from Ghent"),
    ),
  );

  await waitFor(() =>
    expect(screen.queryByText("the X100 ships from Ghent")).toBeNull(),
  );
  const forgot = calls.find((c) => c.method === "DELETE");
  expect(forgot?.url).toContain("/chat/memories/mem-1");
  expect(screen.getByText("Northstar invoices are net 30")).toBeTruthy();
});

test("a refusal reaches the user in the server's own words", async () => {
  answers = [
    {
      match: "/agents/agent-billing/memories",
      body: { memories: [memory({ id: "mem-1" })] },
    },
  ];
  render(
    <AgentMemoryPanel
      channel="room-1"
      agent={BILLING}
      aboutYou={false}
      onClose={() => {}}
    />,
  );
  await screen.findByText("Northstar invoices are net 30");

  answers = [
    {
      match: "/chat/memories/mem-1",
      status: 403,
      body: {
        detail:
          "only the room's owner, or the person whose words taught it, can forget this",
      },
    },
    {
      match: "/agents/agent-billing/memories",
      body: { memories: [memory({ id: "mem-1" })] },
    },
  ];
  fireEvent.click(
    screen.getByLabelText(
      strings.agentMemoryForgetFact("Northstar invoices are net 30"),
    ),
  );

  expect((await screen.findByRole("alert")).textContent).toContain(
    "only the room's owner",
  );
});
