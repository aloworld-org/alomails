// The client for the authenticated `/chat/*` surface (alo Chat, ADR 0038).
//
// Its own small client rather than more methods on `JmapClient`, for the same
// reason sites' and billing's are: a plain REST surface with none of JMAP's
// envelope, changing for different reasons than mail does. It uses the same
// authenticated fetch, so there is one session, not two.
//
// It holds NO rules. Who may see a room, who may post in it, what a legal
// message is, how far a read cursor may move — all of that is the store's, and
// a second weaker copy here is how two doors end up disagreeing. This layer
// sends what was typed and surfaces what came back (UX law 8: the server's own
// sentence is what the user reads).
import { useMemo } from "react";

import { useAuth } from "../auth";
import { API_BASE } from "../platform/runtime";
import type {
  Agent,
  Channel,
  ChannelDetail,
  ChannelSummary,
  FeedMessage,
  Message,
  NewChannel,
  Proposal,
  Reaction,
} from "./types";

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/**
 * A failed chat request. `detail` is the server's own sentence when it sent
 * one — those name the rule that was broken and never another tenant's data,
 * so they are safe to show. `status` lets a caller tell "that breaks a rule"
 * (422) from "that room is not yours to see" (404) without reading prose.
 */
export class ChatError extends Error {
  readonly status: number;
  readonly detail: string | null;

  constructor(status: number, detail: string | null) {
    super(detail ?? `chat request failed (${status})`);
    this.name = "ChatError";
    this.status = status;
    this.detail = detail;
  }
}

/** What to show about a failed request: the server's own sentence when it sent
 *  one, and `fallback` otherwise (a dropped connection, or a failure whose
 *  reason is not the user's business). */
export function chatMessage(error: unknown, fallback: string): string {
  return error instanceof ChatError && error.detail !== null
    ? error.detail
    : fallback;
}

/** The `/chat/*` client. One method per route, added with its screen. */
export class ChatApi {
  readonly #fetch: AuthorizedFetch;

  constructor(authorizedFetch: AuthorizedFetch) {
    this.#fetch = authorizedFetch;
  }

