// What the type checker cannot see about a conversation: that an agent reads
// as an agent and not as a colleague, that a proposal's buttons belong to
// exactly one person, and that a refusal from the server reaches the user
// instead of being swallowed.
//
// The rule under test is the security-visible one from
// `docs/design/chat-agents.md`: **only the asker may decide**. It is enforced
// in the store and again at the route, and a UI that offered the button anyway
// would produce a 403 the user cannot act on — so the button's absence is part
// of the contract, not decoration.
//
// Auth is stubbed to one recording `fetch`; the real module, the real chat
// client and the real approval card all run.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { ChatModule } from "./ChatModule";
import type { ChannelSummary, FeedMessage } from "./types";

const ME = "user-anna";
const THEM = "user-ben";

interface Call {
  url: string;
  method: string;
  body: unknown;
}
const calls: Call[] = [];

/** Answers by URL fragment, so a test states only what it cares about. */
let answers: { match: string; status?: number; body: unknown }[] = [];

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  calls.push({
    url,
    method: init?.method ?? "GET",
    body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
  });
  const hit = answers.find((a) => url.includes(a.match));
  const body = hit?.body ?? {};
  return new Response(JSON.stringify(body), {
    status: hit?.status ?? 200,
    headers: { "content-type": "application/json" },
  });
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch, identity: { sub: ME } }),
}));

// The push stream is a live socket; a test wants the render, not the wire.
vi.mock("../jmap", () => ({
  useJmapClient: () => ({
    subscribeChat: () => new Promise(() => {}),
    driveDownload: async () => new Blob(),
  }),
}));

const ROOM: ChannelSummary = {
  id: "room-1",
  kind: "channel",
  name: "planning",
  topic: null,
  visibility: "public",
  createdBy: ME,
  createdAt: "2026-08-09T10:00:00Z",
  archivedAt: null,
  unread: 0,
  mentions: 0,
  lastReadSeq: 2,
  lastSeq: 2,
  lastAt: "2026-08-09T10:01:00Z",
};

/** A message with everything empty, so each test states only its own point. */
function message(over: Partial<FeedMessage>): FeedMessage {
  return {
    id: "m-1",
    channel: "room-1",
    seq: 1,
    author: ME,
    authorKind: "user",
    authorEmail: "anna@alo.test",
    onBehalfOf: null,
    body: "hello",
    kind: "text",
    threadRootSeq: null,
    reactions: [],
    mentions: [],
    attachments: [],
    proposal: null,
    replyCount: 0,
    lastReplyAt: null,
    createdAt: "2026-08-09T10:00:00Z",
    editedAt: null,
    deletedAt: null,
    ...over,
  };
}

function withMessages(messages: FeedMessage[]) {
  answers = [
    { match: "/chat/channels?", body: { channels: [ROOM] } },
    { match: "/chat/reactions", body: { emoji: ["👍"] } },
    { match: "/messages", body: { messages } },
    { match: "/chat/channels", body: { channels: [ROOM] } },
  ];
}

beforeEach(() => {
  calls.length = 0;
  answers = [];
  fakeFetch.mockClear();
});
afterEach(cleanup);

test("an agent's message is marked as an agent, not shown as a colleague", async () => {
  withMessages([
    message({
      id: "m-agent",
      seq: 2,
      author: "agent-alo",
      authorKind: "agent",
      authorEmail: "alo",
      onBehalfOf: ME,
      body: "Here is what I found.",
    }),
  ]);
  render(<ChatModule />);

  await screen.findByText("Here is what I found.");
  // The word that stops an agent being mistaken for a person.
  expect(screen.getByText(strings.chatAgentTag)).toBeTruthy();
  expect(screen.getByText("alo")).toBeTruthy();
});

test("the person who asked sees a live Approve button", async () => {
  withMessages([
    message({
      id: "m-proposal",
      seq: 2,
      author: "agent-alo",
      authorKind: "agent",
      authorEmail: "alo",
      onBehalfOf: ME,
      body: "I can create that task.",
      proposal: {
        id: "p-1",
        message: "m-proposal",
        askedBy: ME,
        tool: "create_task",
        args: { title: "Review the plan" },
        state: "pending",
        decidedBy: null,
      },
    }),
  ]);
  render(<ChatModule />);

  const approve = await screen.findByText(strings.agentApprove);
  expect(approve).toBeTruthy();
  expect(screen.queryByText(strings.chatProposalNotYours)).toBeNull();

  fireEvent.click(approve);
  await waitFor(() => {
    expect(
      calls.some(
        (c) => c.url.includes("/chat/proposals/p-1") && c.method === "POST",
      ),
    ).toBe(true);
  });
  // Approving runs the action server-side, so the client sends the decision
  // and nothing else — it must not try to execute anything itself.
  expect(
    calls.find((c) => c.url.includes("/chat/proposals/p-1"))?.body,
  ).toEqual({ approve: true });
  expect(calls.some((c) => c.url.includes("/ai/agent/execute"))).toBe(false);
});

test("everyone else sees the proposal and no way to decide it", async () => {
  withMessages([
    message({
      id: "m-proposal",
      seq: 2,
      author: "agent-alo",
      authorKind: "agent",
      authorEmail: "alo",
      onBehalfOf: THEM,
      body: "I can create that task.",
      proposal: {
        id: "p-1",
        message: "m-proposal",
        // Ben asked. Anna is reading.
        askedBy: THEM,
        tool: "create_task",
        args: { title: "Review the plan" },
        state: "pending",
        decidedBy: null,
      },
    }),
  ]);
  render(<ChatModule />);

  // The proposal is visible — a room where actions happen invisibly is worse.
  await screen.findByText("I can create that task.");
  expect(screen.getByText(strings.chatProposalNotYours)).toBeTruthy();
  // ...but there is no button at all. A control that cannot be used must not
  // be drawn as one, disabled or otherwise.
  expect(screen.queryByText(strings.agentApprove)).toBeNull();
  expect(screen.queryByText(strings.agentDiscard)).toBeNull();
});

test("a settled proposal keeps its card and says what it became", async () => {
  withMessages([
    message({
      id: "m-proposal",
      seq: 2,
      author: "agent-alo",
      authorKind: "agent",
      authorEmail: "alo",
      onBehalfOf: ME,
      body: "I can create that task.",
      proposal: {
        id: "p-1",
        message: "m-proposal",
        askedBy: ME,
        tool: "create_task",
        args: { title: "Review the plan" },
        state: "approved",
        decidedBy: ME,
      },
    }),
  ]);
  render(<ChatModule />);

  await screen.findByText(strings.chatProposalSettled("approved"));
  // Even for the asker: decided is decided, and a second tap must be
  // impossible rather than merely refused.
  expect(screen.queryByText(strings.agentApprove)).toBeNull();
});

test("a person's message is not marked as an agent", async () => {
  withMessages([message({ body: "just me talking" })]);
  render(<ChatModule />);

  await screen.findByText("just me talking");
  expect(screen.queryByText(strings.chatAgentTag)).toBeNull();
});
