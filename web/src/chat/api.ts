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
  Channel,
  ChannelDetail,
  ChannelSummary,
  Message,
  NewChannel,
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

  /** A page of history, newest first. Pass the oldest `seq` held as `before`
   *  to walk further back. */
  async messages(
    id: string,
    before?: number,
    limit?: number,
  ): Promise<Message[]> {
    const query = new URLSearchParams();
    if (before !== undefined) query.set("before", String(before));
    if (limit !== undefined) query.set("limit", String(limit));
    const suffix = query.toString() === "" ? "" : `?${query.toString()}`;
    const body = await this.#read<{ messages: Message[] }>(
      `/chat/channels/${encodeURIComponent(id)}/messages${suffix}`,
    );
    return body.messages;
  }

  /** Say something. `threadRootSeq` makes it a reply. */
  async post(
    id: string,
    body: string,
    threadRootSeq?: number,
  ): Promise<Message> {
    return this.#write<Message>(
      "POST",
      `/chat/channels/${encodeURIComponent(id)}/messages`,
      {
        body,
        ...(threadRootSeq === undefined ? {} : { threadRootSeq }),
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
