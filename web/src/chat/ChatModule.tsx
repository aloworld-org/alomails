// alo Chat (ADR 0038) â€” rooms on the left, the conversation on the right.
//
// Domain references (UX law 2): Slack and WhatsApp for reflexes â€” a room list
// with unread counts, a scrolling feed, a composer that sends on Enter â€” and
// Sila for the calm of it. Everything the core task needs is on the surface:
// no menu is required to start a room, open one, or say something (prime law).
//
// Live by the push stream the workspace already keeps open (ADR 0038): a chat
// signal refetches the sidebar, and the open room's newest messages. Sending is
// optimistic â€” the line appears at once and is reconciled by the refetch â€” so a
// click is never answered by silence (law 6).
import type { ReactNode } from "react";
import { Fragment, Suspense, lazy, useCallback, useEffect, useRef, useState } from "react";
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
import {
  candidatesFor,
  channelLabel,
  continues,
  dayOf,
  mentionAt,
  personName,
  shortTime,
  standingOf,
  timeOf,
  withHandlesMarked,
} from "./presentation";
import type { Nameable } from "./presentation";
import { renderBody } from "./richText";
import { MessageLine } from "./MessageLine";
import { ChatSidebar } from "./ChatSidebar";
import styles from "./ChatModule.module.css";

const AuthoringInsertModal = lazy(() =>
  import("../authoring").then((module) => ({
    default: module.AuthoringInsertModal,
  })),
);

/** The ceiling the server enforces (`ATTACHMENTS_MAX` in the store). Kept in
 *  step by hand: exceeding it is refused server-side either way, so the worst
 *  a drifted copy does is offer a choice that is then declined. */
const ATTACHMENTS_MAX = 10;

/** What one page of history holds â€” the server's own default
 *  (`MESSAGE_PAGE_DEFAULT`). A full page means there is probably more behind
 *  it; a short one means we have reached the beginning. */
const PAGE = 50;

function chatAuthoringText(html: string): string {
  const document = new DOMParser().parseFromString(html, "text/html");
  const equation = document.querySelector<HTMLElement>("[data-alo-latex]");
  if (equation !== null) return `$${equation.dataset.aloLatex ?? ""}$`;
  const code = document.querySelector<HTMLElement>("[data-alo-lang]");
  if (code !== null) {
    return `\`\`\`${code.dataset.aloLang ?? ""}\n${code.textContent ?? ""}\n\`\`\``;
  }
  return document.body.textContent ?? "";
}


