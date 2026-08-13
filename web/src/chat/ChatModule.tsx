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
import { Suspense, lazy, useCallback, useEffect, useRef, useState } from "react";
import {
  Archive,
  Hash,
  Paperclip,
} from "lucide-react";

import { strings } from "../i18n";
import { useAuth } from "../auth";
import { saveBlob } from "../drive/parts";
import { RoomPeople } from "./RoomPeople";
import { useJmapClient } from "../jmap/useJmapClient";
import { useDismiss, useIsMobile } from "../ds";
import { ChatError, chatMessage, useChatApi } from "./api";
import type { Attachment, Message } from "./types";
import { useMeetApi } from "../meet/api";
import type { Meeting } from "../meet/api";
import { ChatSwitcher } from "./ChatSwitcher";
import { ConversationHeader } from "./ConversationHeader";
import { ActiveTurns } from "./ActiveTurns";
import { MessageFeed } from "./MessageFeed";
import { ChatComposer } from "./ChatComposer";
import { useRoomDirectory } from "./useRoomDirectory";
import { useChatFeed } from "./useChatFeed";
import { CHAT_ATTACHMENTS_MAX, useChatAttachments } from "./useChatAttachments";
import { chatAuthoringText } from "./authoringText";
import {
  candidatesFor,
  channelLabel,
  mentionAt,
  personName,
} from "./presentation";
import type { Nameable } from "./presentation";
import { ChatSidebar } from "./ChatSidebar";

const AuthoringInsertModal = lazy(() =>
  import("../authoring").then((module) => ({
    default: module.AuthoringInsertModal,
  })),
);
const MeetRoom = lazy(() =>
  import("../meet/MeetRoom").then((module) => ({ default: module.MeetRoom })),
);
const FilePicker = lazy(() =>
  import("../drive/FilePicker").then((module) => ({ default: module.FilePicker })),
);

