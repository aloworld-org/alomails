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
  /** The other member's address for a DM; `null` for named rooms or when the
   * directory can no longer resolve them. */
  counterpart: string | null;
  /** Messages after my read cursor — my own never count. */
  unread: number;
  /** How many of those name me. Separate from `unread` on purpose: forty new
   *  lines and one addressed to you are different calls on your attention. */
  mentions: number;
  lastReadSeq: number;
  lastSeq: number | null;
  lastAt: string | null;
  /** The newest real message, used as the compact sidebar preview. */
  lastBody: string | null;
}

/** An agent turn running right now. */
export interface Turn {
  id: string;
  agent: string;
  handle: string;
  /** Whether this reader is the one who asked, and so may stop it. */
  mine: boolean;
}

/** Somebody in the tenant, found by searching for their address. */
export interface Person {
  user: string;
  email: string;
}

/** One person in a room. */
export interface Member {
  user: string;
  /** Their address, or `null` if the id no longer resolves (they left the
   *  tenant). The id stays the identity; this is only what to show. */
  email: string | null;
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

/** One emoji on one message, with how many people chose it. */
export interface Reaction {
  emoji: string;
  count: number;
  /** Whether I am one of them — what makes the chip a toggle, not a counter. */
  mine: boolean;
}

/** A file shared in a conversation: a pointer into Drive, never a copy.
 *  Name and size are what Drive says *now* — a renamed file shows its new
 *  name, and a file the reader may no longer open is absent entirely. */
export interface Attachment {
  /** The Drive node. Opened with `GET /drive/nodes/{node}/download`. */
  node: string;
  name: string;
  size: number;
  contentType: string | null;
  /** Drive has it in the trash. Still shown, but shown as trashed. */
  trashed: boolean;
  sharedAt: string;
}

/** An agent that can be named in a conversation. It has an identity to post
 *  under and no authority of its own: every turn runs with the access of the
 *  person who asked it. */
export interface Agent {
  id: string;
  /** Typed after `@`. */
  handle: string;
  name: string;
  description: string | null;
  /** Retired: keeps its past messages, takes no new turns. */
  disabled: boolean;
  /** Times it has answered, counted only across rooms you can see — so two
   *  people may legitimately see different numbers for the same agent. */
  answers: number;
  /** Actions it proposed that someone approved, and which therefore ran. */
  actions: number;
  /** When it last said anything here. */
  lastAt: string | null;
}

/** An action an agent proposed, waiting for a tap. */
export interface Proposal {
  id: string;
  message: string;
  /** The person whose words caused it — the only one who may decide. */
  askedBy: string;
  tool: string;
  args: Record<string, unknown>;
  state: "pending" | "approved" | "discarded" | "expired";
  decidedBy: string | null;
}

/** One thing said in a room. */
export interface Message {
  id: string;
  channel: string;
  /** Position in the room: the ordering key and the page cursor. */
  seq: number;
  author: string;
  /** Whether a person or an agent said it. Never inferred from the id: the
   *  two do not share a namespace and must not be told apart by shape. */
  authorKind: "user" | "agent";
  /** A person's address, or an agent's name — whatever to call the author.
   *  `null` when the id no longer resolves. */
  authorEmail: string | null;
  /** On an agent's message, the person whose reach produced it. */
  onBehalfOf: string | null;
  /** Empty when withdrawn — see `deletedAt`. */
  body: string;
  kind: "text" | "system";
  /** The seq this replies to; `null` in the main feed. */
  threadRootSeq: number | null;
  /** Chips under the message; empty when nobody has reacted. */
  reactions: Reaction[];
  /** The user ids this message names. Resolved by the server at post time
   *  against the room's members, so a handle matching nobody there is absent
   *  rather than guessed at here. */
  mentions: string[];
  /** Files shared with it; empty when none. */
  attachments: Attachment[];
  /** An action proposed on this message, if there is one. */
  proposal: Proposal | null;
  createdAt: string;
  editedAt: string | null;
  /** Set when withdrawn: the row survives so the numbering never gains a
   *  hole, but the words are gone. */
  deletedAt: string | null;
  /** Number of other room members whose read cursor includes this message. */
  readBy: number;
}

/**
 * A message as the main feed shows it. The feed carries top-level messages
 * only — a reply lives in its thread and is announced here by the count, so
 * a conversation is never read twice over.
 */
export interface FeedMessage extends Message {
  /** Surviving replies under this message; withdrawn ones are not counted. */
  replyCount: number;
  /** When the thread last moved. */
  lastReplyAt: string | null;
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
