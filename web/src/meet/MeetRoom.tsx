// A meeting on screen.
//
// The media is LiveKit's — its components SDK runs under our UI, which is
// exactly the arrangement the product doctrine describes for an integrated
// engine. What is ours is everything around it: who may be here (decided
// before a token exists), what the meeting belongs to, and what happens when
// there is no engine to talk to.
//
// The `LiveKitRoom` element wants a URL and a token. It never learns which
// workspace this is, and it cannot: the token carries an opaque room name and
// a user id, and this component has nothing else to give it.
import { useEffect, useRef, useState } from "react";
import "@livekit/components-styles";
import {
  LiveKitRoom,
  PreJoin,
  VideoConference,
  formatChatMessageLinks,
  useChat,
  useConnectionState,
  useDataChannel,
  useLocalParticipant,
  useParticipants,
  useRoomContext,
  useTranscriptions,
} from "@livekit/components-react";
import type { LocalUserChoices } from "@livekit/components-react";
import { ConnectionState, Track } from "livekit-client";
import { ArrowLeft, BarChart3, Captions, Check, ChevronDown, Circle, ClipboardList, Copy, FileText, FolderOpen, Hand, Lock, Maximize2, MessageSquare, Mic, MicOff, MonitorUp, NotebookPen, Paperclip, PhoneOff, PictureInPicture2, Plus, RefreshCw, Send, ServerOff, Settings, Share2, ShieldCheck, Smile, Sparkles, Square, Trash2, UserMinus, Users, Video, X } from "lucide-react";

import wavingHand from "../assets/alo-waving-hand.svg";
import { useAuth } from "../auth/AuthProvider";
import { Button } from "../ds";
import { strings } from "../i18n";
import { MeetApiError, MeetUnavailable, useMeetApi } from "./api";
import type { JoinGrant, MeetApi, MeetingAttachment, MeetingMessage, MeetingRecording, MeetingTranscriptSegment, MeetingWorkspace } from "./api";
import styles from "./MeetRoom.module.css";

