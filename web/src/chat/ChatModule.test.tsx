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

/** The composer's `@` list must offer agents, because an agent is the thing a
 *  person is least likely to know is in the room at all. */
function withRoomPeople(messages: FeedMessage[]) {
  answers = [
    { match: "/chat/reactions", body: { emoji: ["👍"] } },
    { match: "/agents", body: { agents: [AGENT] } },
    { match: "/messages", body: { messages } },
    {
      match: "/chat/channels/room-1",
      body: {
        ...ROOM,
        members: [
          {
            user: ME,
            email: "anna@alo.test",
            role: "owner",
            joinedAt: "",
            lastReadSeq: 0,
            muted: false,
          },
          {
            user: THEM,
            email: "ben@alo.test",
            role: "member",
            joinedAt: "",
            lastReadSeq: 0,
            muted: false,
          },
        ],
        myRole: "owner",
      },
    },
    { match: "/chat/channels", body: { channels: [ROOM] } },
  ];
}

const AGENT = {
  id: "agent-alo",
  handle: "alo",
  name: "alo",
  description: "asks and answers",
  disabled: false,
};

test("typing @ offers the room's agents and people, agents first", async () => {
  withRoomPeople([message({ body: "hi" })]);
  render(<ChatModule />);
  const box = await screen.findByLabelText(strings.chatComposerLabel);

  // Nothing offered until an '@' opens a mention.
  expect(screen.queryByText("@alo")).toBeNull();

  fireEvent.change(box, { target: { value: "@a", selectionStart: 2 } });
  await screen.findByText("@alo");
  // Anna matches "@a" too, but the agent leads: it is the discovery.
  const offered = screen.getAllByRole("option").map((o) => o.textContent ?? "");
  expect(offered[0]).toContain("@alo");
  expect(offered.some((o) => o.includes("@anna"))).toBe(true);
});

test("an address typed inline is not a mention, so nothing is offered", async () => {
  withRoomPeople([message({ body: "hi" })]);
  render(<ChatModule />);
  const box = await screen.findByLabelText(strings.chatComposerLabel);

  // The token after this '@' is exactly "alo", which IS an agent handle — so
  // if the boundary rule were dropped, the list would offer @alo here. The
  // server's parser refuses it, and the composer must agree: offering a
  // completion the server then declines to resolve would be a lie.
  fireEvent.change(box, {
    target: { value: "write to ben@alo", selectionStart: 16 },
  });
  await waitFor(() => {
    expect(screen.queryAllByRole("option")).toHaveLength(0);
  });
});

test("choosing a name puts it in the message", async () => {
  withRoomPeople([message({ body: "hi" })]);
  render(<ChatModule />);
  const box = await screen.findByLabelText(strings.chatComposerLabel);

  fireEvent.change(box, { target: { value: "@al", selectionStart: 3 } });
  const choice = await screen.findByText("@alo");
  fireEvent.mouseDown(choice);

  await waitFor(() => {
    expect((box as HTMLInputElement).value).toBe("@alo ");
  });
});

/** Putting an agent in a room was API-only until now: the one thing that makes
 *  this chat different could not be switched on from the interface. */
test("an agent can be put into a room from the UI", async () => {
  answers = [
    { match: "/chat/reactions", body: { emoji: ["👍"] } },
    { match: "/chat/agents", body: { agents: [AGENT] } },
    { match: "/chat/channels/room-1/agents", body: { agents: [] } },
    { match: "/messages", body: { messages: [message({ body: "hi" })] } },
    {
      match: "/chat/channels/room-1",
      body: { ...ROOM, members: [], myRole: "owner" },
    },
    { match: "/chat/channels", body: { channels: [ROOM] } },
  ];
  render(<ChatModule />);

  fireEvent.click(await screen.findByTitle(strings.chatWhoIsHere));
  // It is offered because it is not in the room yet.
  const add = await screen.findByTitle(strings.chatAgentAdd("alo"));
  fireEvent.click(add);

  await waitFor(() => {
    const call = calls.find(
      (c) =>
        c.url.includes("/chat/channels/room-1/agents") && c.method === "POST",
    );
    expect(call?.body).toEqual({ agent: "agent-alo" });
  });
});

test("only my own standing words offer Edit and Withdraw", async () => {
  withMessages([
    message({ id: "m-mine", seq: 1, author: ME, body: "my words" }),
    message({
      id: "m-theirs",
      seq: 2,
      author: THEM,
      authorEmail: "ben@alo.test",
      body: "their words",
    }),
    message({
      id: "m-agent",
      seq: 3,
      author: "agent-alo",
      authorKind: "agent",
      authorEmail: "alo",
      onBehalfOf: ME,
      body: "what the agent said",
    }),
    message({
      id: "m-gone",
      seq: 4,
      author: ME,
      body: "",
      deletedAt: "2026-08-09T10:00:00Z",
    }),
  ]);
  render(<ChatModule />);
  await screen.findByText("my words");

  // One Edit and one Withdraw in the whole feed: mine, still standing.
  // Not someone else's (the server refuses), not the agent's (its message is
  // a record of what it said, not a draft), not an already-withdrawn one.
  expect(screen.getAllByText(strings.chatEditAction)).toHaveLength(1);
  expect(screen.getAllByText(strings.chatWithdrawAction)).toHaveLength(1);
});