  /** My rooms, liveliest first, each with its unread count — the sidebar. */
  async channels(): Promise<ChannelSummary[]> {
    const body = await this.#read<{ channels: ChannelSummary[] }>(
      "/chat/channels",
    );
    return body.channels;
  }

  /** The live public channels I have not joined — "browse channels". */
  async joinable(): Promise<Channel[]> {
    const body = await this.#read<{ channels: Channel[] }>(
      "/chat/channels/joinable",
    );
    return body.channels;
  }

  /** One room with its people and my standing in it. */
  async channel(id: string): Promise<ChannelDetail> {
    return this.#read<ChannelDetail>(
      `/chat/channels/${encodeURIComponent(id)}`,
    );
  }

  /** Create a named room, or open the DM with someone (opening the same DM
   *  twice returns the same room). */
  async createChannel(draft: NewChannel): Promise<Channel> {
    return this.#write<Channel>("POST", "/chat/channels", draft);
  }

  /** Join a live public channel. */
  async join(id: string): Promise<Channel> {
    return this.#write<Channel>(
      "POST",
      `/chat/channels/${encodeURIComponent(id)}/join`,
      {},
    );
  }

  /** The reactions this deployment offers, in the order to show them.
   *
   *  Asked for rather than hardcoded: the set lives in the store and will
   *  grow, and a client with its own copy would offer emoji the server then
   *  refuses. */
  async reactionPalette(): Promise<string[]> {
    const body = await this.#read<{ emoji: string[] }>("/chat/reactions");
    return body.emoji;
  }

  /** Leave a reaction, or take it back if it is already mine. Returns the
   *  message's whole tally, so chips are redrawn from one answer rather than
   *  patched locally and hoped to match. */
  async react(messageId: string, emoji: string): Promise<Reaction[]> {
    const body = await this.#write<{ reactions: Reaction[] }>(
      "POST",
      `/chat/messages/${encodeURIComponent(messageId)}/reactions`,
      { emoji },
    );
    return body.reactions;
  }

  /** The agents in a room, for the composer's `@` list. */
  async channelAgents(id: string): Promise<Agent[]> {
    const body = await this.#read<{ agents: Agent[] }>(
      `/chat/channels/${encodeURIComponent(id)}/agents`,
    );
    return body.agents;
  }

  /** Every agent the tenant has, for choosing one to add to a room. */
  async agents(): Promise<Agent[]> {
    const body = await this.#read<{ agents: Agent[] }>("/chat/agents");
    return body.agents;
  }

  /** Put an agent in a room. */
  async addAgent(id: string, agent: string): Promise<Agent[]> {
    const body = await this.#write<{ agents: Agent[] }>(
      "POST",
      `/chat/channels/${encodeURIComponent(id)}/agents`,
      { agent },
    );
    return body.agents;
  }

  /** Decide a proposed action. Approving runs it in the same request, so the
   *  answer already reflects what happened; 403 if the caller is not the
   *  person who asked. */
  async decideProposal(id: string, approve: boolean): Promise<Proposal> {
    return this.#write<Proposal>(
      "POST",
      `/chat/proposals/${encodeURIComponent(id)}`,
      { approve },
    );
  }

  /** Find messages, newest first. Only what the caller may already read: the
   *  server applies the room rule in the query, not as an afterthought. */
  async search(query: string, channel?: string): Promise<Message[]> {
    const params = new URLSearchParams({ q: query });
    if (channel !== undefined) params.set("channel", channel);
    const body = await this.#read<{ messages: Message[] }>(
      `/chat/search?${params.toString()}`,
    );
    return body.messages;
  }

  /** Take an agent out of a room. Its past messages stay — a room's history
   *  does not change because somebody left it. */
  async removeAgent(id: string, agent: string): Promise<void> {
    await this.#send(
      `/chat/channels/${encodeURIComponent(id)}/agents/${encodeURIComponent(agent)}`,
      { method: "DELETE" },
    ).then(ChatApi.#rejectFailed);
  }

  /** The replies under one message, oldest first — a thread reads forwards. */
  async thread(id: string, rootSeq: number): Promise<Message[]> {
    const body = await this.#read<{ messages: Message[] }>(
      `/chat/channels/${encodeURIComponent(id)}/threads/${rootSeq}`,
    );
    return body.messages;
  }

  /** A page of the main feed, newest first. Pass the oldest `seq` held as
   *  `before` to walk further back. Replies are not here — they live in their
   *  thread, and each message says how many it has. */
  async messages(
    id: string,
    before?: number,
    limit?: number,
  ): Promise<FeedMessage[]> {
    const query = new URLSearchParams();
    if (before !== undefined) query.set("before", String(before));
    if (limit !== undefined) query.set("limit", String(limit));
    const suffix = query.toString() === "" ? "" : `?${query.toString()}`;
    const body = await this.#read<{ messages: FeedMessage[] }>(
      `/chat/channels/${encodeURIComponent(id)}/messages${suffix}`,
    );
    return body.messages;
  }

  /** Say something. `threadRootSeq` makes it a reply; `attachments` are Drive
   *  node ids, shared as pointers. The server refuses the whole post if any
   *  file is not the caller's to share, so nothing is said on a rejection. */
  async post(
    id: string,
    body: string,
    threadRootSeq?: number,
    attachments?: string[],
  ): Promise<Message> {
    return this.#write<Message>(
      "POST",
      `/chat/channels/${encodeURIComponent(id)}/messages`,
      {
        body,
        ...(threadRootSeq === undefined ? {} : { threadRootSeq }),
        ...(attachments === undefined || attachments.length === 0
          ? {}
          : { attachments }),
      },
    );
  }

  /** Move my read cursor. It never moves backwards, and never past the end. */
  async markRead(id: string, seq: number): Promise<void> {
    await this.#send(`/chat/channels/${encodeURIComponent(id)}/read`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ seq }),
    }).then(ChatApi.#rejectFailed);
  }

  async #read<T>(path: string): Promise<T> {
    return this.#json<T>(await this.#send(path, {}));
  }

  async #write<T>(method: string, path: string, body: unknown): Promise<T> {
    return this.#json<T>(
      await this.#send(path, {
        method,
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      }),
    );
  }

  async #send(path: string, init: RequestInit): Promise<Response> {
    try {
      return await this.#fetch(`${API_BASE}${path}`, init);
    } catch {
      // A dropped connection is not a status code; give it one the UI can
      // treat like any other failure rather than an unhandled rejection.
      throw new ChatError(0, null);
    }
  }

  async #json<T>(res: Response): Promise<T> {
    await ChatApi.#rejectFailed(res);
    return (await res.json()) as T;
  }

  /** Turns a non-2xx answer into the shaped [`ChatError`]. */
  static async #rejectFailed(res: Response): Promise<void> {
    if (!res.ok) {
      const problem = (await res.json().catch(() => ({}))) as {
        detail?: unknown;
      };
      const detail = typeof problem.detail === "string" ? problem.detail : null;
      throw new ChatError(res.status, detail);
    }
  }
}

/** The chat client bound to the current session. Memoized per auth context, so
 *  a re-render never re-creates it and effects keyed on it do not loop. */
export function useChatApi(): ChatApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new ChatApi(authorizedFetch), [authorizedFetch]);
}