function useMeetingDuration(startedAt: string | null): string {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, []);
  if (startedAt === null) return "00:00";
  const seconds = Math.max(0, Math.floor((now - new Date(startedAt).getTime()) / 1_000));
  const hours = Math.floor(seconds / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  const rest = seconds % 60;
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`
    : `${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`;
}

function MeetingHeader({ grant }: { grant: JoinGrant }) {
  const duration = useMeetingDuration(grant.meeting.startedAt);
  const rawTitle = grant.meeting.title.trim();
  const title = rawTitle === "" || /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(rawTitle)
    ? strings.meetUntitled
    : rawTitle;
  return (
    <div className={styles.roomHeader}>
      <span className={styles.roomBrand}><Video aria-hidden="true" /></span>
      <div className={styles.roomIdentity}>
        <strong>{title}</strong>
        <span>
          <i />
          {strings.meetLive}
          <b aria-hidden="true">·</b>
          <time>{duration}</time>
        </span>
      </div>
    </div>
  );
}

function PresentingNotice() {
  const { isScreenShareEnabled } = useLocalParticipant();
  if (!isScreenShareEnabled) return null;
  return (
    <div className={styles.presenting} role="status">
      <span><MonitorUp aria-hidden="true" /></span>
      <strong>{strings.meetPresentingTitle}</strong>
      <p>{strings.meetPresentingBody}</p>
    </div>
  );
}

function ChatWelcome() {
  const { chatMessages } = useChat();
  if (chatMessages.length > 0) return null;
  return (
    <div className={styles.chatWelcome} aria-hidden="true">
      <span><Smile aria-hidden="true" /></span>
      <strong>{strings.meetChatEmptyTitle}</strong>
      <p>{strings.meetChatEmptyBody}</p>
    </div>
  );
}

type ChatReactionSignal = { kind: "chat-reaction"; messageId: string; emoji: string; actor: string };

function ChatAttachment({ file }: { file: File }) {
  const [url, setUrl] = useState("");
  useEffect(() => {
    const next = URL.createObjectURL(file);
    setUrl(next);
    return () => URL.revokeObjectURL(next);
  }, [file]);
  if (url === "") return null;
  const isImage = file.type.startsWith("image/");
  return <a className={styles.chatAttachment} href={url} download={file.name}>
    {isImage ? <img src={url} alt={file.name} /> : <span><FileText aria-hidden="true" /><b>{file.name}</b><small>{Math.ceil(file.size / 1024)} KB</small></span>}
  </a>;
}

function StoredChatAttachment({ api, attachment }: { api: MeetApi; attachment: MeetingAttachment }) {
  const [url, setUrl] = useState("");
  useEffect(() => { let current = true; let objectUrl = ""; void api.downloadAttachment(attachment).then((blob) => { if (!current) return; objectUrl = URL.createObjectURL(blob); setUrl(objectUrl); }); return () => { current = false; if (objectUrl !== "") URL.revokeObjectURL(objectUrl); }; }, [api, attachment]);
  if (url === "") return <span className={styles.chatAttachmentLoading}>{attachment.name}</span>;
  return <a className={styles.chatAttachment} href={url} download={attachment.name}>{attachment.contentType.startsWith("image/") ? <img src={url} alt={attachment.name} /> : <span><FileText aria-hidden="true" /><b>{attachment.name}</b><small>{Math.ceil(attachment.size / 1024)} KB</small></span>}</a>;
}

type ToolTab = "agenda" | "polls" | "notes" | "files";

function MeetingTools({ meetingId, host, tab }: { meetingId: string; host: boolean; tab: ToolTab }) {
  const api = useMeetApi();
  const { localParticipant } = useLocalParticipant();
  const [workspace, setWorkspace] = useState<MeetingWorkspace | null>(null);
  const [messages, setMessages] = useState<MeetingMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [question, setQuestion] = useState("");
  const [optionA, setOptionA] = useState("");
  const [optionB, setOptionB] = useState("");
  const [error, setError] = useState(false);
  const notesDirty = useRef(false);
  useEffect(() => {
    let current = true;
    const refresh = () => void Promise.all([api.meetingWorkspace(meetingId), api.messages(meetingId)]).then(([next, history]) => { if (current) { if (!notesDirty.current) setWorkspace(next); setMessages(history); } }).catch(() => setError(true));
    refresh(); const timer = window.setInterval(refresh, 4_000);
    return () => { current = false; window.clearInterval(timer); };
  }, [api, meetingId]);
  const save = async (state: MeetingWorkspace["state"]) => {
    if (workspace === null) return;
    setError(false);
    try { setWorkspace(await api.saveMeetingWorkspace(meetingId, { ...workspace, state })); notesDirty.current = false; } catch { setError(true); }
  };
  if (workspace === null) return <div className={styles.toolLoading}>{strings.meetToolLoading}</div>;
  if (tab === "files") {
    const files = messages.flatMap((message) => message.attachments.map((attachment) => ({ attachment, sender: message.sender, createdAt: message.createdAt })));
    return <section className={styles.meetingToolPanel}><header><FolderOpen /><div><h3>{strings.meetFiles}</h3><p>{strings.meetFilesHint}</p></div></header>{files.length === 0 ? <p className={styles.toolEmpty}>{strings.meetNoFiles}</p> : <ul className={styles.toolFiles}>{files.map(({ attachment, sender, createdAt }) => <li key={attachment.id}><StoredChatAttachment api={api} attachment={attachment} /><small>{sender} · {new Date(createdAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</small></li>)}</ul>}</section>;
  }
  if (tab === "agenda") return <section className={styles.meetingToolPanel}><header><ClipboardList /><div><h3>{strings.meetAgenda}</h3><p>{strings.meetAgendaHint}</p></div></header><ul className={styles.toolAgenda}>{workspace.state.agenda.map((item) => <li key={item.id}><button type="button" disabled={!host} className={item.done ? styles.agendaDone : undefined} onClick={() => void save({ ...workspace.state, agenda: workspace.state.agenda.map((current) => current.id === item.id ? { ...current, done: !current.done } : current) })}><span>{item.done && <Check />}</span>{item.text}</button>{host && <button type="button" aria-label={strings.deleteLabel} onClick={() => void save({ ...workspace.state, agenda: workspace.state.agenda.filter((current) => current.id !== item.id) })}><Trash2 /></button>}</li>)}</ul>{host && <form className={styles.toolAdd} onSubmit={(event) => { event.preventDefault(); const text = draft.trim(); if (text === "") return; void save({ ...workspace.state, agenda: [...workspace.state.agenda, { id: crypto.randomUUID(), text, done: false }] }).then(() => setDraft("")); }}><input value={draft} onChange={(event) => setDraft(event.target.value)} placeholder={strings.meetAgendaPlaceholder} /><button type="submit" disabled={draft.trim() === ""}><Plus />{strings.add}</button></form>}{error && <p role="alert">{strings.meetToolsFailed}</p>}</section>;
  if (tab === "polls") return <section className={styles.meetingToolPanel}><header><BarChart3 /><div><h3>{strings.meetPolls}</h3><p>{strings.meetPollsHint}</p></div></header><div className={styles.toolPolls}>{workspace.state.polls.map((poll) => <article key={poll.id}><strong>{poll.question}</strong>{poll.options.map((option, index) => { const votes = Object.values(poll.votes).filter((vote) => vote === index).length; const selected = poll.votes[localParticipant.identity] === index; return <button type="button" className={selected ? styles.pollSelected : undefined} key={option} onClick={() => void api.voteMeetingPoll(meetingId, poll.id, index).then(setWorkspace).catch(() => setError(true))}><span>{option}</span><small>{votes}</small></button>; })}</article>)}</div>{host && <form className={styles.pollCreate} onSubmit={(event) => { event.preventDefault(); if ([question, optionA, optionB].some((value) => value.trim() === "")) return; void save({ ...workspace.state, polls: [...workspace.state.polls, { id: crypto.randomUUID(), question: question.trim(), options: [optionA.trim(), optionB.trim()], votes: {} }] }).then(() => { setQuestion(""); setOptionA(""); setOptionB(""); }); }}><input value={question} onChange={(event) => setQuestion(event.target.value)} placeholder={strings.meetPollQuestion} /><input value={optionA} onChange={(event) => setOptionA(event.target.value)} placeholder={strings.meetPollOptionOne} /><input value={optionB} onChange={(event) => setOptionB(event.target.value)} placeholder={strings.meetPollOptionTwo} /><button type="submit"><Plus />{strings.meetCreatePoll}</button></form>}{error && <p role="alert">{strings.meetToolsFailed}</p>}</section>;
  return <section className={styles.meetingToolPanel}><header><NotebookPen /><div><h3>{strings.meetNotes}</h3><p>{strings.meetNotesHint}</p></div></header><textarea value={workspace.state.notes} readOnly={!host} onChange={(event) => { notesDirty.current = true; setWorkspace({ ...workspace, state: { ...workspace.state, notes: event.target.value } }); }} placeholder={strings.meetNotesPlaceholder} />{host && <button type="button" className={styles.toolSave} onClick={() => void save(workspace.state)}>{strings.save}</button>}{error && <p role="alert">{strings.meetToolsFailed}</p>}</section>;
}

function InCallChat({ meetingId, hostId, onClose }: { meetingId: string; hostId: string; onClose: () => void }) {
  const api = useMeetApi();
  const { chatMessages, send, isSending } = useChat();
  const participants = useParticipants();
  const { localParticipant } = useLocalParticipant();
  const [tab, setTab] = useState<"messages" | "people" | ToolTab>("messages");
  const [draft, setDraft] = useState("");
  const [recipient, setRecipient] = useState<string | null>(null);
  const [showRecipients, setShowRecipients] = useState(false);
  const [showEmoji, setShowEmoji] = useState(false);
  const [reactions, setReactions] = useState<Record<string, Record<string, string[]>>>({});
  const [history, setHistory] = useState<MeetingMessage[]>([]);
  const [sendError, setSendError] = useState(false);
  const [moderationError, setModerationError] = useState(false);
  const [moderating, setModerating] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const remoteParticipants = participants.filter((person) => !person.isLocal);
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  const { send: sendAction } = useDataChannel("alo-meet-chat-actions", (packet) => {
    try {
      const signal = JSON.parse(decoder.decode(packet.payload)) as ChatReactionSignal;
      if (signal.kind !== "chat-reaction") return;
      setReactions((current) => {
        const message = current[signal.messageId] ?? {};
        const actors = message[signal.emoji] ?? [];
        return { ...current, [signal.messageId]: { ...message, [signal.emoji]: Array.from(new Set([...actors, signal.actor])) } };
      });
    } catch { /* Ignore data from older clients on the same topic. */ }
  });
  const deliveryOptions = recipient === null ? {} : {
    destinationIdentities: [recipient],
    attributes: { "alo.private": "true", "alo.recipient": recipient },
  };
  useEffect(() => {
    let current = true;
    const refresh = () => void api.messages(meetingId).then((messages) => {
      if (!current) return;
      setHistory(messages);
      setReactions((existing) => {
        const next = { ...existing };
        for (const message of messages) {
          for (const reaction of message.reactions ?? []) {
            const byEmoji = next[message.id] ?? {};
            next[message.id] = { ...byEmoji, [reaction.emoji]: Array.from(new Set([...(byEmoji[reaction.emoji] ?? []), reaction.actor])) };
          }
        }
        return next;
      });
    });
    refresh();
    const timer = window.setInterval(refresh, 3_000);
    return () => { current = false; window.clearInterval(timer); };
  }, [api, meetingId]);
  const submit = async (text = draft) => {
    const message = text.trim();
    if (message === "" || isSending) return;
    setSendError(false);
    try {
      const stored = await api.postMessage(meetingId, message, recipient);
      setHistory((current) => [...current, stored]);
      await send(message, { ...deliveryOptions, attributes: { ...deliveryOptions.attributes, "alo.persistedId": stored.id } });
      setDraft("");
    } catch {
      setSendError(true);
    }
  };
  const sendFiles = async (files: FileList | null) => {
    if (files === null || files.length === 0 || isSending) return;
    const accepted = Array.from(files).filter((file) => file.type.startsWith("image/") || file.type === "application/pdf");
    if (accepted.length === 0) return;
    setSendError(false);
    try {
      const body = accepted.map((file) => file.name).join(", ");
      const stored = await api.postMessage(meetingId, body, recipient);
      stored.attachments = await Promise.all(accepted.map((file) => api.uploadAttachment(meetingId, stored.id, file)));
      setHistory((current) => [...current, stored]);
      await send(body, { ...deliveryOptions, attributes: { ...deliveryOptions.attributes, "alo.persistedId": stored.id }, attachments: accepted });
      if (fileInput.current !== null) fileInput.current.value = "";
    } catch { setSendError(true); }
  };
  const react = async (messageId: string, emoji: string, persistedId = messageId) => {
    const actor = localParticipant.name || localParticipant.identity;
    try {
      await api.react(meetingId, persistedId, emoji);
      setReactions((current) => {
        const keys = Array.from(new Set([messageId, persistedId]));
        return keys.reduce((next, key) => ({ ...next, [key]: { ...(next[key] ?? {}), [emoji]: Array.from(new Set([...(next[key]?.[emoji] ?? []), actor])) } }), current);
      });
      await sendAction(encoder.encode(JSON.stringify({ kind: "chat-reaction", messageId, emoji, actor } satisfies ChatReactionSignal)), { reliable: true });
    } catch { setSendError(true); }
  };
  const recipientName = participants.find((person) => person.identity === recipient)?.name || recipient;
  const host = localParticipant.identity === hostId;
  const moderate = async (action: "mute" | "remove", identity: string, trackSid?: string) => {
    setModerationError(false);
    setModerating(`${action}:${identity}`);
    try {
      await api.moderate(meetingId, action, identity, trackSid);
    } catch {
      setModerationError(true);
    } finally {
      setModerating(null);
    }
  };
  return (
    <aside className={styles.aloChat} aria-label={strings.meetChatTitle}>
      <header><h2>{strings.meetChatTitle}</h2><button type="button" onClick={onClose} aria-label={strings.meetClose}><X /></button></header>
      <nav aria-label={strings.meetChatTitle}>
        <button type="button" className={tab === "messages" ? styles.chatTabActive : undefined} onClick={() => setTab("messages")}>{strings.meetChatMessages}</button>
        <button type="button" className={tab === "people" ? styles.chatTabActive : undefined} onClick={() => setTab("people")}>{strings.meetChatPeople(participants.length)}</button>
        <button type="button" className={tab === "agenda" ? styles.chatTabActive : undefined} onClick={() => setTab("agenda")} title={strings.meetAgenda}><ClipboardList /></button>
        <button type="button" className={tab === "polls" ? styles.chatTabActive : undefined} onClick={() => setTab("polls")} title={strings.meetPolls}><BarChart3 /></button>
        <button type="button" className={tab === "notes" ? styles.chatTabActive : undefined} onClick={() => setTab("notes")} title={strings.meetNotes}><NotebookPen /></button>
        <button type="button" className={tab === "files" ? styles.chatTabActive : undefined} onClick={() => setTab("files")} title={strings.meetFiles}><FolderOpen /></button>
      </nav>
      {tab === "messages" ? (
        <>
          <div className={styles.aloChatMessages}>
            {chatMessages.length === 0 && history.length === 0 ? <ChatWelcome /> : <>
            {history.filter((stored) => !chatMessages.some((live) => live.attributes?.["alo.persistedId"] === stored.id)).map((message) => {
              const person = participants.find((participant) => participant.identity === message.sender);
              const name = person?.name || person?.identity || strings.meetSomeone;
              const mine = message.sender === localParticipant.identity;
              return <article key={message.id} className={mine ? styles.chatMine : undefined}><span className={styles.chatAvatar}>{name.slice(0, 1).toUpperCase()}</span><div className={styles.chatMessageBody}><p><strong>{name}{mine ? ` (${strings.meetYou})` : ""}</strong>{message.recipient !== null && <span className={styles.privateBadge}><Lock />{strings.meetPrivate}</span>}<time>{new Date(message.createdAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</time></p><div className={styles.chatBubble}>{formatChatMessageLinks(message.body)}{message.attachments?.map((attachment) => <StoredChatAttachment key={attachment.id} api={api} attachment={attachment} />)}</div><div className={styles.messageActions}>{["👍", "❤️", "😂", "🎉"].map((emoji) => <button type="button" key={emoji} onClick={() => void react(message.id, emoji)} aria-label={`${strings.meetReact} ${emoji}`}>{emoji}</button>)}{!mine && <button type="button" onClick={() => { setRecipient(message.sender); setTab("messages"); }}><Lock />{strings.meetReplyPrivately}</button>}</div>{Object.entries(reactions[message.id] ?? {}).length > 0 && <div className={styles.messageReactions}>{Object.entries(reactions[message.id] ?? {}).map(([emoji, actors]) => <span key={emoji} title={actors.join(", ")}>{emoji} {actors.length}</span>)}</div>}</div></article>;
            })}
            {chatMessages.map((message) => {
              const name = message.from?.name || message.from?.identity || strings.meetSomeone;
              const privateMessage = message.attributes?.["alo.private"] === "true";
              return <article key={message.id} className={message.from?.isLocal ? styles.chatMine : undefined}>
                <span className={styles.chatAvatar}>{name.slice(0, 1).toUpperCase()}</span>
                <div className={styles.chatMessageBody}><p><strong>{name}{message.from?.isLocal ? ` (${strings.meetYou})` : ""}</strong>{privateMessage && <span className={styles.privateBadge}><Lock />{strings.meetPrivate}</span>}<time>{new Date(message.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</time></p><div className={styles.chatBubble}>{formatChatMessageLinks(message.message)}{message.attachedFiles?.map((file) => <ChatAttachment key={`${message.id}-${file.name}`} file={file} />)}</div>
                  <div className={styles.messageActions}>{["👍", "❤️", "😂", "🎉"].map((emoji) => <button type="button" key={emoji} onClick={() => void react(message.id, emoji, message.attributes?.["alo.persistedId"])} aria-label={`${strings.meetReact} ${emoji}`}>{emoji}</button>)}{!message.from?.isLocal && <button type="button" onClick={() => { setRecipient(message.from?.identity ?? null); setTab("messages"); }}><Lock />{strings.meetReplyPrivately}</button>}</div>
                  {Object.entries(reactions[message.id] ?? {}).length > 0 && <div className={styles.messageReactions}>{Object.entries(reactions[message.id] ?? {}).map(([emoji, actors]) => <span key={emoji} title={actors.join(", ")}>{emoji} {actors.length}</span>)}</div>}
                </div>
              </article>;
            })}</>}
          </div>
          <div className={styles.quickReplies}>{[strings.meetQuickReplyOne, strings.meetQuickReplyTwo, strings.meetQuickReplyThree].map((reply) => <button type="button" key={reply} onClick={() => void submit(reply)}>{reply}</button>)}</div>
          <form className={styles.aloChatComposer} onSubmit={(event) => { event.preventDefault(); void submit(); }}>
            {remoteParticipants.length > 0 && <div className={styles.composerContext}>
              <button type="button" onClick={() => setShowRecipients((shown) => !shown)} className={recipient !== null ? styles.privateRecipient : undefined} aria-expanded={showRecipients}>
                <span className={styles.recipientLabel}>{strings.meetSendTo}</span>
                {recipient === null ? <Users /> : <Lock />}
                <strong>{recipient === null ? strings.meetEveryone : recipientName}</strong>
                <ChevronDown className={styles.recipientChevron} />
              </button>
              {showRecipients && <div className={styles.recipientMenu}>
                <p>{strings.meetChooseRecipient}</p>
                <button type="button" onClick={() => { setRecipient(null); setShowRecipients(false); }}><span><strong>{strings.meetEveryone}</strong><small>{strings.meetEveryoneHint}</small></span>{recipient === null && <Check />}</button>
                {remoteParticipants.map((person) => <button type="button" key={person.identity} onClick={() => { setRecipient(person.identity); setShowRecipients(false); }}><span><strong>{person.name || person.identity}</strong><small>{strings.meetPrivateHint}</small></span>{recipient === person.identity && <Check />}</button>)}
              </div>}
            </div>}
            <div className={styles.composerRow}>
              <input ref={fileInput} type="file" accept="image/*,application/pdf" multiple hidden onChange={(event) => void sendFiles(event.target.files)} />
              <button type="button" className={styles.composerTool} onClick={() => fileInput.current?.click()} aria-label={strings.meetAttachFile} title={strings.meetAttachFile}><Paperclip /></button>
              <div className={styles.emojiControl}><button type="button" className={styles.composerTool} onClick={() => setShowEmoji((shown) => !shown)} aria-label={strings.meetAddEmoji}><Smile /></button>{showEmoji && <div className={styles.emojiMenu}>{["👍", "👏", "❤️", "😂", "😊", "🎉"].map((emoji) => <button type="button" key={emoji} onClick={() => { setDraft((value) => `${value}${emoji}`); setShowEmoji(false); }}>{emoji}</button>)}</div>}</div>
            <input value={draft} onChange={(event) => setDraft(event.target.value)} placeholder={strings.meetChatPlaceholder} aria-label={strings.meetChatPlaceholder} />
            <button type="submit" disabled={draft.trim() === "" || isSending} aria-label={strings.chatSend}><Send /></button>
            </div>
            {sendError && <p className={styles.chatSendError} role="alert">{strings.meetMessageSendFailed}</p>}
          </form>
        </>
      ) : tab === "people" ? (
        <div className={styles.peoplePanel}>
          <ul className={styles.peopleList}>{participants.map((person) => {
            const microphone = person.getTrackPublication(Track.Source.Microphone);
            const muted = microphone === undefined || microphone.isMuted;
            const name = person.name || person.identity;
            return <li key={person.identity} className={person.isSpeaking ? styles.personSpeaking : undefined}>
              <span className={styles.chatAvatar}>{name.slice(0, 1).toUpperCase()}</span>
              <div><strong>{name}{person.identity === hostId ? <em>{strings.meetHost}</em> : null}</strong><small>{person.isLocal ? strings.meetYou : person.isSpeaking ? strings.meetSpeaking : muted ? strings.meetMuted : strings.meetParticipant}</small></div>
              <span className={muted ? styles.personMuted : styles.personAudio} title={muted ? strings.meetMuted : strings.meetMicrophone}>{muted ? <MicOff /> : <Mic />}</span>
              {!person.isLocal && <div className={styles.personActions}>
                <button type="button" onClick={() => { setRecipient(person.identity); setTab("messages"); }} aria-label={strings.meetMessagePrivately}><MessageSquare /></button>
                {host && !muted && microphone?.trackSid && <button type="button" disabled={moderating !== null} onClick={() => void moderate("mute", person.identity, microphone.trackSid)} aria-label={strings.meetMuteParticipant}><MicOff /></button>}
                {host && <button type="button" disabled={moderating !== null} onClick={() => { if (window.confirm(strings.meetRemoveParticipantConfirm(name))) void moderate("remove", person.identity); }} aria-label={strings.meetRemoveParticipant}><UserMinus /></button>}
              </div>}
            </li>;
          })}</ul>
          {moderationError && <p className={styles.chatSendError} role="alert">{strings.meetModerationFailed}</p>}
        </div>
      ) : <MeetingTools meetingId={meetingId} host={host} tab={tab} />}
    </aside>
  );
}

function FullscreenAction() {
  const [full, setFull] = useState(document.fullscreenElement !== null);
  useEffect(() => {
    const changed = () => setFull(document.fullscreenElement !== null);
    document.addEventListener("fullscreenchange", changed);
    return () => document.removeEventListener("fullscreenchange", changed);
  }, []);
  const toggle = async () => {
    if (document.fullscreenElement !== null) await document.exitFullscreen();
    else await document.documentElement.requestFullscreen();
  };
  return <button type="button" className={styles.fullscreen} onClick={() => void toggle()} aria-label={full ? strings.meetExitFullscreen : strings.meetEnterFullscreen} title={full ? strings.meetExitFullscreen : strings.meetEnterFullscreen}><Maximize2 aria-hidden="true" /></button>;
}

type MeetSignal =
  | { kind: "hand"; raised: boolean; name: string }
  | { kind: "reaction"; emoji: string; name: string };

function useMeetingRecording(meetingId: string, hostId: string) {
  const api = useMeetApi();
  const { localParticipant } = useLocalParticipant();
  const [recording, setRecording] = useState<MeetingRecording | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let current = true;
    const refresh = async () => {
      try {
        const value = await api.currentRecording(meetingId);
        if (current) setRecording(value?.status === "completed" || value?.status === "failed" ? null : value);
      } catch {
        // Recording status is supplemental to the call. A short API outage
        // must never interrupt media or eject someone from the room.
      }
    };
    void refresh();
    const timer = window.setInterval(() => void refresh(), 3_000);
    return () => { current = false; window.clearInterval(timer); };
  }, [api, meetingId]);

  const run = async (action: () => Promise<MeetingRecording>) => {
    setBusy(true);
    setError("");
    try { setRecording(await action()); }
    catch (failure) { setError(failure instanceof Error ? failure.message : strings.meetRecordingFailed); }
    finally { setBusy(false); }
  };
  const isHost = localParticipant.identity === hostId;
  const consented = recording?.consents.some((consent) => consent.user === localParticipant.identity) ?? false;
  return {
    recording, busy, error, isHost, consented,
    request: () => run(() => api.requestRecording(meetingId)),
    consent: () => recording === null ? Promise.resolve() : run(() => api.consentRecording(meetingId, recording.id)),
    start: () => recording === null ? Promise.resolve() : run(() => api.startRecording(meetingId, recording.id)),
    stop: () => recording === null ? Promise.resolve() : run(() => api.stopRecording(meetingId, recording.id)),
  };
}

type RecordingControls = ReturnType<typeof useMeetingRecording>;

function MeetingActions({ meetingId, recording, onLeave, chatOpen, onChat, captionsOpen, onCaptions, onSettings, onPictureInPicture }: { meetingId: string; recording: RecordingControls; onLeave: () => void; chatOpen: boolean; onChat: () => void; captionsOpen: boolean; onCaptions: () => void; onSettings: () => void; onPictureInPicture: () => void }) {
  const api = useMeetApi();
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  const { localParticipant } = useLocalParticipant();
  const [handRaised, setHandRaised] = useState(false);
  const [raisedHands, setRaisedHands] = useState<string[]>([]);
  const [reaction, setReaction] = useState<{ emoji: string; name: string; key: number } | null>(null);
  const [copied, setCopied] = useState(false);
  const [minutesBusy, setMinutesBusy] = useState(false);
  const [minutesError, setMinutesError] = useState(false);
  const { send } = useDataChannel("alo-meet-actions", (message) => {
    try {
      const signal = JSON.parse(decoder.decode(message.payload)) as MeetSignal;
      if (signal.kind === "hand") {
        setRaisedHands((current) => signal.raised
          ? Array.from(new Set([...current, signal.name]))
          : current.filter((name) => name !== signal.name));
      } else if (signal.kind === "reaction") {
        setReaction({ emoji: signal.emoji, name: signal.name, key: Date.now() });
      }
    } catch {
      // Data-channel topics are shared with older clients. Unknown payloads
      // are ignored rather than breaking a call already in progress.
    }
  });
  const name = localParticipant.name || strings.meetSomeone;

  const broadcast = async (signal: MeetSignal) => {
    await send(encoder.encode(JSON.stringify(signal)), { reliable: true });
  };
  const toggleHand = () => {
    const raised = !handRaised;
    setHandRaised(raised);
    setRaisedHands((current) => raised
      ? Array.from(new Set([...current, name]))
      : current.filter((person) => person !== name));
    void broadcast({ kind: "hand", raised, name });
  };
  const react = (emoji: string) => {
    setReaction({ emoji, name, key: Date.now() });
    void broadcast({ kind: "reaction", emoji, name });
  };
  const share = async () => {
    const url = `${window.location.origin}/meet?meeting=${encodeURIComponent(meetingId)}`;
    if (navigator.share !== undefined) {
      await navigator.share({ title: strings.meetInviteTitle, text: strings.meetInviteText, url });
    } else {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2_000);
    }
  };
  const generateMinutes = async () => {
    setMinutesBusy(true);
    setMinutesError(false);
    try {
      const segments = await api.transcript(meetingId);
      const text = segments.filter((segment) => segment.final && segment.text.trim() !== "").map((segment) => `${segment.speaker}: ${segment.text}`).join("\n");
      if (text === "") throw new Error("empty transcript");
      const [summary, actions] = await Promise.all([api.summarizeTranscript(text), api.extractTranscriptActions(text)]);
      const actionText = actions.length === 0 ? strings.meetMinutesNoActions : actions.map((action) => `• ${action.title}`).join("\n");
      await api.postMessage(meetingId, `${strings.meetMinutesTitle}\n\n${summary}\n\n${strings.meetMinutesActions}\n${actionText}`, null);
    } catch { setMinutesError(true); }
    finally { setMinutesBusy(false); }
  };

  return (
    <>
      <div className={styles.meetingActions}>
        <button type="button" className={handRaised ? styles.actionActive : undefined} onClick={toggleHand} aria-pressed={handRaised}>
          <Hand aria-hidden="true" />{handRaised ? strings.meetLowerHand : strings.meetRaiseHand}
        </button>
        <div className={styles.reactions}>
          <button type="button" aria-label={strings.meetReact} title={strings.meetReact}><Smile aria-hidden="true" /></button>
          <div className={styles.reactionMenu}>
            {["👍", "👏", "❤️", "😂", "🎉"].map((emoji) => (
              <button type="button" key={emoji} onClick={() => react(emoji)} aria-label={`${strings.meetReact} ${emoji}`}>{emoji}</button>
            ))}
          </div>
        </div>
        <button type="button" onClick={() => void share()}>
          {copied ? <Copy aria-hidden="true" /> : <Share2 aria-hidden="true" />}
          {copied ? strings.meetLinkCopied : strings.meetInvite}
        </button>
        <button type="button" className={chatOpen ? styles.actionActive : undefined} onClick={onChat} aria-pressed={chatOpen}>
          <MessageSquare aria-hidden="true" />{strings.meetChat}
        </button>
        <button type="button" className={captionsOpen ? styles.actionActive : undefined} onClick={onCaptions} aria-pressed={captionsOpen} aria-label={strings.meetCaptions} title={strings.meetCaptions}>
          <Captions aria-hidden="true" />
        </button>
        <button type="button" onClick={onPictureInPicture} aria-label={strings.meetPictureInPicture} title={strings.meetPictureInPicture}>
          <PictureInPicture2 aria-hidden="true" />
        </button>
        <button type="button" onClick={onSettings} aria-label={strings.meetDeviceSettings} title={strings.meetDeviceSettings}>
          <Settings aria-hidden="true" />
        </button>
        {recording.isHost && <button type="button" disabled={minutesBusy} onClick={() => void generateMinutes()} title={strings.meetGenerateMinutes}><Sparkles aria-hidden="true" />{strings.meetGenerateMinutes}</button>}
        {recording.recording === null && recording.isHost && (
          <button type="button" disabled={recording.busy} onClick={() => void recording.request()}><Circle aria-hidden="true" />{strings.meetRecord}</button>
        )}
        {recording.recording?.status === "pending" && recording.isHost && (
          <button type="button" disabled={recording.busy} onClick={() => void recording.start()}><Circle aria-hidden="true" />{strings.meetStartRecording}</button>
        )}
        {recording.recording?.status === "pending" && !recording.isHost && !recording.consented && (
          <button type="button" disabled={recording.busy} onClick={() => void recording.consent()}><Check aria-hidden="true" />{strings.meetIConsent}</button>
        )}
        {recording.recording?.status === "recording" && recording.isHost && (
          <button type="button" className={styles.recordingActive} disabled={recording.busy} onClick={() => void recording.stop()}><Square aria-hidden="true" />{strings.meetStopRecording}</button>
        )}
        {recording.recording?.status === "recording" && !recording.isHost && (
          <button type="button" className={styles.recordingActive} disabled><Circle aria-hidden="true" />{strings.meetRecording}</button>
        )}
        <button type="button" className={styles.leaveAction} onClick={onLeave}>
          <PhoneOff aria-hidden="true" />{strings.meetLeave}
        </button>
      </div>
      {raisedHands.length > 0 && (
        <div className={styles.raisedHands} role="status"><Hand aria-hidden="true" />{strings.meetHandsRaised(raisedHands.join(", "))}</div>
      )}
      {reaction !== null && (
        <div key={reaction.key} className={styles.reactionBurst} role="status">
          <span>{reaction.emoji}</span><small>{reaction.name}</small>
        </div>
      )}
      {minutesError && <div className={styles.recordingError} role="alert">{strings.meetMinutesFailed}</div>}
    </>
  );
}

function MeetingCaptions({ meetingId, visible }: { meetingId: string; visible: boolean }) {
  const api = useMeetApi();
  const live = useTranscriptions();
  const { localParticipant } = useLocalParticipant();
  const participants = useParticipants();
  const [stored, setStored] = useState<MeetingTranscriptSegment[]>([]);
  const [language, setLanguage] = useState<"original" | "en" | "fr" | "nl">("original");
  const [translations, setTranslations] = useState<Record<string, string>>({});
  const saved = useRef(new Map<string, string>());
  useEffect(() => {
    let current = true;
    void api.transcript(meetingId).then((segments) => { if (current) setStored(segments); });
    return () => { current = false; };
  }, [api, meetingId]);
  useEffect(() => {
    for (const segment of live) {
      const transcribedTrack = segment.streamInfo.attributes?.["lk.transcribed_track_id"];
      const localMicrophone = localParticipant.getTrackPublication(Track.Source.Microphone)?.trackSid;
      if (segment.participantInfo.identity !== localParticipant.identity && transcribedTrack !== localMicrophone) continue;
      const id = segment.streamInfo.attributes?.["lk.segment_id"] ?? segment.streamInfo.id;
      const finalSegment = segment.streamInfo.attributes?.["lk.transcription_final"] === "true";
      const fingerprint = `${segment.text}\u0000${finalSegment}`;
      if (segment.text.trim() === "" || saved.current.get(id) === fingerprint) continue;
      saved.current.set(id, fingerprint);
      void api.putTranscriptSegment(meetingId, { id, text: segment.text, final: finalSegment }).then((persisted) => {
        setStored((current) => [...current.filter((item) => item.id !== persisted.id), persisted].sort((a, b) => a.createdAt.localeCompare(b.createdAt)));
      });
    }
  }, [api, live, localParticipant.identity, meetingId]);
  const merged = new Map(stored.map((segment) => [segment.id, segment]));
  for (const segment of live) {
    const id = segment.streamInfo.attributes?.["lk.segment_id"] ?? segment.streamInfo.id;
    const transcribedTrack = segment.streamInfo.attributes?.["lk.transcribed_track_id"];
    const speaker = participants.find((participant) => participant.getTrackPublication(Track.Source.Microphone)?.trackSid === transcribedTrack);
    merged.set(id, {
      id,
      speaker: speaker?.identity ?? segment.participantInfo.identity,
      text: segment.text,
      final: segment.streamInfo.attributes?.["lk.transcription_final"] === "true",
      createdAt: merged.get(id)?.createdAt ?? new Date().toISOString(),
    });
  }
  const lines = Array.from(merged.values()).filter((segment) => segment.text.trim() !== "").slice(-3);
  useEffect(() => {
    if (!visible || language === "original") return;
    for (const line of lines.filter((segment) => segment.final)) {
      const key = `${line.id}:${language}`;
      if (translations[key] !== undefined) continue;
      void api.translateCaption(line.text, language).then((text) => setTranslations((current) => ({ ...current, [key]: text }))).catch(() => undefined);
    }
  }, [api, language, lines, translations, visible]);
  if (!visible) return null;
  return <div className={styles.captionOverlay} role="log" aria-live="polite" aria-label={strings.meetCaptions}>
    <label className={styles.captionLanguage}>{strings.meetCaptionLanguage}<select value={language} onChange={(event) => setLanguage(event.target.value as typeof language)}><option value="original">{strings.meetCaptionOriginal}</option><option value="en">English</option><option value="nl">Nederlands</option><option value="fr">Français</option></select></label>
    {lines.length === 0 ? <span className={styles.captionWaiting}>{strings.meetCaptionsWaiting}</span> : lines.map((segment) => <p key={segment.id}><strong>{segment.speaker === localParticipant.identity ? strings.meetYou : segment.speaker}</strong><span>{language === "original" ? segment.text : translations[`${segment.id}:${language}`] ?? segment.text}</span></p>)}
  </div>;
}

function DeviceSettings({ onClose }: { onClose: () => void }) {
  const room = useRoomContext();
  const [devices, setDevices] = useState<MediaDeviceInfo[]>([]);
  const [selected, setSelected] = useState<Record<MediaDeviceKind, string>>({ audioinput: "", audiooutput: "", videoinput: "" });
  const [background, setBackground] = useState<"none" | "blur">(() => room.localParticipant.getTrackPublication(Track.Source.Camera)?.videoTrack?.getProcessor() ? "blur" : "none");
  const [backgroundBusy, setBackgroundBusy] = useState(false);
  const [backgroundError, setBackgroundError] = useState("");
  useEffect(() => {
    let current = true;
    void navigator.mediaDevices.enumerateDevices().then((found) => {
      if (!current) return;
      setDevices(found);
      setSelected({
        audioinput: room.getActiveDevice("audioinput") ?? "",
        audiooutput: room.getActiveDevice("audiooutput") ?? "",
        videoinput: room.getActiveDevice("videoinput") ?? "",
      });
    });
    return () => { current = false; };
  }, [room]);
  const choose = async (kind: MediaDeviceKind, deviceId: string) => {
    await room.switchActiveDevice(kind, deviceId);
    setSelected((value) => ({ ...value, [kind]: deviceId }));
  };
  const chooseBackground = async (next: "none" | "blur") => {
    const camera = room.localParticipant.getTrackPublication(Track.Source.Camera)?.videoTrack;
    if (camera === undefined || backgroundBusy || next === background) return;
    setBackgroundBusy(true);
    setBackgroundError("");
    try {
      if (next === "none") {
        await camera.stopProcessor();
      } else {
        const { BackgroundProcessor, supportsBackgroundProcessors } = await import("@livekit/track-processors");
        if (!supportsBackgroundProcessors()) throw new Error("unsupported");
        await camera.setProcessor(BackgroundProcessor({ mode: "background-blur", blurRadius: 12 }));
      }
      setBackground(next);
    } catch {
      setBackgroundError(strings.meetBackgroundUnsupported);
    } finally {
      setBackgroundBusy(false);
    }
  };
  const groups: Array<{ kind: MediaDeviceKind; label: string }> = [
    { kind: "audioinput", label: strings.meetMicrophone },
    { kind: "videoinput", label: strings.meetCamera },
    { kind: "audiooutput", label: strings.meetSpeaker },
  ];
  return <div className={styles.settingsBackdrop} role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <section className={styles.deviceSettings} role="dialog" aria-modal="true" aria-labelledby="meet-device-settings-title">
      <header><div><span>{strings.meetSettings}</span><h2 id="meet-device-settings-title">{strings.meetDeviceSettings}</h2></div><button type="button" onClick={onClose} aria-label={strings.meetClose}><X /></button></header>
      <div className={styles.deviceFields}>{groups.map(({ kind, label }) => <label key={kind}><span>{label}</span><select value={selected[kind]} onChange={(event) => void choose(kind, event.target.value)}>{devices.filter((device) => device.kind === kind).map((device, index) => <option key={device.deviceId} value={device.deviceId}>{device.label || `${label} ${index + 1}`}</option>)}</select></label>)}</div>
      <div className={styles.backgroundSettings}>
        <div><strong>{strings.meetBackgroundEffects}</strong><small>{strings.meetBackgroundEffectsHint}</small></div>
        <div role="group" aria-label={strings.meetBackgroundEffects}>
          <button type="button" className={background === "none" ? styles.backgroundActive : undefined} disabled={backgroundBusy} onClick={() => void chooseBackground("none")}><span className={styles.backgroundNone} />{strings.meetBackgroundNone}</button>
          <button type="button" className={background === "blur" ? styles.backgroundActive : undefined} disabled={backgroundBusy} onClick={() => void chooseBackground("blur")}><span className={styles.backgroundBlur} />{strings.meetBackgroundBlur}</button>
        </div>
        {backgroundError !== "" && <p role="alert">{backgroundError}</p>}
      </div>
      <p>{strings.meetDeviceSettingsHint}</p>
      <footer><Button onClick={onClose}>{strings.meetDone}</Button></footer>
    </section>
  </div>;
}

function ConnectionRecovery() {
  const state = useConnectionState();
  if (state !== ConnectionState.Reconnecting && state !== ConnectionState.SignalReconnecting) return null;
  return <div className={styles.connectionRecovery} role="status" aria-live="polite"><RefreshCw aria-hidden="true" /><div><strong>{strings.meetReconnecting}</strong><span>{strings.meetReconnectingHint}</span></div></div>;
}

function MeetingExperience({ meetingId, hostId, onLeave }: { meetingId: string; hostId: string; onLeave: () => void }) {
  const [chatOpen, setChatOpen] = useState(true);
  const [captionsOpen, setCaptionsOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const recording = useMeetingRecording(meetingId, hostId);
  const pictureInPicture = async () => {
    const video = document.querySelector<HTMLVideoElement>(`.${styles.livekit} video:not([data-lk-local-participant="true"])`) ?? document.querySelector<HTMLVideoElement>(`.${styles.livekit} video`);
    if (video === null || document.pictureInPictureEnabled !== true) return;
    if (document.pictureInPictureElement !== null) await document.exitPictureInPicture();
    else await video.requestPictureInPicture();
  };
  return <>
    <VideoConference chatMessageFormatter={formatChatMessageLinks} />
    <PresentingNotice />
    <ConnectionRecovery />
    <FullscreenAction />
    <MeetingActions meetingId={meetingId} recording={recording} onLeave={onLeave} chatOpen={chatOpen} onChat={() => setChatOpen((open) => !open)} captionsOpen={captionsOpen} onCaptions={() => setCaptionsOpen((open) => !open)} onSettings={() => setSettingsOpen(true)} onPictureInPicture={() => void pictureInPicture()} />
    {recording.recording?.status === "pending" && (
      <aside className={styles.recordingNotice} role="status">
        <Circle aria-hidden="true" />
        <div><strong>{strings.meetRecordingConsentTitle}</strong><span>{strings.meetRecordingConsentBody}</span></div>
        <small>{strings.meetConsentCount(recording.recording.consents.length)}</small>
        {!recording.isHost && !recording.consented && <button type="button" disabled={recording.busy} onClick={() => void recording.consent()}>{strings.meetIConsent}</button>}
        {recording.consented && <span className={styles.consentGiven}><Check aria-hidden="true" />{strings.meetRecordingConsentGiven}</span>}
      </aside>
    )}
    {recording.recording?.status === "recording" && <div className={styles.recordingIndicator} role="status"><span />{strings.meetRecording}</div>}
    {recording.error !== "" && <div className={styles.recordingError} role="alert">{recording.error}</div>}
    <MeetingCaptions meetingId={meetingId} visible={captionsOpen} />
    {chatOpen && <InCallChat meetingId={meetingId} hostId={hostId} onClose={() => setChatOpen(false)} />}
    {settingsOpen && <DeviceSettings onClose={() => setSettingsOpen(false)} />}
  </>;
}

/**
 * Join a meeting and hold it until the person leaves.
 *
 * `onLeft` fires only when they choose to hang up. Network failures remain on
 * this screen so the person can reconnect without losing their place.
 */
export function MeetRoom({
  meetingId,
  onLeft,
}: {
  meetingId: string;
  onLeft: () => void;
}) {
  const api = useMeetApi();
  const { identity } = useAuth();
  const [grant, setGrant] = useState<JoinGrant | null>(null);
  const [problem, setProblem] = useState<{ kind: "join" | "unavailable"; message: string } | null>(null);
  // Which camera and microphone, checked before joining rather than
  // discovered after. Somebody who joins broken usually leaves rather than
  // hunting for a settings menu mid-meeting.
  const [choices, setChoices] = useState<LocalUserChoices | null>(null);
  const [joinAttempt, setJoinAttempt] = useState(0);
  const connectedOnce = useRef(false);

  useEffect(() => {
    if (choices === null) return;
    let joined = true;
    setGrant(null);
    void (async () => {
      try {
        const g = await api.join(meetingId);
        // The person may have left before the token arrived; joining a room
        // nobody is looking at would hold a camera open.
        if (joined) setGrant(g);
      } catch (failure) {
        if (!joined) return;
        if (failure instanceof MeetApiError && failure.status === 404) {
          onLeft();
          return;
        }
        setProblem(failure instanceof MeetUnavailable
          ? { kind: "unavailable", message: strings.meetNoEngine }
          : { kind: "join", message: strings.meetJoinFailed });
      }
    })();
    return () => {
      joined = false;
    };
  }, [api, choices, joinAttempt, meetingId, onLeft]);

  if (problem !== null) {
    return (
      <div className={styles.notice}>
        <div className={styles.noticeCard} role="alert">
          <span className={styles.noticeMark}>
            {problem.kind === "unavailable"
              ? <ServerOff aria-hidden="true" />
              : <Video aria-hidden="true" />}
          </span>
          <span className={styles.noticeEyebrow}>{strings.meetTitle}</span>
          <h1 className={styles.noticeTitle}>
            {problem.kind === "unavailable" ? strings.meetUnavailableTitle : strings.meetJoinProblemTitle}
          </h1>
          <p className={styles.noticeText}>{problem.message}</p>
          <div className={styles.noticeActions}>
            {problem.kind === "join" && (
              <Button
                icon={<RefreshCw aria-hidden="true" />}
                onClick={() => {
                  connectedOnce.current = false;
                  setProblem(null);
                  setJoinAttempt((attempt) => attempt + 1);
                }}
              >
                {strings.meetRetry}
              </Button>
            )}
            <Button variant="ghost" onClick={onLeft}>{strings.meetClose}</Button>
          </div>
        </div>
      </div>
    );
  }

  if (choices === null) {
    return (
      <div className={`${styles.room} ${styles.prejoinRoom}`} data-lk-theme="default">
        <Button
          variant="ghost"
          className={styles.back}
          icon={<ArrowLeft aria-hidden="true" />}
          onClick={onLeft}
        >
          {strings.meetBack}
        </Button>
        <main className={styles.prejoinShell}>
          <section className={styles.prejoinIntro}>
            <p className={styles.readyGreeting}>
              <img src={wavingHand} alt="" />
              {strings.meetReadyGreeting(identity?.name.split(" ")[0] ?? "")}
            </p>
            <h1>{strings.meetReadyTitle}</h1>
            <p className={styles.readyCopy}>{strings.meetReadyBody}</p>
            <div className={styles.readySafety}>
              <span><ShieldCheck aria-hidden="true" /></span>
              <div><strong>{strings.meetReadySafetyTitle}</strong><p>{strings.meetReadySafetyBody}</p></div>
            </div>
          </section>
          <section className={styles.prejoin} aria-label={strings.meetReadyTitle}>
            <PreJoin
            defaults={{
              // The same default the call itself uses: heard, and seen only
              // by choice.
              audioEnabled: true,
              videoEnabled: false,
              // LiveKit requires a non-empty display name before enabling its
              // submit button. alo's signed token owns the real identity, so
              // this internal placeholder is never shown or sent as identity.
              username: "alo",
            }}
            onSubmit={setChoices}
            onError={() => setProblem({ kind: "join", message: strings.meetJoinFailed })}
            joinLabel={strings.meetJoinNow}
            micLabel={strings.meetMicrophone}
            camLabel={strings.meetCamera}
            persistUserChoices
            />
            <p className={styles.joinHint}>{strings.meetSettingsAfterJoin}</p>
          </section>
        </main>
      </div>
    );
  }

  if (grant === null) {
    return (
      <div className={styles.notice} aria-live="polite" aria-busy="true">
        <span className={styles.joiningMark}><Video aria-hidden="true" /></span>
        <p className={styles.noticeText}>{strings.meetJoining}</p>
      </div>
    );
  }

  return (
    <div className={styles.room}>
      <Button
        variant="ghost"
        className={styles.inCallBack}
        icon={<ArrowLeft aria-hidden="true" />}
        onClick={onLeft}
      >
        {strings.meetBack}
      </Button>
      <div className={styles.inCallSafety} role="status">
        <ShieldCheck aria-hidden="true" />
        <span>{strings.meetReadySafetyTitle}</span>
      </div>
      <MeetingHeader grant={grant} />
      <LiveKitRoom
        serverUrl={grant.url}
        token={grant.token}
        // Connect straight away: the person pressed join, and asking again
        // inside the room would be asking twice.
        connect
        // Camera off, microphone on. Somebody joining a call expects to be
        // heard and to choose to be seen — the reverse surprises people in a
        // way that is hard to undo.
        video={choices.videoEnabled}
        audio={choices.audioEnabled}
        options={{ adaptiveStream: true, dynacast: true }}
        onConnected={() => { connectedOnce.current = true; }}
        onDisconnected={() => {
          // A failed initial signal connection is not the same action as the
          // person leaving. Keep them in Meet with a useful retry state.
          setProblem({ kind: "join", message: connectedOnce.current ? strings.meetConnectionLost : strings.meetJoinFailed });
        }}
        onError={() => setProblem({ kind: "join", message: strings.meetJoinFailed })}
        className={styles.livekit}
        // The engine's own theme attribute. Without it none of its CSS
        // variables exist, and `.lk-grid-layout-wrapper`'s
        // `height: calc(100% - var(--lk-control-bar-height))` becomes invalid
        // — so the video area collapsed to zero and the call rendered as an
        // empty black screen. A day of CSS guesses; one missing attribute.
        data-lk-theme="default"
      >
        <MeetingExperience meetingId={meetingId} hostId={grant.meeting.createdBy} onLeave={onLeft} />
      </LiveKitRoom>
    </div>
  );
}