export function ChatModule() {
  const api = useChatApi();
  const client = useJmapClient();
  // The reader's own id, for marking the messages addressed to them.
  const { identity } = useAuth();
  const me = identity?.sub ?? null;
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [replyContext, setReplyContext] = useState<{
    message: Message;
    private: boolean;
  } | null>(null);
  // What may be left, per the server. Empty until it answers, which simply
  // means no picker yet â€” never a picker offering emoji it would refuse.
  // Files chosen but not yet sent. Held as Drive nodes so the composer can
  // show their names without a second lookup.
  // Who can be named here: the room's people and its agents, in one list,
  // because the person typing does not care which kind they are reaching for.
  const [highlighted, setHighlighted] = useState(0);
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
  // What was half-typed in each room. Switching rooms to check something and
  // losing a sentence is a small betrayal every chat app learned to avoid.
  const drafts = useRef<Map<string, string>>(new Map());
  const [switcher, setSwitcher] = useState<string | null>(null);
  // Where reading stopped when this room was opened. Held still afterwards:
  // the line must not creep down as new messages land while you are looking.
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
  const directory = useRoomDirectory(setError);
  const { channels, openId, setOpenId, creating, browsing, setBrowsing, dmQuery, setDmQuery, dmFound, setDmFound, finding, found, loadChannels, find, findPeople, openDm, renameRoom, archiveRoom, browse, joinRoom, createChannel } = directory;
  const { feedRef, messages, setMessages, turns, palette, nameable, readUpTo, moreBehind, loadingOlder, loadMessages, loadTurns, loadOlder, editMessage, withdrawMessage, decide, react } = useChatFeed(openId, channels, loadChannels, setError);
  const { staged, setStaged, picking, setPicking, dropping, setDropping, shareDropped, mergePicked } = useChatAttachments(setError);

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
  // Derived, not stored: the list is a function of what is typed and where
  // the caret is, so it can never disagree with the composer.
  // Null while nothing is being searched for: the picker shows its groups.
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
    <div className="flex h-full min-h-0 overflow-hidden bg-app text-primary">
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
        className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-surface"
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
          <div className="absolute inset-3 z-40 flex items-center justify-center rounded-xl border-2 border-dashed border-accent bg--tint">
            <span className="flex items-center gap-2 rounded-full bg-surface px-4 py-3 text-sm font-semibold text-primary shadow-md">
              <Paperclip size={16} />
              {strings.chatDropFiles}
            </span>
          </div>
        )}
        {open === null ? (
          <div className="flex flex-1 flex-col items-center justify-center px-6 text-center">
            <span className="mb-3 flex size-14 items-center justify-center rounded-xl bg--tint text-accent"><Hash size={28} /></span>
            <p className="m-0 text-lg font-semibold text-primary">{strings.chatNoRoomOpenLead}</p>
            <p className="mt-2 max-w-md text-sm text-tertiary">{strings.chatNoRoomOpenHint}</p>
          </div>
        ) : (
          <>
            <ConversationHeader room={open} mobile={isMobile} liveMeeting={liveMeeting} onBack={() => setOpenId(null)} onMeet={() => {
              if (liveMeeting !== null) { setInMeeting(liveMeeting.id); return; }
              void meet.start({ channel: open.id, title: channelLabel(open) }).then((meeting) => { setLiveMeeting(meeting); setInMeeting(meeting.id); }).catch(() => setError(strings.meetJoinFailed));
            }} onPeople={() => setShowingPeople(true)} onRename={() => void renameRoom(open)} onArchive={() => void archiveRoom(open)} />

            <ActiveTurns turns={turns} onStop={(turn) => { if (openId !== null) void api.stopTurn(openId, turn.id).then(() => loadTurns(openId)); }} />

            <MessageFeed room={open} messages={messages} feedRef={feedRef} moreBehind={moreBehind} loadingOlder={loadingOlder} readUpTo={readUpTo} palette={palette} me={me} onOlder={() => void loadOlder()} onReact={(message, emoji) => void react(message.id, emoji)} onOpenFile={(file) => void openFile(file)} onDecide={(proposal, approve) => void decide(proposal, approve)} onEdit={(message, body) => void editMessage(message, body)} onWithdraw={(message) => void withdrawMessage(message)} onReplyHere={(message) => { setReplyContext({ message, private: false }); composerRef.current?.focus(); }} onReplyPrivate={(message) => { void openDm({ user: message.author, email: message.authorEmail ?? message.author }).then(() => { setReplyContext({ message, private: true }); composerRef.current?.focus(); }); }} />

            {error !== null && <p className="mx-auto mb-2 w-full max-w-4xl rounded-md bg--tint px-3 py-2 text-sm text-primary" role="alert">{error}</p>}

            {open.archivedAt !== null ? (
              // The server refuses new words in an archived room, so the
              // composer must not be offered at all: a control that looks
              // usable and answers with an error is worse than its absence.
              <p className="mx-auto mb-4 flex w-full max-w-4xl items-center justify-center gap-2 rounded-md border border-subtle bg-raised px-3 py-2 text-sm text-tertiary">
                <Archive size={14} /> {strings.chatArchivedNote}
              </p>
            ) : (
              <ChatComposer room={open} composerRef={composerRef} menuRef={composerMenuRef} draft={draft} sending={sending} reply={replyContext} staged={staged} selected={hasSelection} menu={composerMenu} palette={palette} emojiQuery={emojiQuery} suggestions={suggestions} highlighted={highlighted} onSubmit={() => void send()} onDraft={(value, at, selected) => { setDraft(value); if (openId !== null) drafts.current.set(openId, value); setCaret(at); setHasSelection(selected); setHighlighted(0); }} onSelect={(at, selected) => { setCaret(at); setHasSelection(selected); }} onCaretToEnd={() => setCaret(draft.length)} onHighlighted={setHighlighted} onComplete={complete} onCancelReply={() => setReplyContext(null)} onUnstage={(id) => setStaged((held) => held.filter((file) => file.id !== id))} onMenu={setComposerMenu} onPickFile={() => { setComposerMenu(null); setPicking(true); }} onAuthor={(kind) => { setComposerMenu(null); setAuthoringInsert({ kind, target: "message" }); }} onInsert={(text) => { setComposerMenu(null); insertAtCaret(text); }} onEmojiQuery={setEmojiQuery} onWrap={wrapSelection} />
            )}
          </>
        )}
      </section>

      {inMeeting !== null && (
        <Suspense fallback={<div className="fixed inset-0 z-50 flex items-center justify-center bg-overlay text-sm text-on-accent">{strings.chatLoading}</div>}><MeetRoom
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
        /></Suspense>
      )}

      {switcher !== null && (
        <ChatSwitcher query={switcher} rooms={switcherHits} onQuery={setSwitcher} onClose={() => setSwitcher(null)} onChoose={(id) => { setOpenId(id); setSwitcher(null); }} />
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
        <Suspense fallback={<div className="fixed inset-0 z-50 flex items-center justify-center bg-overlay text-sm text-on-accent">{strings.chatLoading}</div>}><FilePicker
          max={CHAT_ATTACHMENTS_MAX}
          onClose={() => setPicking(false)}
          onPick={mergePicked}
        /></Suspense>
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