test("editing sends the new words, and an unchanged edit sends nothing", async () => {
  withMessages([
    message({ id: "m-mine", seq: 1, author: ME, body: "teh plan" }),
  ]);
  render(<ChatModule />);
  await screen.findByText("teh plan");

  fireEvent.click(screen.getByText(strings.chatEditAction));
  const box = screen.getByLabelText(strings.chatEditLabel);

  // Saving it untouched must not stamp "edited" on words nobody changed.
  fireEvent.submit(box);
  await waitFor(() => {
    expect(calls.some((c) => c.method === "PATCH")).toBe(false);
  });

  fireEvent.click(screen.getByText(strings.chatEditAction));
  const again = screen.getByLabelText(strings.chatEditLabel);
  fireEvent.change(again, { target: { value: "the plan" } });
  fireEvent.submit(again);

  await waitFor(() => {
    const patch = calls.find((c) => c.method === "PATCH");
    expect(patch?.url).toContain("/chat/messages/m-mine");
    expect(patch?.body).toEqual({ body: "the plan" });
  });
});

/** A full page means there is probably more behind it. A short page means we
 *  have reached the beginning — and the control must go away, or it invites a
 *  request that returns nothing forever. */
function page(n: number, from: number) {
  return Array.from({ length: n }, (_, i) =>
    message({ id: `m-${from - i}`, seq: from - i, body: `line ${from - i}` }),
  );
}

test("a full page offers to show earlier messages; a short one does not", async () => {
  // 50 back = exactly one page, so something is probably behind it.
  withMessages(page(50, 50));
  render(<ChatModule />);
  await screen.findByText("line 50");
  expect(screen.getByText(strings.chatOlder)).toBeTruthy();

  cleanup();
  calls.length = 0;
  // A short page is the beginning of the room.
  withMessages(page(3, 3));
  render(<ChatModule />);
  await screen.findByText("line 3");
  expect(screen.queryByText(strings.chatOlder)).toBeNull();
});

test("showing earlier messages asks for what is behind the oldest held", async () => {
  withMessages(page(50, 50));
  render(<ChatModule />);
  await screen.findByText("line 50");

  fireEvent.click(screen.getByText(strings.chatOlder));

  await waitFor(() => {
    // The cursor is the oldest seq on screen — seq 1 — not a page number and
    // not an offset, so a message arriving meanwhile cannot shift the window.
    expect(calls.some((c) => c.url.includes("before=1"))).toBe(true);
  });
});

test("browsing lists open channels and joining opens the one chosen", async () => {
  answers = [
    { match: "/chat/reactions", body: { emoji: ["👍"] } },
    {
      match: "/chat/channels/joinable",
      body: { channels: [{ ...ROOM, id: "room-open", name: "open-room" }] },
    },
    { match: "/messages", body: { messages: [] } },
    { match: "/chat/channels", body: { channels: [ROOM] } },
  ];
  render(<ChatModule />);

  fireEvent.click(await screen.findByTitle(strings.chatBrowse));
  fireEvent.click(await screen.findByText("open-room"));

  await waitFor(() => {
    expect(
      calls.some(
        (c) =>
          c.url.includes("/chat/channels/room-open/join") &&
          c.method === "POST",
      ),
    ).toBe(true);
  });
});

test("a colleague is searched for, never listed, and one letter asks nothing", async () => {
  answers = [
    { match: "/chat/reactions", body: { emoji: ["👍"] } },
    {
      match: "/chat/people",
      body: { people: [{ user: THEM, email: "ben@alo.test" }] },
    },
    { match: "/messages", body: { messages: [] } },
    { match: "/chat/channels", body: { channels: [ROOM] } },
  ];
  render(<ChatModule />);

  fireEvent.click(await screen.findByTitle(strings.chatNewDm));
  const box = screen.getByLabelText(strings.chatFindPerson);

  // One letter must not reach the server: it would ask a question whose only
  // answer is nothing, and a client that asks anyway invites someone to widen
  // the rule later "because it's already being called".
  fireEvent.change(box, { target: { value: "b" } });
  await waitFor(() => {
    expect(calls.some((c) => c.url.includes("/chat/people"))).toBe(false);
  });
  expect(screen.getByText(strings.chatFindPersonHint)).toBeTruthy();

  fireEvent.change(box, { target: { value: "ben" } });
  await screen.findByText("ben@alo.test");

  fireEvent.click(screen.getByText("ben@alo.test"));
  await waitFor(() => {
    const made = calls.find(
      (c) => c.url.endsWith("/chat/channels") && c.method === "POST",
    );
    expect(made?.body).toEqual({ kind: "dm", with: THEM });
  });
});