/**
 * One line of conversation, used by both the feed and the thread panel â€” the
 * two must never drift into showing a message differently. `children` is what
 * hangs under it (the thread affordance in the feed, nothing in a thread).
 */
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
  const [replyContext, setReplyContext] = useState<{
    message: Message;
    private: boolean;
  } | null>(null);
  // What may be left, per the server. Empty until it answers, which simply
  // means no picker yet â€” never a picker offering emoji it would refuse.
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
  const [hasSelection, setHasSelection] = useState(false);
  const [showingPeople, setShowingPeople] = useState(false);
  // One composer popover at a time: opening either closes the other, so two
  // menus can never sit open over each other.
  const [composerMenu, setComposerMenu] = useState<"share" | "emoji" | null>(
    null,
  );
  const composerMenuRef = useRef<HTMLDivElement | null>(null);
  const [authoringInsert, setAuthoringInsert] = useState<{
    kind: "equation" | "code";
    target: "message";
  } | null>(null);
  // Which room's row menu is open, by id â€” one at a time, same reason.
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
  // it. Best-effort â€” a composer that cannot suggest still sends.
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

  // Live: a chat signal refreshes the sidebar, the open room, and the open
  // thread â€” a reply arriving while someone reads the thread is exactly the
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
  }, [client, openId, loadChannels, loadMessages]);

  // Keep the newest line in view as the conversation grows â€” but only when
  // the newest line actually changed. Scrolling to the bottom after prepending
  // older history would throw the reader out of what they went back to read.
  const newestSeq = messages?.[messages.length - 1]?.seq ?? null;
  useEffect(() => {
    const feed = feedRef.current;
    if (feed !== null) feed.scrollTop = feed.scrollHeight;
  }, [newestSeq, openId]);

  async function send() {
    const words = draft.trim();
    const body = replyContext === null
      ? words
      : `> ${personName(replyContext.message.authorEmail, replyContext.message.author)}: ${replyContext.message.body.replace(/\n/g, "\n> ")}\n\n${words}`;
    if (body === "" || openId === null || sending) return;
    const files = staged;
    setSending(true);
    setError(null);
    setDraft("");
    setReplyContext(null);
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

  /** Put `text` where the caret is and keep typing there â€” used by the share
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
    // Select the sample so the next keystroke replaces it â€” the mark is done,
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
   *  under the cursor jumps â€” the single thing that makes an infinite feed
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
      // press again â€” the server settles it, not a check here.
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
    // Confirmed, because it changes the room for everyone in it â€” and said in
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
      // that hides the rooms you are already in reads as empty and broken â€”
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
        setMessages((held) => {
          const current = held ?? [];
          const known = new Set(current.map((message) => message.id));
          const earlier = [...page].reverse().filter((message) => !known.has(message.id));
          return [...earlier, ...current];
        });
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
  }

  async function withdrawMessage(message: Message) {
    setError(null);
    try {
      await api.withdrawMessage(message.id);
    } catch (failure) {
      setError(chatMessage(failure, strings.chatWithdrawFailed));
    }
    if (openId !== null) void loadMessages(openId);
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
      // Apply where it is shown â€” a message can be on screen in the feed, in
      // the thread panel, or in both at once.
      const applied = <T extends Message>(list: T[] | null): T[] | null =>
        list?.map((m) =>
          m.id === messageId ? { ...m, reactions: tally } : m,
        ) ?? null;
      setMessages(applied);
    } catch (failure) {
      setError(chatMessage(failure, strings.chatReactFailed));
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
  // Channels, then people, then what is archived â€” the order a person looks
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
  // Every room, filtered by what has been typed â€” including archived ones,
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
  return (
    <div className={styles.module}>
      {(!isMobile || openId === null) && (
        <ChatSidebar
          channels={channels}
          openId={openId}
          creating={creating}
          finding={finding}
          found={found}
          dmQuery={dmQuery}
          dmFound={dmFound}
          browsing={browsing}
          rowMenu={rowMenu}
          onCreateChannel={() => void createChannel()}
          onStartDm={() => setDmQuery("")}
          onBrowse={() => void browse()}
          onFind={(query) => void find(query)}
          onFindPeople={(query) => void findPeople(query)}
          onCloseDm={() => { setDmQuery(null); setDmFound([]); }}
          onOpenDm={(person) => void openDm(person)}
          onCloseBrowse={() => setBrowsing(null)}
          onJoinRoom={(room) => void joinRoom(room)}
          onOpen={(id) => { setOpenId(id); setBrowsing(null); }}
          onRowMenu={setRowMenu}
          onRename={(room) => void renameRoom(room)}
          onArchive={(room) => void archiveRoom(room)}
        />
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
                    {/* Only the person who asked may stop it â€” the same rule
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
                      onReplyHere={
                        open.archivedAt === null
                          ? (m) => {
                              setReplyContext({ message: m, private: false });
                              composerRef.current?.focus();
                            }
                          : undefined
                      }
                      onReplyPrivate={
                        open.kind !== "dm" &&
                        message.authorKind === "user" &&
                        message.author !== me
                          ? (m) => {
                              void openDm({
                                user: m.author,
                                email: m.authorEmail ?? m.author,
                              }).then(() => {
                                setReplyContext({ message: m, private: true });
                                composerRef.current?.focus();
                              });
                            }
                          : undefined
                      }
                    >
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
                {replyContext !== null && (
                  <div className={styles.replyContext}>
                    <span className={styles.replyContextIcon} aria-hidden="true">
                      <Reply size={15} />
                    </span>
                    <span className={styles.replyContextCopy}>
                      <strong>
                        {replyContext.private
                          ? strings.chatReplyingPrivately(
                              personName(
                                replyContext.message.authorEmail,
                                replyContext.message.author,
                              ),
                            )
                          : strings.chatReplyingHere}
                      </strong>
                      <span>{replyContext.message.body}</span>
                    </span>
                    <button
                      type="button"
                      className={styles.searchClear}
                      onClick={() => setReplyContext(null)}
                      aria-label={strings.chatCancelReply}
                    >
                      <X size={15} />
                    </button>
                  </div>
                )}
                {hasSelection && <div className={styles.formatBar} role="toolbar" aria-label={strings.chatFormatting}>
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
                </div>}
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
                            setAuthoringInsert({ kind: "code", target: "message" });
                          }}
                        >
                          <SquareCode size={15} className={styles.shareIcon} />
                          <span>
                            <span className={styles.shareName}>{strings.chatCodeBlock}</span>
                            <span className={styles.shareHint}>{strings.chatCodeBlockHint}</span>
                          </span>
                        </button>
                        <button
                          type="button"
                          role="menuitem"
                          className={styles.shareItem}
                          onClick={() => {
                            setComposerMenu(null);
                            setAuthoringInsert({ kind: "equation", target: "message" });
                          }}
                        >
                          <Sigma size={15} className={styles.shareIcon} />
                          <span>
                            <span className={styles.shareName}>{strings.chatFormula}</span>
                            <span className={styles.shareHint}>{strings.chatFormulaHint}</span>
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
                    setHasSelection(event.target.selectionStart !== event.target.selectionEnd);
                    setHighlighted(0);
                  }}
                  onSelect={(event) => {
                    setCaret(event.currentTarget.selectionStart ?? 0);
                    setHasSelection(event.currentTarget.selectionStart !== event.currentTarget.selectionEnd);
                  }}
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

      {authoringInsert !== null && (
        <Suspense fallback={null}>
          <AuthoringInsertModal
            kind={authoringInsert.kind}
            onClose={() => setAuthoringInsert(null)}
            onInsert={(html) => {
              const text = chatAuthoringText(html);
              insertAtCaret(text);
              setAuthoringInsert(null);
            }}
          />
        </Suspense>
      )}
    </div>
  );
}

export { ChatError };
