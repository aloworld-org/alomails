// The shapes the `/chat/*` surface speaks (alo Chat, ADR 0038). Mirrors
// `docs/design/chat.md`; the server is the authority for every rule, so
// nothing here validates anything — these are the wire's own words.

/** A named room, or the private pair a DM is. */
export type ChannelKind = "channel" | "dm";

/** Who may see a named channel. A DM is always private. */
export type ChannelVisibility = "public" | "private";

/** What a member may do to the room itself. */
export type MemberRole = "owner" | "member";

/** One room. */
export interface Channel {
  id: string;
  kind: ChannelKind;
  /** The `#name` of a named room; `null` for a DM. */
  name: string | null;
  topic: string | null;
  visibility: ChannelVisibility;
  createdBy: string;
  createdAt: string;
  /** Set once archived: out of the lists, history still readable. */
  archivedAt: string | null;
}

/** A room as the sidebar needs it: with what is unread and when it last
 *  had life. */
export interface ChannelSummary extends Channel {
  /** Messages after my read cursor — my own never count. */
  unread: number;
  lastReadSeq: number;
  lastSeq: number | null;
  lastAt: string | null;
}

/** One person in a room. */
export interface Member {
  user: string;
  role: MemberRole;
  joinedAt: string;
  lastReadSeq: number;
  muted: boolean;
}

/** A room with its people and my own standing in it (`myRole` is `null` when
 *  I am only a reader of a public room). */
export interface ChannelDetail extends Channel {
  members: Member[];
  myRole: MemberRole | null;
}

/** One thing said in a room. */
export interface Message {
  id: string;
  channel: string;
  /** Position in the room: the ordering key and the page cursor. */
  seq: number;
  author: string;
  /** Empty when withdrawn — see `deletedAt`. */
  body: string;
  kind: "text" | "system";
  /** The seq this replies to; `null` in the main feed. */
  threadRootSeq: number | null;
  createdAt: string;
  editedAt: string | null;
  /** Set when withdrawn: the row survives so the numbering never gains a
   *  hole, but the words are gone. */
  deletedAt: string | null;
}

/** What to create: a named room, or a DM with one person. */
export interface NewChannel {
  kind?: ChannelKind;
  name?: string;
  topic?: string;
  visibility?: ChannelVisibility;
  /** The other person, when opening a DM. */
  with?: string;
}
