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
import { useCallback, useEffect, useRef, useState } from "react";
import {
  Archive,
  Hash,
  Loader2,
  Lock,
  MessageSquarePlus,
  MessagesSquare,
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
  X,
} from "lucide-react";

import { strings } from "../i18n";
import { useAuth } from "../auth";
import { FilePicker, fileSize, saveBlob } from "../drive";
import { AgentActionCard } from "../shell/AgentActionCard";
import { RoomPeople } from "./RoomPeople";
import { useJmapClient } from "../jmap";
import { Avatar, Button, useDialogs } from "../ds";
import { ChatError, chatMessage, useChatApi } from "./api";
import type { DriveNodeDto } from "../jmap/types";
import type {
  Attachment,
  Channel,
  ChannelSummary,
  FeedMessage,
  Person,
  Message,
  Proposal,
} from "./types";
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
  /** This message continues the previous author's run: no avatar, no name, no
   *  timestamp — just the words, aligned under the ones above. */
  grouped?: boolean;
  children?: ReactNode;
}) {
  const namesMe = me !== null && message.mentions.includes(me);
  const [picking, setPicking] = useState(false);
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
            <span className={styles.pickerWrap}>
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
        <span className={styles.gutterTime}>{timeOf(message.createdAt)}</span>
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
          <input
            className={styles.editInput}
            value={editing}
            onChange={(event) => setEditing(event.target.value)}
            aria-label={strings.chatEditLabel}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.preventDefault();
                setEditing(null);
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
            ? withHandlesMarked(message.body)
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
  const composerRef = useRef<HTMLInputElement | null>(null);
  const [caret, setCaret] = useState(0);
  const [showingPeople, setShowingPeople] = useState(false);
  const [sharing, setSharing] = useState(false);
  const [emoji, setEmoji] = useState(false);
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
    void loadMessages(openId);
  }, [openId, loadMessages]);

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

  async function browse() {
    setError(null);
    try {
      setBrowsing(await api.joinable());
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

  const open = channels?.find((c) => c.id === openId) ?? null;
  // Derived, not stored: the list is a function of what is typed and where
  // the caret is, so it can never disagree with the composer.
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
                      <span className={styles.channelName}>{person.email}</span>
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
              <p className={styles.sidebarNote}>{strings.chatNothingToJoin}</p>
            ) : (
              <ul className={styles.channelList}>
                {browsing.map((room) => (
                  <li key={room.id}>
                    <button
                      type="button"
                      className={styles.channel}
                      onClick={() => void joinRoom(room)}
                    >
                      <Hash size={15} className={styles.channelIcon} />
                      <span className={styles.channelName}>{room.name}</span>
                      <span className={styles.joinHint}>
                        {strings.chatJoin}
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
            <Loader2 className={styles.spin} size={14} /> {strings.chatLoading}
          </p>
        ) : channels.length === 0 ? (
          // Law 5: an empty room list teaches the one next step.
          <div className={styles.emptySidebar}>
            <p className={styles.emptyLead}>{strings.chatNoChannelsLead}</p>
            <p className={styles.emptyHint}>{strings.chatNoChannelsHint}</p>
          </div>
        ) : (
          <ul className={styles.channelList}>
            {channels.map((channel) => (
              <li key={channel.id}>
                <button
                  type="button"
                  className={[
                    channel.id === openId ? styles.channelOpen : styles.channel,
                    // An archived room stays reachable (its history is still
                    // the team's), but it must not read as a live one.
                    channel.archivedAt !== null ? styles.channelArchived : "",
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
                      <span className={styles.badge}>{channel.unread}</span>
                    )
                  )}
                </button>
              </li>
            ))}
          </ul>
        )}
      </aside>

      <section className={styles.room}>
        {open === null ? (
          <div className={styles.emptyRoom}>
            <Hash size={28} className={styles.emptyRoomIcon} />
            <p className={styles.emptyLead}>{strings.chatNoRoomOpenLead}</p>
            <p className={styles.emptyHint}>{strings.chatNoRoomOpenHint}</p>
          </div>
        ) : (
          <>
            <header className={styles.roomHeader}>
              <div className={styles.roomTitle}>
                <h3 className={styles.roomName}>{channelLabel(open)}</h3>
                {open.topic !== null && (
                  <p className={styles.roomTopic}>{open.topic}</p>
                )}
              </div>
              <div className={styles.roomActions}>
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

            <div className={styles.feed} ref={feedRef}>
              {/* An explicit control rather than a scroll trigger: a feed that
                  loads on approach fires while someone is simply reading back,
                  and there is no way to tell it to stop. */}
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
                  <MessageLine
                    key={message.id}
                    message={message}
                    grouped={continues(message, messages[i - 1])}
                    palette={open.archivedAt === null ? palette : []}
                    me={me}
                    onReact={(emoji) => void react(message.id, emoji)}
                    onOpenFile={(file) => void openFile(file)}
                    onDecide={(p, ok) => void decide(p, ok)}
                    onEdit={(m, body) => void editMessage(m, body)}
                    onWithdraw={(m) => void withdrawMessage(m)}
                  >
                    {message.replyCount > 0 ? (
                      <button
                        type="button"
                        className={styles.threadLink}
                        onClick={() => setThreadSeq(message.seq)}
                      >
                        <MessagesSquare size={13} />
                        {strings.chatReplies(message.replyCount)}
                      </button>
                    ) : (
                      // Offered only when the room can still take words —
                      // an archived room must not invite a reply it will
                      // refuse.
                      open.archivedAt === null && (
                        <button
                          type="button"
                          className={styles.replyLink}
                          onClick={() => setThreadSeq(message.seq)}
                        >
                          <Reply size={13} />
                          {strings.chatReplyInThread}
                        </button>
                      )
                    )}
                  </MessageLine>
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
                <span className={styles.shareWrap}>
                  <button
                    type="button"
                    className={styles.composerTool}
                    onClick={() => setSharing((open) => !open)}
                    aria-label={strings.chatShare}
                    title={strings.chatShare}
                    aria-expanded={sharing}
                  >
                    <Plus size={18} />
                  </button>
                  {sharing && (
                    <div className={styles.shareMenu} role="menu">
                      <button
                        type="button"
                        role="menuitem"
                        className={styles.shareItem}
                        onClick={() => {
                          setSharing(false);
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
                          setSharing(false);
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
                          setSharing(false);
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
                    onClick={() => setEmoji((open) => !open)}
                    aria-label={strings.chatInsertEmoji}
                    title={strings.chatInsertEmoji}
                    aria-expanded={emoji}
                  >
                    <Smile size={18} />
                  </button>
                  {emoji && palette.length > 0 && (
                    <div className={styles.emojiMenu} role="menu">
                      {palette.map((glyph) => (
                        <button
                          key={glyph}
                          type="button"
                          role="menuitem"
                          className={styles.pickerOption}
                          onClick={() => {
                            setEmoji(false);
                            insertAtCaret(glyph);
                          }}
                        >
                          {glyph}
                        </button>
                      ))}
                    </div>
                  )}
                </span>
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
                <input
                  ref={composerRef}
                  className={styles.input}
                  value={draft}
                  onChange={(event) => {
                    setDraft(event.target.value);
                    setCaret(event.target.selectionStart ?? 0);
                    setHighlighted(0);
                  }}
                  onSelect={(event) =>
                    setCaret(event.currentTarget.selectionStart ?? 0)
                  }
                  onKeyDown={(event) => {
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
