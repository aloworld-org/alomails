// alo Chat (ADR 0038) — rooms on the left, the conversation on the right.
//
// Domain references (UX law 2): Slack and WhatsApp for reflexes — a room list
// with unread counts, a scrolling feed, a composer that sends on Enter — and
// Sila for the calm of it. Everything the core task needs is on the surface:
// no menu is required to start a room, open one, or say something (prime law).
//
// Live by the push stream the workspace already keeps open (ADR 0038): a chat
// signal refetches the sidebar, and the open room's newest messages. Sending is
// optimistic — the line appears at once and is reconciled by the refetch — so a
// click is never answered by silence (law 6).
import type { ReactNode } from "react";
import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import {
  Archive,
  ChevronLeft,
  Hash,
  Quote,
  List,
  Sigma,
  SquareCode,
  Code,
  Italic,
  Bold,
  Loader2,
  Lock,
  MessageSquarePlus,
  MessagesSquare,
  MoreHorizontal,
  Paperclip,
  Pencil,
  Plus,
  Reply,
  Search,
  Send,
  Sparkles,
  Smile,
  SmilePlus,
  Trash2,
  Users,
  Video,
  X,
} from "lucide-react";

import { strings } from "../i18n";
import { useAuth } from "../auth";
import { FilePicker, fileSize, saveBlob } from "../drive";
import { AgentActionCard } from "../shell/AgentActionCard";
import { RoomPeople } from "./RoomPeople";
import { useJmapClient } from "../jmap";
import { Avatar, Button, useDialogs, useDismiss, useIsMobile } from "../ds";
import { ChatError, chatMessage, useChatApi } from "./api";
import type { DriveNodeDto } from "../jmap/types";
import type {
  Attachment,
  Channel,
  ChannelSummary,
  FeedMessage,
  Person,
  Turn,
  Message,
  Proposal,
} from "./types";
import { useMeetApi } from "../meet";
import type { Meeting } from "../meet";
import { MeetRoom } from "../meet";
import { EMOJI, searchEmoji } from "./emoji";
import { renderBody } from "./richText";
import styles from "./ChatModule.module.css";

/** The ceiling the server enforces (`ATTACHMENTS_MAX` in the store). Kept in
 *  step by hand: exceeding it is refused server-side either way, so the worst
 *  a drifted copy does is offer a choice that is then declined. */
const ATTACHMENTS_MAX = 10;

/** What one page of history holds — the server's own default
 *  (`MESSAGE_PAGE_DEFAULT`). A full page means there is probably more behind
 *  it; a short one means we have reached the beginning. */
const PAGE = 50;

/** A room's label: its `#name`, or the standing of a DM. */
function channelLabel(channel: ChannelSummary): string {
  return channel.name ?? strings.chatDirectMessage;
}

/**
 * How a person reads in the feed: the local part of their address, which is
 * what colleagues actually call each other, falling back to the opaque id
 * when the directory no longer knows them (they have left the tenant). This
 * schema has no display-name column yet; when it does, it belongs here.
 */
function personName(email: string | null, id: string): string {
  if (email === null) return id;
  const at = email.indexOf("@");
  return at > 0 ? email.slice(0, at) : email;
}

/** Minutes within which consecutive messages from one person are one run.
 *  Slack and Teams both use about this; longer and a reply an hour later
 *  hides under a stale name, shorter and a quick exchange fragments. */
const GROUP_MINUTES = 5;

/** Whether `message` continues the run started by `before`. */
function continues(message: Message, before: Message | undefined): boolean {
  if (before === undefined) return false;
  if (before.author !== message.author) return false;
  if (before.authorKind !== message.authorKind) return false;
  // A proposal or an attachment ends a run: those carry their own block and
  // reading them without a name above is disorienting.
  if (before.proposal !== null || before.attachments.length > 0) return false;
  const gap =
    new Date(message.createdAt).getTime() -
    new Date(before.createdAt).getTime();
  return gap >= 0 && gap < GROUP_MINUTES * 60_000;
}

/** The day a message belongs to, as a divider reads it. */
function dayOf(iso: string): string {
  const d = new Date(iso);
  const today = new Date();
  const same = (a: Date, b: Date) => a.toDateString() === b.toDateString();
  if (same(d, today)) return strings.chatToday;
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (same(d, yesterday)) return strings.chatYesterday;
  return d.toLocaleDateString(undefined, {
    weekday: "long",
    day: "numeric",
    month: "long",
    // The year only when it is not this one — "12 March 2024" matters, "12
    // March 2026" is noise in March 2026.
    ...(d.getFullYear() === today.getFullYear() ? {} : { year: "numeric" }),
  });
}

/** Hour and minute only, for the gutter beside a grouped line. The full
 *  locale time ("10:05 AM") is wider than the gutter and was rendering
 *  clipped against the words. */
