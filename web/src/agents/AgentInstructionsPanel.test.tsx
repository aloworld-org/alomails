// The standing-instruction card's contract (A7.2): the cards are drawn from
// the server's answer, Cancel exists only where the server said it would be
// honoured (`canCancel`), a paused card says so, cancelling is a DELETE
// followed by a redraw from the server's fresh answer, and a refusal reaches
// the user in the server's own words.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { Agent, AgentInstruction } from "../chat/types";
import { strings } from "../i18n";
import { AgentInstructionsPanel } from "./AgentInstructionsPanel";

interface Call {
  url: string;
  method: string;
  body: string | null;
}
const calls: Call[] = [];

/** Answers by URL fragment, so a test states only what it cares about. */
let answers: { match: string; status?: number; body: unknown }[] = [];

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  calls.push({
    url,
    method: init?.method ?? "GET",
    body: typeof init?.body === "string" ? init.body : null,
  });
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

function card(over: Partial<AgentInstruction>): AgentInstruction {
  return {
    id: "ins-1",
    agentId: "agent-billing",
    agentHandle: "billing",
    text: "list the invoices that fell overdue",
    trigger: { kind: "schedule", everyMinutes: 60 },
    nextRun: "2026-08-30T09:00:00Z",
    lastFiredAt: null,
    paused: false,
    author: "sam@acme.eu",
    createdAt: "2026-08-29T10:00:00Z",
    canCancel: true,
    ...over,
  };
}

beforeEach(() => {
  calls.length = 0;
  answers = [
    { match: "/instructions", body: { instructions: [] } },
    { match: "/agents", body: { agents: [BILLING] } },
  ];
});

afterEach(cleanup);

test("the cards are listed, with Cancel only where the server will honour it", async () => {
  answers = [
    {
      match: "/instructions",
      body: {
        instructions: [
          card({ id: "ins-1", text: "list the invoices that fell overdue" }),
          card({
            id: "ins-2",
            text: "chase the unpaid quotes",
            canCancel: false,
            trigger: { kind: "event", event: "issue_invoice" },
            nextRun: null,
          }),
        ],
      },
    },
    { match: "/agents", body: { agents: [BILLING] } },
  ];
  render(<AgentInstructionsPanel channel="room-1" onClose={() => {}} />);

  expect(
    await screen.findByText("list the invoices that fell overdue"),
  ).toBeTruthy();
  expect(screen.getByText("chase the unpaid quotes")).toBeTruthy();
  // The schedule and the event each read as a sentence.
  expect(
    screen.getByText(strings.agentInstructionHourly, { exact: false }),
  ).toBeTruthy();
  expect(
    screen.getByText(strings.agentInstructionOnEvent("issue_invoice"), {
      exact: false,
    }),
  ).toBeTruthy();
  // One card is this reader's to cancel, the other is not — no button that
  // would only earn a 403.
  expect(
    screen.getByLabelText(
      strings.agentInstructionCancelThis("list the invoices that fell overdue"),
    ),
  ).toBeTruthy();
  expect(
    screen.queryByLabelText(
      strings.agentInstructionCancelThis("chase the unpaid quotes"),
    ),
  ).toBeNull();
});

test("a paused card says so, and shows no next run", async () => {
  answers = [
    {
      match: "/instructions",
      body: { instructions: [card({ paused: true })] },
    },
    { match: "/agents", body: { agents: [BILLING] } },
  ];
  render(<AgentInstructionsPanel channel="room-1" onClose={() => {}} />);

  expect(await screen.findByText(strings.agentInstructionPaused)).toBeTruthy();
  expect(
    screen.queryByText(strings.agentInstructionNextRun(""), { exact: false }),
  ).toBeNull();
});

test("cancelling asks the server and redraws from its fresh answer", async () => {
  answers = [
    {
      match: "/instructions",
      body: { instructions: [card({ id: "ins-1" })] },
    },
    { match: "/agents", body: { agents: [BILLING] } },
  ];
  render(<AgentInstructionsPanel channel="room-1" onClose={() => {}} />);
  await screen.findByText("list the invoices that fell overdue");

  // After the DELETE the reload must see the shorter list.
  answers = [
    { match: "/chat/instructions/ins-1", body: {} },
    { match: "/instructions", body: { instructions: [] } },
    { match: "/agents", body: { agents: [BILLING] } },
  ];
  fireEvent.click(
    screen.getByLabelText(
      strings.agentInstructionCancelThis("list the invoices that fell overdue"),
    ),
  );

  await waitFor(() =>
    expect(
      screen.queryByText("list the invoices that fell overdue"),
    ).toBeNull(),
  );
  const cancelled = calls.find((c) => c.method === "DELETE");
  expect(cancelled?.url).toContain("/chat/instructions/ins-1");
});

test("the form stands a scheduled instruction up and the new card appears", async () => {
  render(<AgentInstructionsPanel channel="room-1" onClose={() => {}} />);
  await screen.findByText(strings.agentInstructionsEmpty);

  answers = [
    { match: "/instructions", body: { instructions: [card({})] } },
    { match: "/agents", body: { agents: [BILLING] } },
  ];
  fireEvent.change(screen.getByLabelText(strings.agentInstructionAgentLabel), {
    target: { value: "agent-billing" },
  });
  fireEvent.change(screen.getByLabelText(strings.agentInstructionTextLabel), {
    target: { value: "list the invoices that fell overdue" },
  });
  fireEvent.click(screen.getByText(strings.agentInstructionAdd));

  expect(
    await screen.findByText("list the invoices that fell overdue"),
  ).toBeTruthy();
  const posted = calls.find((c) => c.method === "POST");
  expect(posted?.url).toContain("/chat/channels/room-1/instructions");
  expect(JSON.parse(posted?.body ?? "{}")).toEqual({
    agentId: "agent-billing",
    text: "list the invoices that fell overdue",
    trigger: { kind: "schedule", everyMinutes: 60 },
  });
});

test("a refusal reaches the user in the server's own words", async () => {
  render(<AgentInstructionsPanel channel="room-1" onClose={() => {}} />);
  await screen.findByText(strings.agentInstructionsEmpty);

  answers = [
    {
      match: "/instructions",
      status: 422,
      body: { detail: "this channel already holds twenty standing instructions" },
    },
    { match: "/agents", body: { agents: [BILLING] } },
  ];
  fireEvent.change(screen.getByLabelText(strings.agentInstructionAgentLabel), {
    target: { value: "agent-billing" },
  });
  fireEvent.change(screen.getByLabelText(strings.agentInstructionTextLabel), {
    target: { value: "one instruction too many" },
  });
  fireEvent.click(screen.getByText(strings.agentInstructionAdd));

  expect((await screen.findByRole("alert")).textContent).toContain(
    "twenty standing instructions",
  );
});

test("a room with no agents offers no form", async () => {
  answers = [
    { match: "/instructions", body: { instructions: [] } },
    { match: "/agents", body: { agents: [] } },
  ];
  render(<AgentInstructionsPanel channel="room-1" onClose={() => {}} />);

  await screen.findByText(strings.agentInstructionsEmpty);
  expect(screen.queryByText(strings.agentInstructionAdd)).toBeNull();
});
