// Talking to alo about meetings — never to the media engine directly.
//
// The browser asks alo to start or join; alo decides, records it, and hands
// back a short-lived token and the engine's URL. The engine's room name never
// crosses this boundary on its own: it arrives inside a signed token or not at
// all, so a room name cannot be passed around as a way in.
import { useMemo } from "react";

import { useAuth } from "../auth";
import { API_BASE } from "../platform/runtime";

/** A meeting as the workspace knows it. */
export interface Meeting {
  id: string;
  title: string;
  createdBy: string;
  channel: string | null;
  event: string | null;
  createdAt: string;
  startedAt: string | null;
  endedAt: string | null;
  live: boolean;
}

/** What a browser needs to actually join: where the engine is, and proof. */
export interface JoinGrant {
  meeting: Meeting;
  url: string;
  token: string;
}

export interface MeetingMessage {
  id: string;
  sender: string;
  recipient: string | null;
  body: string;
  createdAt: string;
  attachments: MeetingAttachment[];
  reactions: MeetingReaction[];
}

export interface MeetingAttachment { id: string; name: string; contentType: string; size: number; url: string; }
export interface MeetingReaction { emoji: string; actor: string; }
export interface MeetingTranscriptSegment { id: string; speaker: string; text: string; final: boolean; createdAt: string; }

/** Raised when meetings are not configured on this deployment. */
export class MeetUnavailable extends Error {}

/** An API failure whose status callers may need to act on. */
export class MeetApiError extends Error {
  constructor(readonly status: number, action: string) {
    super(`${action} ${status}`);
  }
}

export class MeetApi {
  /** The workspace's own fetch: it carries the bearer token and refreshes it,
   *  so this module never handles a credential. */
  readonly #fetch: (url: string, init?: RequestInit) => Promise<Response>;

  constructor(
    authorizedFetch: (url: string, init?: RequestInit) => Promise<Response>,
  ) {
    this.#fetch = authorizedFetch;
  }

  async #send(path: string, init?: RequestInit): Promise<Response> {
    return this.#fetch(`${API_BASE}/api${path}`, {
      ...init,
      headers: {
        ...(init?.body === undefined
          ? {}
          : { "content-type": "application/json" }),
        ...init?.headers,
      },
    });
  }

  /** Start a meeting, optionally attached to a chat room or a calendar event. */
  async start(within?: { channel?: string; event?: string; title?: string }) {
    const res = await this.#send("/meet", {
      method: "POST",
      body: JSON.stringify({
        title: within?.title ?? "",
        channel: within?.channel ?? null,
        event: within?.event ?? null,
      }),
    });
    if (!res.ok) throw new Error(`start ${res.status}`);
    return (await res.json()) as Meeting;
  }

  /** Everything live this person can walk into. */
  async mine(): Promise<Meeting[]> {
    const res = await this.#send("/meet");
    if (!res.ok) return [];
    return ((await res.json()) as { meetings: Meeting[] }).meetings;
  }

  /** Recently ended meetings this person was allowed to attend. */
  async history(): Promise<Meeting[]> {
    const res = await this.#send("/meet/history");
    if (!res.ok) return [];
    return ((await res.json()) as { meetings: Meeting[] }).meetings;
  }

  async messages(id: string): Promise<MeetingMessage[]> {
    const res = await this.#send(`/meet/${encodeURIComponent(id)}/messages`);
    if (!res.ok) return [];
    return ((await res.json()) as { messages: MeetingMessage[] }).messages;
  }

  async postMessage(id: string, body: string, recipient: string | null): Promise<MeetingMessage> {
    const res = await this.#send(`/meet/${encodeURIComponent(id)}/messages`, {
      method: "POST",
      body: JSON.stringify({ body, recipient }),
    });
    if (!res.ok) throw new MeetApiError(res.status, "post meeting message");
    return (await res.json()) as MeetingMessage;
  }

  async react(meeting: string, message: string, emoji: string): Promise<void> {
    const res = await this.#send(`/meet/${encodeURIComponent(meeting)}/messages/${encodeURIComponent(message)}/reactions`, {
      method: "POST",
      body: JSON.stringify({ emoji }),
    });
    if (!res.ok) throw new MeetApiError(res.status, "react to meeting message");
  }

  async transcript(meeting: string): Promise<MeetingTranscriptSegment[]> {
    const res = await this.#send(`/meet/${encodeURIComponent(meeting)}/transcript`);
    if (!res.ok) return [];
    return ((await res.json()) as { segments: MeetingTranscriptSegment[] }).segments;
  }

  async putTranscriptSegment(meeting: string, segment: { id: string; text: string; final: boolean }): Promise<MeetingTranscriptSegment> {
    const res = await this.#send(`/meet/${encodeURIComponent(meeting)}/transcript`, {
      method: "POST",
      body: JSON.stringify(segment),
    });
    if (!res.ok) throw new MeetApiError(res.status, "save meeting transcript");
    return (await res.json()) as MeetingTranscriptSegment;
  }

  async uploadAttachment(meeting: string, message: string, file: File): Promise<MeetingAttachment> {
    const res = await this.#send(`/meet/${encodeURIComponent(meeting)}/messages/${encodeURIComponent(message)}/attachments?name=${encodeURIComponent(file.name)}`, { method: "POST", body: file, headers: { "content-type": file.type } });
    if (!res.ok) throw new MeetApiError(res.status, "upload meeting attachment");
    return (await res.json()) as MeetingAttachment;
  }

  async downloadAttachment(attachment: MeetingAttachment): Promise<Blob> {
    const res = await this.#send(attachment.url.replace(/^\/api/, ""));
    if (!res.ok) throw new MeetApiError(res.status, "download meeting attachment");
    return res.blob();
  }

  /** Meetings still running in a room. */
  async liveIn(channel: string): Promise<Meeting[]> {
    const res = await this.#send(
      `/meet/channels/${encodeURIComponent(channel)}`,
    );
    if (!res.ok) return [];
    return ((await res.json()) as { meetings: Meeting[] }).meetings;
  }

  /**
   * Take a place in a meeting.
   *
   * A 503 means this deployment has no media engine — the meeting is real and
   * attendance was recorded, there is simply nowhere to hold it. That is worth
   * saying differently from a failure.
   */
  /** The meeting on a calendar event, or `null` when the invitation has none.
   *  An absent meeting is an ordinary state, not a failure. */
  async forEvent(event: string): Promise<Meeting | null> {
    const res = await this.#send(`/meet/events/${encodeURIComponent(event)}`);
    if (!res.ok) return null;
    return ((await res.json()) as { meeting: Meeting | null }).meeting;
  }

  async join(id: string): Promise<JoinGrant> {
    const res = await this.#send(`/meet/${encodeURIComponent(id)}/join`, {
      method: "POST",
      body: "{}",
    });
    if (res.status === 503) {
      throw new MeetUnavailable("no media engine is configured");
    }
    if (!res.ok) throw new MeetApiError(res.status, "join");
    return (await res.json()) as JoinGrant;
  }

  /** Declare a meeting over for everyone. */
  async end(id: string): Promise<void> {
    await this.#send(`/meet/${encodeURIComponent(id)}/end`, {
      method: "POST",
      body: "{}",
    });
  }
}

/** The Meet client for the signed-in person. */
export function useMeetApi(): MeetApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new MeetApi(authorizedFetch), [authorizedFetch]);
}