function shortTime(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

/** Local time of day, for the line beside an author. */
function timeOf(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime())
    ? ""
    : at.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

/**
 * A message body with its `@handles` marked.
 *
 * The marking is typographic only — who was actually named is the server's
 * answer (`message.mentions`), resolved against the room's members at post
 * time. Re-deciding that here would be a second, weaker copy of a rule that
 * already has an owner; this only makes what was typed visible.
 */
function withHandlesMarked(body: string): ReactNode[] {
  const parts: ReactNode[] = [];
  const pattern = /(^|[\s([{"'])(@[A-Za-z0-9._%+-]+(?:@[A-Za-z0-9.-]+)?)/g;
  let at = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(body)) !== null) {
    const start = match.index + match[1]!.length;
    if (start > at) parts.push(body.slice(at, start));
    parts.push(
      <span key={`${start}`} className={styles.handle}>
        {match[2]}
      </span>,
    );
    at = start + match[2]!.length;
  }
  if (at < body.length) parts.push(body.slice(at));
  return parts;
}

/** One thing that can be named after an `@`: a person or an agent. */
interface Nameable {
  /** What gets typed — a person's local part, or an agent's handle. */
  handle: string;
  /** What is shown: an address, or an agent's name. */
  label: string;
  agent: boolean;
}

/**
 * The `@token` being typed immediately before the caret, if any.
 *
 * Mirrors the server's own parser (`parse_handles`): an `@` only opens a
 * mention at a word boundary, so an address typed inline is not one, and a
 * space ends it. The two must agree, or the list would offer a completion the
 * server then declines to resolve.
 */
function mentionAt(
  value: string,
  caret: number,
): { start: number; token: string } | null {
  const upto = value.slice(0, caret);
  const at = upto.lastIndexOf("@");
  if (at < 0) return null;
  const before = at === 0 ? " " : upto[at - 1]!;
  if (!/[\s([{"']/.test(before)) return null;
  const token = upto.slice(at + 1);
  if (/\s/.test(token)) return null;
  return { start: at, token: token.toLowerCase() };
}

/** Who the list offers for `token`, agents first: an agent is the thing a
 *  person is least likely to know is there. */
function candidatesFor(token: string, all: Nameable[]): Nameable[] {
  const matching = all.filter((n) => n.handle.startsWith(token));
  return [
    ...matching.filter((n) => n.agent),
    ...matching.filter((n) => !n.agent),
  ].slice(0, 6);
}

/**
 * Whether this reader may decide a proposal, and why not when they may not.
 *
 * Spread rather than passed as a possibly-undefined prop: with
 * `exactOptionalPropertyTypes`, "absent" and "present but undefined" are
 * different things, and only the first means "decidable".
 */
function standingOf(
  proposal: Proposal,
  me: string | null,
): { standing?: { decidable: false; reason: string } } {
  if (proposal.state !== "pending") {
    return {
      standing: {
        decidable: false,
        reason: strings.chatProposalSettled(proposal.state),
      },
    };
  }
  if (proposal.askedBy !== me) {
    return {
      standing: { decidable: false, reason: strings.chatProposalNotYours },
    };
  }
  return {};
}

/**
 * One line of conversation, used by both the feed and the thread panel — the
 * two must never drift into showing a message differently. `children` is what
 * hangs under it (the thread affordance in the feed, nothing in a thread).
 */
function MessageLine({
  message,
  palette,
  me,
  onReact,
  onOpenFile,
  onDecide,
  onEdit,
  onWithdraw,
  onReply,
  grouped = false,
  children,
}: {
  message: Message;
  /** What may be left here, asked of the server. Empty disables the picker. */
  palette: string[];
  /** The reader's own user id, for "this one is addressed to me". */
  me: string | null;
  onReact: (emoji: string) => void;
  /** Fetches and saves a shared file. The API takes a bearer token, so a
   *  plain link would arrive unauthenticated and 401. */
  onOpenFile: (file: Attachment) => void;
  /** Decide the proposal on this message. Only ever reachable for the asker;
   *  everyone else sees the card without buttons. */
  onDecide: (proposal: Proposal, approve: boolean) => void;
  /** Rewrite these words. Offered only on one's own, because the server
   *  refuses anyone else's and an offer that ends in 403 is a lie. */
  onEdit: (message: Message, body: string) => void;
  onWithdraw: (message: Message) => void;
  /** Open this message's thread. In the toolbar, not the flow: an affordance
   *  that is invisible until hover must not occupy space while invisible. */
  onReply?: ((message: Message) => void) | undefined;
  /** This message continues the previous author's run: no avatar, no name, no
   *  timestamp — just the words, aligned under the ones above. */
  grouped?: boolean;
  children?: ReactNode;
}) {
  const namesMe = me !== null && message.mentions.includes(me);
  const [picking, setPicking] = useState(false);
  const pickerRef = useRef<HTMLSpanElement | null>(null);
  const closePicker = useCallback(() => setPicking(false), []);
  useDismiss(picking, pickerRef, closePicker);
  const [editing, setEditing] = useState<string | null>(null);
  // Mine to change: my own words, still standing, and never an agent's — an
  // agent's message is a record of what it said, not a draft.
  const mine =
    me !== null &&
    message.authorKind === "user" &&
    message.author === me &&
    message.deletedAt === null;
  const isAgent = message.authorKind === "agent";
  // An agent's name is already a name; a person's address needs its local part.
  const who = isAgent
    ? (message.authorEmail ?? message.author)
    : personName(message.authorEmail, message.author);
  // Withdrawn words take no reactions — the server refuses, so the picker is
  // not offered on them either.
  const reactable = palette.length > 0 && message.deletedAt === null;
  return (
    <article
      className={[
        namesMe ? styles.messageForMe : styles.message,
        grouped ? styles.grouped : "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {editing === null && (
        <div className={styles.tools}>
          {reactable && (
            <span className={styles.pickerWrap} ref={pickerRef}>
              <button
                type="button"
                className={styles.tool}
                onClick={() => setPicking((open) => !open)}
                aria-label={strings.chatAddReaction}
                title={strings.chatAddReaction}
                aria-expanded={picking}
              >
                <SmilePlus size={16} />
              </button>
              {picking && (
                <span className={styles.picker} role="menu">
                  {palette.map((emoji) => (
                    <button
                      key={emoji}
                      type="button"
                      role="menuitem"
                      className={styles.pickerOption}
                      onClick={() => {
                        setPicking(false);
                        onReact(emoji);
                      }}
                    >
                      {emoji}
                    </button>
                  ))}
                </span>
              )}
            </span>
          )}
          {onReply !== undefined && message.deletedAt === null && (
            <button
              type="button"
              className={styles.tool}
              onClick={() => onReply(message)}
              aria-label={strings.chatReplyInThread}
              title={strings.chatReplyInThread}
            >
              <Reply size={15} />
            </button>
          )}
          {mine && (
            <>
              <button
                type="button"
                className={styles.tool}
                onClick={() => setEditing(message.body)}
                aria-label={strings.chatEditAction}
                title={strings.chatEditAction}
              >
                <Pencil size={15} />
              </button>
              <button
                type="button"
                className={styles.tool}
                onClick={() => onWithdraw(message)}
                aria-label={strings.chatWithdrawAction}
                title={strings.chatWithdrawAction}
              >
                <Trash2 size={15} />
              </button>
            </>
          )}
        </div>
      )}

      {grouped ? (
        // The time still exists for the reader who wants it, on approach only,
        // in the gutter the avatar would occupy.
        <span className={styles.gutterTime}>
          {shortTime(message.createdAt)}
        </span>
      ) : (
        <div className={styles.messageMeta}>
          {isAgent ? (
            <span className={styles.agentMark} aria-hidden="true">
              <Sparkles size={13} />
            </span>
          ) : (
            <Avatar
              name={who}
              email={message.authorEmail ?? undefined}
              size="sm"
            />
          )}
          <span
            className={isAgent ? styles.authorAgent : styles.author}
            // The full address on hover: the local part is what people say, the
            // address is what settles who it was.
            title={message.authorEmail ?? message.author}
          >
            {who}
          </span>
          {isAgent && (
            // An agent is not a colleague and must never be mistaken for one.
            <span className={styles.agentTag}>{strings.chatAgentTag}</span>
          )}
          <span className={styles.time}>{timeOf(message.createdAt)}</span>
          {message.editedAt !== null && message.deletedAt === null && (
            <span className={styles.edited}>{strings.chatEdited}</span>
          )}
        </div>
      )}
      {editing !== null ? (
        <form
          className={styles.editRow}
          onSubmit={(event) => {
            event.preventDefault();
            const next = editing.trim();
            setEditing(null);
            // An unchanged edit is not an edit; sending it would stamp
            // "edited" on words nobody touched.
            if (next !== "" && next !== message.body) onEdit(message, next);
          }}
        >
          <textarea
            className={styles.editInput}
            value={editing}
            rows={Math.min(
              12,
              editing.split(String.fromCharCode(10)).length + 1,
            )}
            onChange={(event) => setEditing(event.target.value)}
            aria-label={strings.chatEditLabel}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.preventDefault();
                setEditing(null);
              }
              // Same rule as the composer: Enter commits, Shift+Enter breaks
              // the line. A message written across lines must be editable
              // across lines.
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                const next = editing.trim();
                setEditing(null);
                if (next !== "" && next !== message.body) onEdit(message, next);
              }
            }}
            autoFocus
          />
          <Button type="submit" size="sm" variant="primary">
            {strings.chatEditSave}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setEditing(null)}>
            {strings.chatEditCancel}
          </Button>
        </form>
      ) : (
        <p
          className={
            message.deletedAt === null ? styles.body : styles.withdrawn
          }
        >
          {message.deletedAt === null
            ? renderBody(message.body, withHandlesMarked)
            : strings.chatWithdrawn}
        </p>
      )}

      {message.proposal !== null && (
        <div className={styles.proposal}>
          <AgentActionCard
            action={{
              tool: message.proposal.tool,
              args: message.proposal.args,
              say: message.body,
            }}
            running={false}
            onApprove={() => onDecide(message.proposal!, true)}
            onDiscard={() => onDecide(message.proposal!, false)}
            {...standingOf(message.proposal, me)}
          />
        </div>
      )}

      {message.attachments.length > 0 && (
        <ul className={styles.files}>
          {message.attachments.map((file) => (
            <li key={file.node}>
              {/* A plain link to Drive's own download route: the file is not
                  copied here, so opening it is Drive's business and Drive's
                  permission check. */}
              <button
                type="button"
                className={styles.file}
                onClick={() => void onOpenFile(file)}
                title={strings.chatOpenFile}
              >
                <Paperclip size={14} className={styles.fileIcon} />
                <span className={styles.fileName}>{file.name}</span>
                <span className={styles.fileSize}>{fileSize(file.size)}</span>
                {file.trashed && (
                  <span className={styles.fileTrashed}>
                    {strings.chatFileTrashed}
                  </span>
                )}
              </button>
            </li>
          ))}
        </ul>
      )}

      {message.reactions.length > 0 && (
        <div className={styles.chips}>
          {message.reactions.map((reaction) => (
            <button
              key={reaction.emoji}
              type="button"
              className={reaction.mine ? styles.chipMine : styles.chip}
              onClick={() => onReact(reaction.emoji)}
              // The chip is a toggle, and says which way it will go.
              aria-pressed={reaction.mine}
              disabled={!reactable}
            >
              <span aria-hidden="true">{reaction.emoji}</span>
              <span className={styles.chipCount}>{reaction.count}</span>
            </button>
          ))}
        </div>
      )}

      {children}
    </article>
  );
}

export function ChatModule() {
  const api = useChatApi();
  const client = useJmapClient();
  // The reader's own id, for marking the messages addressed to them.
  const { identity } = useAuth();
  const dialogs = useDialogs();
  const me = identity?.sub ?? null;
  const [channels, setChannels] = useState<ChannelSummary[] | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  const [messages, setMessages] = useState<FeedMessage[] | null>(null);
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  // The open thread, by the seq of the message it hangs under. A seq is the
  // right handle here: it is what the server addresses a thread by, and it
  // survives a refetch that replaces every message object.
  const [threadSeq, setThreadSeq] = useState<number | null>(null);
  const [replies, setReplies] = useState<Message[] | null>(null);
  const [replyDraft, setReplyDraft] = useState("");
  const [replying, setReplying] = useState(false);
  // What may be left, per the server. Empty until it answers, which simply
  // means no picker yet — never a picker offering emoji it would refuse.
  const [palette, setPalette] = useState<string[]>([]);
  // Files chosen but not yet sent. Held as Drive nodes so the composer can
  // show their names without a second lookup.
  const [staged, setStaged] = useState<DriveNodeDto[]>([]);
  const [picking, setPicking] = useState(false);
  // Who can be named here: the room's people and its agents, in one list,
  // because the person typing does not care which kind they are reaching for.
  const [nameable, setNameable] = useState<Nameable[]>([]);
  const [highlighted, setHighlighted] = useState(0);
  const feedRef = useRef<HTMLDivElement | null>(null);
  const composerRef = useRef<HTMLTextAreaElement | null>(null);
  const [caret, setCaret] = useState(0);
  const [showingPeople, setShowingPeople] = useState(false);
  // One composer popover at a time: opening either closes the other, so two
  // menus can never sit open over each other.
  const [composerMenu, setComposerMenu] = useState<"share" | "emoji" | null>(
    null,
  );
  const composerMenuRef = useRef<HTMLDivElement | null>(null);
  // Which room's row menu is open, by id — one at a time, same reason.
  const [emojiQuery, setEmojiQuery] = useState("");
  // Agent turns running in the open room. Refetched on every push, so it
  // follows the same signal the messages do rather than polling on a timer.
  const [turns, setTurns] = useState<Turn[]>([]);
  // What was half-typed in each room. Switching rooms to check something and
  // losing a sentence is a small betrayal every chat app learned to avoid.
  const drafts = useRef<Map<string, string>>(new Map());
  const [switcher, setSwitcher] = useState<string | null>(null);
  const [dropping, setDropping] = useState(false);
  // Where reading stopped when this room was opened. Held still afterwards:
  // the line must not creep down as new messages land while you are looking.
  const [readUpTo, setReadUpTo] = useState<number | null>(null);
  // On a phone the two columns become one screen at a time, the way Mail
  // already does it: the list until you pick a room, the room until you come
  // back. Two columns on a 390px screen gave the conversation 58 pixels.
  const isMobile = useIsMobile();
  // A meeting belongs to the room it was started from, so the room is where
  // starting one lives. Whoever is in the conversation is who it is for.
  const meet = useMeetApi();
  const [inMeeting, setInMeeting] = useState<string | null>(null);
  const [liveMeeting, setLiveMeeting] = useState<Meeting | null>(null);
  const [rowMenu, setRowMenu] = useState<string | null>(null);
  const rowMenuRef = useRef<HTMLDivElement | null>(null);
  const closeRowMenu = useCallback(() => setRowMenu(null), []);
  useDismiss(rowMenu !== null, rowMenuRef, closeRowMenu);
  const closeComposerMenu = useCallback(() => setComposerMenu(null), []);
  useDismiss(composerMenu !== null, composerMenuRef, closeComposerMenu);
  // Whether anything remains behind the oldest line held. Derived from the
  // last page's size rather than a count, because a count would be a second
  // truth about the same thing.
  // The live public channels not yet joined. Loaded on demand: it is a
  // browsing act, not something every sidebar draw should pay for.
  const [browsing, setBrowsing] = useState<Channel[] | null>(null);
  // Starting a conversation with someone: the search text, and what it found.
  const [dmQuery, setDmQuery] = useState<string | null>(null);
  const [dmFound, setDmFound] = useState<Person[]>([]);
  const [moreBehind, setMoreBehind] = useState(false);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [finding, setFinding] = useState("");
  const [found, setFound] = useState<Message[] | null>(null);

  const loadChannels = useCallback(async () => {
    try {
      const rooms = await api.channels();
      setChannels(rooms);
      setOpenId((current) => current ?? rooms[0]?.id ?? null);
    } catch (failure) {
      setError(chatMessage(failure, strings.chatLoadFailed));
    }
  }, [api]);

  const loadTurns = useCallback(
    async (id: string) => {
      try {
        setTurns(await api.turns(id));
      } catch {
        // A room that cannot say who is thinking is not broken; it just says
        // nothing. Never surface this.
        setTurns([]);
      }
    },
    [api],
  );

  const loadMessages = useCallback(
    async (id: string) => {
      try {
        // Newest-first on the wire; the feed reads oldest-first.
        const page = await api.messages(id);
        setMessages([...page].reverse());
        setMoreBehind(page.length === PAGE);
        const newest = page[0]?.seq;
        if (newest !== undefined) await api.markRead(id, newest);
      } catch (failure) {
        setError(chatMessage(failure, strings.chatLoadFailed));
      }
    },
    [api],
  );

  const loadReplies = useCallback(
    async (id: string, rootSeq: number) => {
      try {
        setReplies(await api.thread(id, rootSeq));
      } catch (failure) {
        setError(chatMessage(failure, strings.chatThreadFailed));
      }
    },
    [api],
  );

  useEffect(() => {
    void loadChannels();
  }, [loadChannels]);

  useEffect(() => {
    // Asked once: the offered set changes with a release, not with a room.
    void api
      .reactionPalette()
      .then(setPalette)
      .catch(() => setPalette([]));
  }, [api]);

  useEffect(() => {
    if (openId === null) return;
    setMessages(null);
    // A thread belongs to the room it was opened in; changing rooms closes it
    // rather than leaving a panel of someone else's replies on screen.
    setThreadSeq(null);
    setReplies(null);
    setTurns([]);
    setDraft(drafts.current.get(openId) ?? "");
    void meet
      .liveIn(openId)
      .then((live) => setLiveMeeting(live[0] ?? null))
      .catch(() => setLiveMeeting(null));
    setReadUpTo(channels?.find((c) => c.id === openId)?.lastReadSeq ?? null);
    void loadMessages(openId);
    void loadTurns(openId);
  }, [openId, loadMessages, loadTurns]);

  // Reloaded per room: membership is per room, and so is which agents are in
  // it. Best-effort — a composer that cannot suggest still sends.
  useEffect(() => {
    if (openId === null) return;
    let live = true;
    void (async () => {
      const [detail, agents] = await Promise.all([
        api.channel(openId).catch(() => null),
        api.channelAgents(openId).catch(() => []),
      ]);
      if (!live) return;
      const people: Nameable[] = (detail?.members ?? []).map((m) => ({
        handle: (m.email ?? m.user).split("@")[0]!.toLowerCase(),
        label: m.email ?? m.user,
        agent: false,
      }));
      setNameable([
        ...agents.map((a) => ({
          handle: a.handle,
          label: a.name,
          agent: true,
        })),
        ...people,
      ]);
    })();
    return () => {
      live = false;
    };
  }, [api, openId]);

  useEffect(() => {
    if (openId === null || threadSeq === null) return;
    setReplies(null);
    void loadReplies(openId, threadSeq);
  }, [openId, threadSeq, loadReplies]);

  // Live: a chat signal refreshes the sidebar, the open room, and the open
  // thread — a reply arriving while someone reads the thread is exactly the
  // case the stream exists for.
  useEffect(() => {
    const controller = new AbortController();
    let live = true;
    void (async () => {
      while (live && !controller.signal.aborted) {
        try {
          await client.subscribeChat(() => {
            void loadChannels();
            if (openId !== null) {
              void loadMessages(openId);
              void loadTurns(openId);
              if (threadSeq !== null) void loadReplies(openId, threadSeq);
            }
          }, controller.signal);
        } catch {
          // A dropped stream is normal (sleep, proxy timeout); pause, reconnect.
          await new Promise((resume) => setTimeout(resume, 3_000));
        }
      }
    })();
    return () => {
      live = false;
      controller.abort();
    };
  }, [client, openId, threadSeq, loadChannels, loadMessages, loadReplies]);

  // Keep the newest line in view as the conversation grows — but only when
  // the newest line actually changed. Scrolling to the bottom after prepending
  // older history would throw the reader out of what they went back to read.
  const newestSeq = messages?.[messages.length - 1]?.seq ?? null;
  useEffect(() => {
    const feed = feedRef.current;
    if (feed !== null) feed.scrollTop = feed.scrollHeight;
  }, [newestSeq, openId]);

  async function send() {
    const body = draft.trim();
    if (body === "" || openId === null || sending) return;
    const files = staged;
    setSending(true);
    setError(null);
    setDraft("");
    setStaged([]);
    try {
      const sent = await api.post(
        openId,
        body,
        undefined,
        files.map((f) => f.id),
      );
      // A message just said has no replies yet; the refetch will confirm it.
      setMessages((current) => [
        ...(current ?? []),
        { ...sent, replyCount: 0, lastReplyAt: null },
      ]);
      void loadChannels();
    } catch (failure) {
      // Give back the words AND the files: the server refuses the whole post
      // when a file is not shareable, so nothing was said and nothing should
      // be lost.
      setDraft(body);
      setStaged(files);
      setError(chatMessage(failure, strings.chatSendFailed));
    } finally {
      setSending(false);
    }
  }

  async function openFile(file: Attachment) {
    try {
      saveBlob(await client.driveDownload(file.node), file.name);
    } catch (failure) {
      // Drive is the authority here: if it will not serve the bytes, the
      // reader has lost access since the message was written.
      setError(chatMessage(failure, strings.chatAttachFailed));
    }
  }

  /** Put `text` where the caret is and keep typing there — used by the share
   *  menu and the emoji picker, so neither has to know how the composer
   *  works. */
  function insertAtCaret(text: string) {
    const at = composerRef.current?.selectionStart ?? draft.length;
    setDraft(`${draft.slice(0, at)}${text}${draft.slice(at)}`);
    const next = at + text.length;
    requestAnimationFrame(() => {
      composerRef.current?.focus();
      composerRef.current?.setSelectionRange(next, next);
      setCaret(next);
    });
  }

  /** Wrap the selection in `before`/`after`, or if nothing is selected, insert
   *  both and leave the caret between them ready to type. */
  function wrapSelection(before: string, after: string, sample: string) {
    const box = composerRef.current;
    if (box === null) return;
    const from = box.selectionStart ?? draft.length;
    const to = box.selectionEnd ?? from;
    const chosen = draft.slice(from, to);
    const body = chosen === "" ? sample : chosen;
    const next = `${draft.slice(0, from)}${before}${body}${after}${draft.slice(to)}`;
    setDraft(next);
    if (openId !== null) drafts.current.set(openId, next);
    // Select the sample so the next keystroke replaces it — the mark is done,
    // the words are what you type now.
    const start = from + before.length;
    requestAnimationFrame(() => {
      composerRef.current?.focus();
      composerRef.current?.setSelectionRange(start, start + body.length);
      setCaret(start + body.length);
    });
  }

  /** Put the chosen handle where the `@token` was, and carry on typing. */
  function complete(choice: Nameable) {
    const found = mentionAt(draft, caret);
    if (found === null) return;
    const next = `${draft.slice(0, found.start)}@${choice.handle} ${draft.slice(caret)}`;
    setDraft(next);
    setHighlighted(0);
    // Put the caret after what was just inserted, not at the end of the line:
    // people complete a name mid-sentence.
    const at = found.start + choice.handle.length + 2;
    requestAnimationFrame(() => {
      composerRef.current?.focus();
      composerRef.current?.setSelectionRange(at, at);
      setCaret(at);
    });
  }

  /** Look for something that was said. Debounced by the keystroke that
   *  triggers it rather than a timer: a short question is cheap, and a timer
   *  would make the first result feel late. */
  async function find(query: string) {
    setFinding(query);
    if (query.trim() === "") {
      setFound(null);
      return;
    }
    try {
      setFound(await api.search(query));
    } catch (failure) {
      setError(chatMessage(failure, strings.chatSearchFailed));
      setFound([]);
    }
  }

  /** Fetch the page behind the oldest line held, and keep the reader where
   *  they were.
   *
   *  Prepending changes the scroll height, so without correction the content
   *  under the cursor jumps — the single thing that makes an infinite feed
   *  feel broken. The height is measured before and after, and the difference
   *  is added back to the scroll position.
   */
  async function findPeople(query: string) {
    setDmQuery(query);
    if (query.trim().length < 2) {
      // The server wants two characters; asking with fewer would be a request
      // that always answers nothing.
      setDmFound([]);
      return;
    }
    try {
      setDmFound(await api.findPeople(query));
    } catch (failure) {
      setError(chatMessage(failure, strings.chatPeopleFailed));
      setDmFound([]);
    }
  }

  async function openDm(person: Person) {
    setError(null);
    try {
      // Opening the same DM twice returns the same room, so this is safe to
      // press again — the server settles it, not a check here.
      const room = await api.createChannel({ kind: "dm", with: person.user });
      await loadChannels();
      setDmQuery(null);
      setDmFound([]);
      setOpenId(room.id);
    } catch (failure) {
      setError(chatMessage(failure, strings.chatDmFailed));
    }
  }

  async function renameRoom(room: ChannelSummary) {
    const name = (
      await dialogs.prompt({
        title: strings.chatRename,
        message: strings.chatRenamePrompt,
        defaultValue: room.name ?? "",
        confirmLabel: strings.chatRenameSave,
      })
    )?.trim();
    if (
      name === undefined ||
      name === null ||
      name === "" ||
      name === room.name
    )
      return;
    try {
      await api.renameChannel(room.id, { name });
      await loadChannels();
    } catch (failure) {
      setError(chatMessage(failure, strings.chatRenameFailed));
    }
  }

  async function archiveRoom(room: ChannelSummary) {
    // Confirmed, because it changes the room for everyone in it — and said in
    // terms of what actually happens, since nothing is deleted.
    const sure = await dialogs.confirm({
      title: strings.chatArchiveTitle(room.name ?? strings.chatDirectMessage),
      message: strings.chatArchiveWarning,
      confirmLabel: strings.chatArchiveConfirm,
    });
    if (!sure) return;
    try {
      await api.archiveChannel(room.id);
      await loadChannels();
    } catch (failure) {
      setError(chatMessage(failure, strings.chatArchiveFailed));
    }
  }

  async function shareDropped(files: FileList) {
    setDropping(false);
    if (files.length === 0) return;
    setError(null);
    try {
      for (const file of Array.from(files).slice(0, ATTACHMENTS_MAX)) {
        // Into Drive first, then staged as a pointer: dropping a file into a
        // room should leave it somewhere the person can find again, not only
        // inside a conversation.
        const id = await client.driveUpload(null, null, file);
        const node = await client.driveNode(id);
        if (node === null) continue;
        setStaged((held) =>
          held.length >= ATTACHMENTS_MAX || held.some((h) => h.id === node.id)
            ? held
            : [...held, node],
        );
      }
    } catch (failure) {
      setError(chatMessage(failure, strings.chatAttachFailed));
    }
  }

  async function browse() {
    setError(null);
    try {
      // Everything public, not only what is left to join. Browsing a directory
      // that hides the rooms you are already in reads as empty and broken —
      // which is exactly how it read.
      const open = await api.joinable();
      const mine = (channels ?? []).filter(
        (c) =>
          c.kind === "channel" &&
          c.visibility === "public" &&
          c.archivedAt === null,
      );
      const all = [...open, ...mine].sort((a, b) =>
        (a.name ?? "").localeCompare(b.name ?? ""),
      );
      setBrowsing(all);
    } catch (failure) {
      setError(chatMessage(failure, strings.chatBrowseFailed));
      setBrowsing([]);
    }
  }

  async function joinRoom(channel: Channel) {
    setError(null);
    try {
      await api.join(channel.id);
      await loadChannels();
      setBrowsing(null);
      // Open what was just joined: joining in order to then hunt for it in the
      // list would be a step the person did not ask for.
      setOpenId(channel.id);
    } catch (failure) {
      setError(chatMessage(failure, strings.chatJoinFailed));
    }
  }

  async function loadOlder() {
    const feed = feedRef.current;
    const oldest = messages?.[0]?.seq;
    if (openId === null || oldest === undefined || loadingOlder) return;
    setLoadingOlder(true);
    const before = feed?.scrollHeight ?? 0;
    try {
      const page = await api.messages(openId, oldest);
      if (page.length > 0) {
        setMessages((held) => [...[...page].reverse(), ...(held ?? [])]);
      }
      setMoreBehind(page.length === PAGE);
      requestAnimationFrame(() => {
        const now = feedRef.current;
        if (now !== null) now.scrollTop += now.scrollHeight - before;
      });
    } catch (failure) {
      setError(chatMessage(failure, strings.chatLoadFailed));
    } finally {
      setLoadingOlder(false);
    }
  }

  async function editMessage(message: Message, body: string) {
    setError(null);
    try {
      await api.editMessage(message.id, body);
    } catch (failure) {
      setError(chatMessage(failure, strings.chatEditFailed));
    }
    if (openId !== null) void loadMessages(openId);
    if (openId !== null && threadSeq !== null)
      void loadReplies(openId, threadSeq);
  }

  async function withdrawMessage(message: Message) {
    setError(null);
    try {
      await api.withdrawMessage(message.id);
    } catch (failure) {
      setError(chatMessage(failure, strings.chatWithdrawFailed));
    }
    if (openId !== null) void loadMessages(openId);
    if (openId !== null && threadSeq !== null)
      void loadReplies(openId, threadSeq);
  }

  async function decide(proposal: Proposal, approve: boolean) {
    setError(null);
    try {
      await api.decideProposal(proposal.id, approve);
    } catch (failure) {
      // Includes the 403 for someone else's proposal, said in the server's
      // own words rather than a guess made here.
      setError(chatMessage(failure, strings.chatDecideFailed));
    }
    // Either way the room moved: approving ran the action, and both outcomes
    // settle a card the whole room is watching.
    if (openId !== null) void loadMessages(openId);
  }

  async function react(messageId: string, emoji: string) {
    try {
      const tally = await api.react(messageId, emoji);
      // Apply where it is shown — a message can be on screen in the feed, in
      // the thread panel, or in both at once.
      const applied = <T extends Message>(list: T[] | null): T[] | null =>
        list?.map((m) =>
          m.id === messageId ? { ...m, reactions: tally } : m,
        ) ?? null;
      setMessages(applied);
      setReplies(applied);
    } catch (failure) {
      setError(chatMessage(failure, strings.chatReactFailed));
    }
  }

  async function sendReply() {
    const body = replyDraft.trim();
    if (body === "" || openId === null || threadSeq === null || replying)
      return;
    setReplying(true);
    setError(null);
    setReplyDraft("");
    try {
      const sent = await api.post(openId, body, threadSeq);
      setReplies((current) => [...(current ?? []), sent]);
      // The feed shows the count, so it has to hear about this too.
      void loadMessages(openId);
    } catch (failure) {
      setReplyDraft(body);
      setError(chatMessage(failure, strings.chatSendFailed));
    } finally {
      setReplying(false);
    }
  }

  async function createChannel() {
    // The app's own dialog, not the browser's: a native prompt says
    // "localhost:5173 says" and looks nothing like the product it is part of.
    const name = (
      await dialogs.prompt({
        title: strings.chatNewChannel,
        message: strings.chatNewChannelPrompt,
        placeholder: strings.chatNewChannelPlaceholder,
        confirmLabel: strings.chatCreate,
      })
    )?.trim();
    if (name === undefined || name === null || name === "") return;
    setCreating(true);
    setError(null);
    try {
      const room = await api.createChannel({ name });
      await loadChannels();
      setOpenId(room.id);
    } catch (failure) {
      setError(chatMessage(failure, strings.chatCreateFailed));
    } finally {
      setCreating(false);
    }
  }

  // Typing anywhere types into the composer. Nobody should have to find the
  // field first; in a room, keystrokes mean a message unless they plainly mean
  // something else.
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.ctrlKey || event.metaKey || event.altKey) return;
      if (event.key.length !== 1) return;
      const at = document.activeElement;
      const editable =
        at instanceof HTMLInputElement ||
        at instanceof HTMLTextAreaElement ||
        (at instanceof HTMLElement && at.isContentEditable);
      if (editable) return;
      const box = composerRef.current;
      if (box === null) return;
      box.focus();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  // Ctrl/Cmd+K anywhere in the module. Registered on the document because a
  // switcher you must first click into is not a switcher.
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setSwitcher((at) => (at === null ? "" : null));
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  const open = channels?.find((c) => c.id === openId) ?? null;
  // Channels, then people, then what is archived — the order a person looks
  // in. One flat list is what hid direct messages entirely.
  const rooms = channels ?? [];
  const sections: { label: string; rooms: ChannelSummary[] }[] = [
    {
      label: strings.chatSectionChannels,
      rooms: rooms.filter((c) => c.kind === "channel" && c.archivedAt === null),
    },
    {
      label: strings.chatSectionDirect,
      rooms: rooms.filter((c) => c.kind === "dm" && c.archivedAt === null),
    },
    {
      label: strings.chatSectionArchived,
      rooms: rooms.filter((c) => c.archivedAt !== null),
    },
  ].filter((section) => section.rooms.length > 0);
  // Derived, not stored: the list is a function of what is typed and where
  // the caret is, so it can never disagree with the composer.
  // Null while nothing is being searched for: the picker shows its groups.
  const emojiHits = emojiQuery.trim() === "" ? null : searchEmoji(emojiQuery);
  // Every room, filtered by what has been typed — including archived ones,
  // because jumping to something old is exactly when you cannot find it in
  // the list.
  const switcherHits = (channels ?? [])
    .filter((c) =>
      channelLabel(c)
        .toLowerCase()
        .includes((switcher ?? "").toLowerCase()),
    )
    .slice(0, 8);
  const mention = mentionAt(draft, caret);
  const suggestions =
    mention === null ? [] : candidatesFor(mention.token, nameable);
  // The message a thread hangs under, taken from the feed the panel was opened
  // from. If a refetch drops it (someone withdrew it), the panel closes with
  // it rather than floating replies under nothing.
  const threadRoot =
    threadSeq === null
      ? null
      : (messages?.find((m) => m.seq === threadSeq) ?? null);

  return (
    <div className={styles.module}>
      {(!isMobile || openId === null) && (
        <aside className={styles.sidebar}>
          <header className={styles.sidebarHeader}>
            <h2 className={styles.sidebarTitle}>{strings.moduleChat}</h2>
          </header>

          {/* Named, not iconographic. Three unlabelled glyphs in a corner is how
            "start a DM" and "add an agent" became things nobody could find. */}
          <div className={styles.actions}>
            <button
              type="button"
              className={styles.action}
              onClick={() => void createChannel()}
              disabled={creating}
            >
              <MessageSquarePlus size={16} className={styles.actionIcon} />
              {strings.chatNewChannel}
            </button>
            <button
              type="button"
              className={styles.action}
              onClick={() => setDmQuery("")}
            >
              <Users size={16} className={styles.actionIcon} />
              {strings.chatNewDm}
            </button>
            <button
              type="button"
              className={styles.action}
              onClick={() => void browse()}
            >
              <Hash size={16} className={styles.actionIcon} />
              {strings.chatBrowse}
            </button>
          </div>
          <div className={styles.searchRow}>
            <Search size={14} className={styles.channelIcon} />
            <input
              className={styles.search}
              value={finding}
              onChange={(event) => void find(event.target.value)}
              placeholder={strings.chatSearchPlaceholder}
              aria-label={strings.chatSearchPlaceholder}
              autoComplete="off"
            />
            {finding !== "" && (
              <button
                type="button"
                className={styles.searchClear}
                onClick={() => void find("")}
                aria-label={strings.chatSearchClear}
              >
                <X size={14} />
              </button>
            )}
          </div>

          {dmQuery !== null ? (
            <div className={styles.browsePane}>
              <div className={styles.browseHead}>
                <span className={styles.browseTitle}>{strings.chatNewDm}</span>
                <button
                  type="button"
                  className={styles.searchClear}
                  onClick={() => {
                    setDmQuery(null);
                    setDmFound([]);
                  }}
                  aria-label={strings.chatClose}
                >
                  <X size={14} />
                </button>
              </div>
              <div className={styles.searchRow}>
                <Users size={14} className={styles.channelIcon} />
                <input
                  className={styles.search}
                  value={dmQuery}
                  onChange={(event) => void findPeople(event.target.value)}
                  placeholder={strings.chatFindPerson}
                  aria-label={strings.chatFindPerson}
                  autoComplete="off"
                  autoFocus
                />
              </div>
              {dmFound.length === 0 ? (
                <p className={styles.sidebarNote}>
                  {dmQuery.trim().length < 2
                    ? strings.chatFindPersonHint
                    : strings.chatNobodyFound}
                </p>
              ) : (
                <ul className={styles.channelList}>
                  {dmFound.map((person) => (
                    <li key={person.user}>
                      <button
                        type="button"
                        className={styles.channel}
                        onClick={() => void openDm(person)}
                      >
                        <Avatar
                          name={person.email}
                          email={person.email}
                          size="sm"
                        />
                        <span className={styles.channelName}>
                          {person.email}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          ) : browsing !== null ? (
            <div className={styles.browsePane}>
              <div className={styles.browseHead}>
                <span className={styles.browseTitle}>{strings.chatBrowse}</span>
                <button
                  type="button"
                  className={styles.searchClear}
                  onClick={() => setBrowsing(null)}
                  aria-label={strings.chatClose}
                >
                  <X size={14} />
                </button>
              </div>
              {browsing.length === 0 ? (
                <p className={styles.sidebarNote}>
                  {strings.chatNothingToJoin}
                </p>
              ) : (
                <ul className={styles.channelList}>
                  {browsing.map((room) => (
                    <li key={room.id}>
                      <button
                        type="button"
                        className={styles.channel}
                        onClick={() => {
                          if ((channels ?? []).some((c) => c.id === room.id)) {
                            setOpenId(room.id);
                            setBrowsing(null);
                          } else {
                            void joinRoom(room);
                          }
                        }}
                      >
                        <Hash size={15} className={styles.channelIcon} />
                        <span className={styles.channelName}>{room.name}</span>
                        <span className={styles.joinHint}>
                          {(channels ?? []).some((c) => c.id === room.id)
                            ? strings.chatJoined
                            : strings.chatJoin}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          ) : found !== null ? (
            found.length === 0 ? (
              <p className={styles.sidebarNote}>{strings.chatSearchNothing}</p>
            ) : (
              <ul className={styles.channelList}>
                {found.map((hit) => (
                  <li key={hit.id}>
                    <button
                      type="button"
                      className={styles.hit}
                      onClick={() => {
                        // Open the room it was said in. The message is not
                        // scrolled to yet — see chatSearchOpensRoom.
                        setOpenId(hit.channel);
                        void find("");
                      }}
                    >
                      <span className={styles.hitWho}>
                        {hit.authorKind === "agent"
                          ? (hit.authorEmail ?? hit.author)
                          : personName(hit.authorEmail, hit.author)}
                      </span>
                      <span className={styles.hitBody}>{hit.body}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )
          ) : channels === null ? (
            <p className={styles.sidebarNote}>
              <Loader2 className={styles.spin} size={14} />{" "}
              {strings.chatLoading}
            </p>
          ) : channels.length === 0 ? (
            // Law 5: an empty room list teaches the one next step.
            <div className={styles.emptySidebar}>
              <p className={styles.emptyLead}>{strings.chatNoChannelsLead}</p>
              <p className={styles.emptyHint}>{strings.chatNoChannelsHint}</p>
            </div>
          ) : (
            <div className={styles.channelList} ref={rowMenuRef}>
              {sections.map((section) => (
                <section key={section.label}>
                  <h3 className={styles.sectionLabel}>{section.label}</h3>
                  <ul className={styles.sectionList}>
                    {section.rooms.map((channel) => (
                      // The row is the target; its menu sits beside the button rather
                      // than inside it, because a button inside a button is invalid
                      // and swallows the click.
                      <li key={channel.id} className={styles.channelRow}>
                        <button
                          type="button"
                          className={[
                            channel.id === openId
                              ? styles.channelOpen
                              : styles.channel,
                            // An archived room stays reachable (its history is still
                            // the team's), but it must not read as a live one.
                            channel.archivedAt !== null
                              ? styles.channelArchived
                              : "",
                          ]
                            .filter(Boolean)
                            .join(" ")}
                          onClick={() => setOpenId(channel.id)}
                        >
                          {channel.kind === "dm" ? (
                            <Users size={15} className={styles.channelIcon} />
                          ) : channel.visibility === "private" ? (
                            <Lock size={15} className={styles.channelIcon} />
                          ) : (
                            <Hash size={15} className={styles.channelIcon} />
                          )}
                          <span className={styles.channelName}>
                            {channelLabel(channel)}
                          </span>
                          {channel.archivedAt !== null && (
                            <Archive
                              size={13}
                              className={styles.channelIcon}
                              aria-label={strings.chatArchived}
                            />
                          )}
                          {channel.mentions > 0 ? (
                            // A room with something addressed to you says so, rather
                            // than hiding it inside a larger unread number.
                            <span
                              className={styles.badgeMention}
                              title={strings.chatMentionsYou(channel.mentions)}
                            >
                              @{channel.mentions}
                            </span>
                          ) : (
                            channel.unread > 0 && (
                              <span className={styles.badge}>
                                {channel.unread}
                              </span>
                            )
                          )}
                        </button>
                        {channel.kind === "channel" &&
                          channel.archivedAt === null && (
                            <span className={styles.rowMenuWrap}>
                              <button
                                type="button"
                                className={styles.rowMenuButton}
                                onClick={() =>
                                  setRowMenu((at) =>
                                    at === channel.id ? null : channel.id,
                                  )
                                }
                                aria-label={strings.chatChannelActions(
                                  channelLabel(channel),
                                )}
                                aria-expanded={rowMenu === channel.id}
                              >
                                <MoreHorizontal size={16} />
                              </button>
                              {rowMenu === channel.id && (
                                <span className={styles.rowMenu} role="menu">
                                  <button
                                    type="button"
                                    role="menuitem"
                                    className={styles.rowMenuItem}
                                    onClick={() => {
                                      setRowMenu(null);
                                      void renameRoom(channel);
                                    }}
                                  >
                                    <Pencil size={14} />
                                    {strings.chatRename}
                                  </button>
                                  <button
                                    type="button"
                                    role="menuitem"
                                    className={styles.rowMenuItem}
                                    onClick={() => {
                                      setRowMenu(null);
                                      void archiveRoom(channel);
                                    }}
                                  >
                                    <Archive size={14} />
                                    {strings.chatArchiveAction}
                                  </button>
                                </span>
                              )}
                            </span>
                          )}
                      </li>
                    ))}
                  </ul>
                </section>
              ))}
            </div>
          )}
        </aside>
      )}

      <section
        className={styles.room}
        onDragOver={(event) => {
          if (!event.dataTransfer.types.includes("Files")) return;
          event.preventDefault();
          setDropping(true);
        }}
        onDragLeave={(event) => {
          // Only when the pointer has truly left the room, not merely
          // crossed onto a child of it.
          if (!event.currentTarget.contains(event.relatedTarget as Node))
            setDropping(false);
        }}
        onDrop={(event) => {
          if (!event.dataTransfer.types.includes("Files")) return;
          event.preventDefault();
          void shareDropped(event.dataTransfer.files);
        }}
      >
        {dropping && (
          <div className={styles.dropVeil}>
            <span className={styles.dropWord}>
              <Paperclip size={16} />
              {strings.chatDropFiles}
            </span>
          </div>
        )}
        {open === null ? (
          <div className={styles.emptyRoom}>
            <Hash size={28} className={styles.emptyRoomIcon} />
            <p className={styles.emptyLead}>{strings.chatNoRoomOpenLead}</p>
            <p className={styles.emptyHint}>{strings.chatNoRoomOpenHint}</p>
          </div>
        ) : (
          <>
            <header className={styles.roomHeader}>
              {isMobile && (
                // The way back. Without it a phone opens a room and stays
                // there for ever.
                <button
                  type="button"
                  className={styles.backButton}
                  onClick={() => setOpenId(null)}
                  aria-label={strings.chatBackToList}
                >
                  <ChevronLeft size={18} />
                </button>
              )}
              <div className={styles.roomTitle}>
                <h3 className={styles.roomName}>{channelLabel(open)}</h3>
                {open.topic !== null && (
                  <p className={styles.roomTopic}>{open.topic}</p>
                )}
              </div>
              <div className={styles.roomActions}>
                <button
                  type="button"
                  className={styles.roomMeet}
                  onClick={() => {
                    if (liveMeeting !== null) {
                      setInMeeting(liveMeeting.id);
                      return;
                    }
                    void meet
                      .start({ channel: open.id, title: channelLabel(open) })
                      .then((m) => {
                        setLiveMeeting(m);
                        setInMeeting(m.id);
                      })
                      .catch(() => setError(strings.meetJoinFailed));
                  }}
                  title={
                    liveMeeting !== null ? strings.meetJoin : strings.meetStart
                  }
                >
                  <Video size={15} />
                  {liveMeeting !== null ? strings.meetLive : strings.meetStart}
                </button>
                <button
                  type="button"
                  className={styles.roomPeople}
                  onClick={() => setShowingPeople(true)}
                  title={strings.chatMembersAndAgents}
                >
                  <Users size={15} />
                  {strings.chatMembersAndAgents}
                </button>
                {open.kind === "channel" && open.archivedAt === null && (
                  <>
                    <button
                      type="button"
                      className={styles.roomIcon}
                      onClick={() => void renameRoom(open)}
                      aria-label={strings.chatRename}
                      title={strings.chatRename}
                    >
                      <Pencil size={15} />
                    </button>
                    <button
                      type="button"
                      className={styles.roomIcon}
                      onClick={() => void archiveRoom(open)}
                      aria-label={strings.chatArchiveAction}
                      title={strings.chatArchiveAction}
                    >
                      <Archive size={15} />
                    </button>
                  </>
                )}
              </div>
            </header>

            {turns.length > 0 && (
              <div className={styles.thinkingRow}>
                {turns.map((turn) => (
                  <span key={turn.id} className={styles.thinking}>
                    <Sparkles size={13} className={styles.thinkingMark} />
                    <span className={styles.thinkingDots} aria-hidden="true">
                      <i />
                      <i />
                      <i />
                    </span>
                    {strings.chatThinking(turn.handle)}
                    {/* Only the person who asked may stop it — the same rule
                        as approving what it proposes. */}
                    {turn.mine && openId !== null && (
                      <button
                        type="button"
                        className={styles.stop}
                        onClick={() => {
                          void api
                            .stopTurn(openId, turn.id)
                            .then(() => loadTurns(openId));
                        }}
                      >
                        {strings.chatStop}
                      </button>
                    )}
                  </span>
                ))}
              </div>
            )}

            <div className={styles.feed} ref={feedRef}>
              {/* An explicit control rather than a scroll trigger: a feed that
                  loads on approach fires while someone is simply reading back,
                  and there is no way to tell it to stop. */}
              {!moreBehind && messages !== null && (
                <div className={styles.beginning}>
                  <h4 className={styles.beginningName}>
                    {open.kind === "dm"
                      ? strings.chatBeginningDm
                      : strings.chatBeginning(channelLabel(open))}
                  </h4>
                  {open.topic !== null && (
                    <p className={styles.beginningTopic}>{open.topic}</p>
                  )}
                </div>
              )}
              {moreBehind && messages !== null && (
                <button
                  type="button"
                  className={styles.older}
                  onClick={() => void loadOlder()}
                  disabled={loadingOlder}
                >
                  {loadingOlder ? strings.chatLoading : strings.chatOlder}
                </button>
              )}
              {messages === null ? (
                <p className={styles.feedNote}>
                  <Loader2 className={styles.spin} size={14} />{" "}
                  {strings.chatLoading}
                </p>
              ) : messages.length === 0 ? (
                <p className={styles.feedNote}>{strings.chatNoMessagesYet}</p>
              ) : (
                messages.map((message, i) => (
                  <Fragment key={message.id}>
                    {readUpTo !== null &&
                      message.seq > readUpTo &&
                      (messages[i - 1]?.seq ?? 0) <= readUpTo &&
                      i > 0 && (
                        <div className={styles.unread}>
                          <span className={styles.unreadLabel}>
                            {strings.chatNewMessages}
                          </span>
                        </div>
                      )}
                    {(i === 0 ||
                      dayOf(message.createdAt) !==
                        dayOf(messages[i - 1]!.createdAt)) && (
                      <div className={styles.day}>
                        <span className={styles.dayLabel}>
                          {dayOf(message.createdAt)}
                        </span>
                      </div>
                    )}
                    <MessageLine
                      message={message}
                      grouped={continues(message, messages[i - 1])}
                      palette={open.archivedAt === null ? palette : []}
                      me={me}
                      onReact={(emoji) => void react(message.id, emoji)}
                      onOpenFile={(file) => void openFile(file)}
                      onDecide={(p, ok) => void decide(p, ok)}
                      onEdit={(m, body) => void editMessage(m, body)}
                      onWithdraw={(m) => void withdrawMessage(m)}
                      onReply={
                        open.archivedAt === null
                          ? (m) => setThreadSeq(m.seq)
                          : undefined
                      }
                    >
                      {message.replyCount > 0 && (
                        <button
                          type="button"
                          className={styles.threadLink}
                          onClick={() => setThreadSeq(message.seq)}
                        >
                          <MessagesSquare size={13} />
                          {strings.chatReplies(message.replyCount)}
                        </button>
                      )}
                    </MessageLine>
                  </Fragment>
                ))
              )}
            </div>

            {error !== null && <p className={styles.error}>{error}</p>}

            {open.archivedAt !== null ? (
              // The server refuses new words in an archived room, so the
              // composer must not be offered at all: a control that looks
              // usable and answers with an error is worse than its absence.
              <p className={styles.archivedNote}>
                <Archive size={14} /> {strings.chatArchivedNote}
              </p>
            ) : (
              <form
                className={styles.composer}
                onSubmit={(event) => {
                  event.preventDefault();
                  void send();
                }}
              >
                {/* Visible, always. The syntax exists for people who know it; this is
                for everyone else, who should never have to learn markup to make a
                word bold. */}
                <div className={styles.formatBar}>
                  <button
                    type="button"
                    className={styles.formatTool}
                    onClick={() =>
                      wrapSelection("**", "**", strings.chatFormatHint)
                    }
                    aria-label={strings.chatBold}
                    title={`${strings.chatBold}  (Ctrl+B)`}
                  >
                    <Bold size={15} />
                  </button>
                  <button
                    type="button"
                    className={styles.formatTool}
                    onClick={() =>
                      wrapSelection("_", "_", strings.chatFormatHint)
                    }
                    aria-label={strings.chatItalic}
                    title={`${strings.chatItalic}  (Ctrl+I)`}
                  >
                    <Italic size={15} />
                  </button>
                  <button
                    type="button"
                    className={styles.formatTool}
                    onClick={() => wrapSelection("`", "`", "code")}
                    aria-label={strings.chatInlineCode}
                    title={strings.chatInlineCode}
                  >
                    <Code size={15} />
                  </button>
                  <button
                    type="button"
                    className={styles.formatTool}
                    onClick={() =>
                      wrapSelection(
                        "```" + String.fromCharCode(10),
                        String.fromCharCode(10) + "```",
                        "code",
                      )
                    }
                    aria-label={strings.chatCodeBlock}
                    title={strings.chatCodeBlock}
                  >
                    <SquareCode size={15} />
                  </button>
                  <button
                    type="button"
                    className={styles.formatTool}
                    onClick={() => wrapSelection("$", "$", "e^{i\\pi}+1=0")}
                    aria-label={strings.chatFormula}
                    title={strings.chatFormula}
                  >
                    <Sigma size={15} />
                  </button>
                  <button
                    type="button"
                    className={styles.formatTool}
                    onClick={() =>
                      wrapSelection(
                        String.fromCharCode(10) + "- ",
                        "",
                        strings.chatFormatHint,
                      )
                    }
                    aria-label={strings.chatBulletList}
                    title={strings.chatBulletList}
                  >
                    <List size={15} />
                  </button>
                  <button
                    type="button"
                    className={styles.formatTool}
                    onClick={() =>
                      wrapSelection(
                        String.fromCharCode(10) + "> ",
                        "",
                        strings.chatFormatHint,
                      )
                    }
                    aria-label={strings.chatQuoteAction}
                    title={strings.chatQuoteAction}
                  >
                    <Quote size={15} />
                  </button>
                </div>
                {staged.length > 0 && (
                  <ul className={styles.staged}>
                    {staged.map((file) => (
                      <li key={file.id}>
                        <button
                          type="button"
                          className={styles.stagedChip}
                          onClick={() =>
                            setStaged((held) =>
                              held.filter((f) => f.id !== file.id),
                            )
                          }
                          aria-label={strings.chatUnstage(file.name)}
                        >
                          <Paperclip size={13} />
                          <span className={styles.stagedName}>{file.name}</span>
                          <X size={13} />
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
                <div className={styles.composerMenus} ref={composerMenuRef}>
                  <span className={styles.shareWrap}>
                    <button
                      type="button"
                      className={styles.composerTool}
                      onClick={() =>
                        setComposerMenu((at) =>
                          at === "share" ? null : "share",
                        )
                      }
                      aria-label={strings.chatShare}
                      title={strings.chatShare}
                      aria-expanded={composerMenu === "share"}
                    >
                      <Plus size={18} />
                    </button>
                    {composerMenu === "share" && (
                      <div className={styles.shareMenu} role="menu">
                        <button
                          type="button"
                          role="menuitem"
                          className={styles.shareItem}
                          onClick={() => {
                            setComposerMenu(null);
                            setPicking(true);
                          }}
                        >
                          <Paperclip size={15} className={styles.shareIcon} />
                          <span>
                            <span className={styles.shareName}>
                              {strings.chatShareFile}
                            </span>
                            <span className={styles.shareHint}>
                              {strings.chatShareFileHint}
                            </span>
                          </span>
                        </button>
                        <button
                          type="button"
                          role="menuitem"
                          className={styles.shareItem}
                          onClick={() => {
                            setComposerMenu(null);
                            // Open the '@' list by typing the character the
                            // composer already understands, rather than
                            // inventing a second way to name someone.
                            insertAtCaret("@");
                          }}
                        >
                          <Users size={15} className={styles.shareIcon} />
                          <span>
                            <span className={styles.shareName}>
                              {strings.chatShareMention}
                            </span>
                            <span className={styles.shareHint}>
                              {strings.chatShareMentionHint}
                            </span>
                          </span>
                        </button>
                        <button
                          type="button"
                          role="menuitem"
                          className={styles.shareItem}
                          onClick={() => {
                            setComposerMenu(null);
                            insertAtCaret("@alo ");
                          }}
                        >
                          <Sparkles size={15} className={styles.shareIcon} />
                          <span>
                            <span className={styles.shareName}>
                              {strings.chatShareAsk}
                            </span>
                            <span className={styles.shareHint}>
                              {strings.chatShareAskHint}
                            </span>
                          </span>
                        </button>
                      </div>
                    )}
                  </span>
                  <span className={styles.shareWrap}>
                    <button
                      type="button"
                      className={styles.composerTool}
                      onClick={() =>
                        setComposerMenu((at) =>
                          at === "emoji" ? null : "emoji",
                        )
                      }
                      aria-label={strings.chatInsertEmoji}
                      title={strings.chatInsertEmoji}
                      aria-expanded={composerMenu === "emoji"}
                    >
                      <Smile size={18} />
                    </button>
                    {composerMenu === "emoji" && palette.length > 0 && (
                      <div className={styles.emojiMenu}>
                        <input
                          className={styles.emojiSearch}
                          value={emojiQuery}
                          onChange={(event) =>
                            setEmojiQuery(event.target.value)
                          }
                          placeholder={strings.chatEmojiSearch}
                          aria-label={strings.chatEmojiSearch}
                          autoComplete="off"
                          autoFocus
                        />
                        <div className={styles.emojiScroll}>
                          {emojiHits !== null ? (
                            emojiHits.length === 0 ? (
                              <p className={styles.emojiNone}>
                                {strings.chatEmojiNone}
                              </p>
                            ) : (
                              <div className={styles.emojiGrid}>
                                {emojiHits.map((glyph) => (
                                  <button
                                    key={glyph}
                                    type="button"
                                    className={styles.pickerOption}
                                    onClick={() => {
                                      setComposerMenu(null);
                                      setEmojiQuery("");
                                      insertAtCaret(glyph);
                                    }}
                                  >
                                    {glyph}
                                  </button>
                                ))}
                              </div>
                            )
                          ) : (
                            EMOJI.map((group) => (
                              <div key={group.name}>
                                <h4 className={styles.emojiHeading}>
                                  {group.name}
                                </h4>
                                <div className={styles.emojiGrid}>
                                  {group.items.map(([glyph]) => (
                                    <button
                                      key={glyph}
                                      type="button"
                                      className={styles.pickerOption}
                                      onClick={() => {
                                        setComposerMenu(null);
                                        insertAtCaret(glyph);
                                      }}
                                    >
                                      {glyph}
                                    </button>
                                  ))}
                                </div>
                              </div>
                            ))
                          )}
                        </div>
                      </div>
                    )}
                  </span>
                </div>
                {suggestions.length > 0 && (
                  <ul className={styles.suggestions} role="listbox">
                    {suggestions.map((choice, i) => (
                      <li key={`${choice.agent}-${choice.handle}`}>
                        <button
                          type="button"
                          role="option"
                          aria-selected={i === highlighted}
                          className={
                            i === highlighted
                              ? styles.suggestionOn
                              : styles.suggestion
                          }
                          // A mousedown, not a click: a click fires after the
                          // input has already lost focus and closed the list.
                          onMouseDown={(event) => {
                            event.preventDefault();
                            complete(choice);
                          }}
                        >
                          {choice.agent ? (
                            <Sparkles size={13} className={styles.agentHint} />
                          ) : (
                            <Users size={13} className={styles.channelIcon} />
                          )}
                          <span className={styles.suggestionHandle}>
                            @{choice.handle}
                          </span>
                          <span className={styles.suggestionLabel}>
                            {choice.label}
                          </span>
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
                <textarea
                  ref={composerRef}
                  rows={1}
                  className={styles.input}
                  value={draft}
                  onChange={(event) => {
                    setDraft(event.target.value);
                    if (openId !== null)
                      drafts.current.set(openId, event.target.value);
                    setCaret(event.target.selectionStart ?? 0);
                    setHighlighted(0);
                  }}
                  onSelect={(event) =>
                    setCaret(event.currentTarget.selectionStart ?? 0)
                  }
                  onKeyDown={(event) => {
                    if (
                      event.key === "Enter" &&
                      !event.shiftKey &&
                      suggestions.length === 0
                    ) {
                      event.preventDefault();
                      void send();
                      return;
                    }
                    if (suggestions.length === 0) return;
                    // While the list is open it owns these keys, so Enter
                    // completes a name instead of sending a half-typed one.
                    if (event.key === "ArrowDown") {
                      event.preventDefault();
                      setHighlighted((at) => (at + 1) % suggestions.length);
                    } else if (event.key === "ArrowUp") {
                      event.preventDefault();
                      setHighlighted(
                        (at) =>
                          (at - 1 + suggestions.length) % suggestions.length,
                      );
                    } else if (event.key === "Enter" || event.key === "Tab") {
                      event.preventDefault();
                      const choice = suggestions[highlighted];
                      if (choice !== undefined) complete(choice);
                    } else if (event.key === "Escape") {
                      event.preventDefault();
                      // Dismiss without choosing: move the caret past the
                      // token so the list stops matching it.
                      setCaret(draft.length);
                      setHighlighted(0);
                    }
                  }}
                  placeholder={strings.chatComposerPlaceholder(
                    channelLabel(open),
                  )}
                  aria-label={strings.chatComposerLabel}
                  autoComplete="off"
                />
                <button
                  type="submit"
                  className={styles.send}
                  disabled={draft.trim() === "" || sending}
                  aria-label={strings.chatSend}
                  title={strings.chatSend}
                >
                  <Send size={17} />
                </button>
              </form>
            )}
          </>
        )}
      </section>

      {inMeeting !== null && (
        <MeetRoom
          meetingId={inMeeting}
          onLeft={() => {
            setInMeeting(null);
            if (openId !== null) {
              void meet
                .liveIn(openId)
                .then((live) => setLiveMeeting(live[0] ?? null))
                .catch(() => setLiveMeeting(null));
            }
          }}
        />
      )}

      {switcher !== null && (
        <div
          className={styles.switcherBackdrop}
          role="dialog"
          aria-modal="true"
          aria-label={strings.chatJumpTo}
          onClick={() => setSwitcher(null)}
        >
          <div
            className={styles.switcher}
            onClick={(event) => event.stopPropagation()}
          >
            <input
              className={styles.switcherInput}
              value={switcher}
              onChange={(event) => setSwitcher(event.target.value)}
              placeholder={strings.chatJumpTo}
              aria-label={strings.chatJumpTo}
              autoFocus
              onKeyDown={(event) => {
                if (event.key === "Escape") setSwitcher(null);
                if (event.key === "Enter") {
                  const first = switcherHits[0];
                  if (first !== undefined) {
                    setOpenId(first.id);
                    setSwitcher(null);
                  }
                }
              }}
            />
            <ul className={styles.switcherList}>
              {switcherHits.length === 0 ? (
                <li className={styles.switcherNone}>{strings.chatNoRoom}</li>
              ) : (
                switcherHits.map((room, i) => (
                  <li key={room.id}>
                    <button
                      type="button"
                      className={
                        i === 0 ? styles.switcherFirst : styles.switcherItem
                      }
                      onClick={() => {
                        setOpenId(room.id);
                        setSwitcher(null);
                      }}
                    >
                      {room.kind === "dm" ? (
                        <Users size={15} className={styles.channelIcon} />
                      ) : (
                        <Hash size={15} className={styles.channelIcon} />
                      )}
                      {channelLabel(room)}
                    </button>
                  </li>
                ))
              )}
            </ul>
          </div>
        </div>
      )}

      {showingPeople && openId !== null && (
        <RoomPeople
          channel={openId}
          onClose={() => setShowingPeople(false)}
          onChanged={() => {
            // The room's cast changed: reload the feed (an agent's arrival is
            // narrated nowhere yet) and the '@' list, which is per room.
            void loadMessages(openId);
            void loadChannels();
          }}
        />
      )}

      {picking && (
        <FilePicker
          max={ATTACHMENTS_MAX}
          onClose={() => setPicking(false)}
          onPick={(files) => {
            setPicking(false);
            // Merge rather than replace, so choosing twice adds rather than
            // silently discarding the first pick.
            setStaged((held) => {
              const merged = [...held];
              for (const file of files) {
                if (
                  !merged.some((f) => f.id === file.id) &&
                  merged.length < ATTACHMENTS_MAX
                ) {
                  merged.push(file);
                }
              }
              return merged;
            });
          }}
        />
      )}

      {threadRoot !== null && open !== null && (
        <aside className={styles.thread}>
          <header className={styles.threadHeader}>
            <h3 className={styles.roomName}>{strings.chatThread}</h3>
            <button
              type="button"
              className={styles.threadClose}
              onClick={() => setThreadSeq(null)}
              aria-label={strings.chatThreadClose}
            >
              <X size={16} />
            </button>
          </header>

          <div className={styles.threadFeed}>
            {/* The root is shown first, so a thread is readable on its own
                without hunting for what it is about. */}
            <MessageLine
              message={threadRoot}
              palette={open.archivedAt === null ? palette : []}
              me={me}
              onReact={(emoji) => void react(threadRoot.id, emoji)}
              onOpenFile={(file) => void openFile(file)}
              onDecide={(p, ok) => void decide(p, ok)}
              onEdit={(m, body) => void editMessage(m, body)}
              onWithdraw={(m) => void withdrawMessage(m)}
            />
            <hr className={styles.threadRule} />
            {replies === null ? (
              <p className={styles.feedNote}>
                <Loader2 className={styles.spin} size={14} />{" "}
                {strings.chatLoading}
              </p>
            ) : replies.length === 0 ? (
              <p className={styles.feedNote}>{strings.chatThreadEmpty}</p>
            ) : (
              replies.map((reply, i) => (
                <MessageLine
                  key={reply.id}
                  message={reply}
                  grouped={continues(reply, replies[i - 1])}
                  palette={open.archivedAt === null ? palette : []}
                  me={me}
                  onReact={(emoji) => void react(reply.id, emoji)}
                  onOpenFile={(file) => void openFile(file)}
                  onDecide={(p, ok) => void decide(p, ok)}
                  onEdit={(m, body) => void editMessage(m, body)}
                  onWithdraw={(m) => void withdrawMessage(m)}
                />
              ))
            )}
          </div>

          {open.archivedAt === null && (
            <form
              className={styles.composer}
              onSubmit={(event) => {
                event.preventDefault();
                void sendReply();
              }}
            >
              <input
                className={styles.input}
                value={replyDraft}
                onChange={(event) => setReplyDraft(event.target.value)}
                placeholder={strings.chatThreadPlaceholder}
                aria-label={strings.chatThreadPlaceholder}
                autoComplete="off"
              />
              <Button
                type="submit"
                variant="primary"
                disabled={replyDraft.trim() === "" || replying}
              >
                <Send size={15} />
                {strings.chatSend}
              </Button>
            </form>
          )}
        </aside>
      )}
    </div>
  );
}

export { ChatError };
