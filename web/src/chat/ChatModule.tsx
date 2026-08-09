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
  Reply,
  Send,
  Users,
  X,
} from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { Avatar, Button } from "../ds";
import { ChatError, chatMessage, useChatApi } from "./api";
import type { ChannelSummary, FeedMessage, Message } from "./types";
import styles from "./ChatModule.module.css";

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

/** Local time of day, for the line beside an author. */
function timeOf(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime())
    ? ""
    : at.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

/**
 * One line of conversation, used by both the feed and the thread panel — the
 * two must never drift into showing a message differently. `children` is what
 * hangs under it (the thread affordance in the feed, nothing in a thread).
 */
function MessageLine({
  message,
  children,
}: {
  message: Message;
  children?: ReactNode;
}) {
  const who = personName(message.authorEmail, message.author);
  return (
    <article className={styles.message}>
      <div className={styles.messageMeta}>
        <Avatar name={who} email={message.authorEmail ?? undefined} size="sm" />
        <span
          className={styles.author}
          // The full address on hover: the local part is what people say, the
          // address is what settles who it was.
          title={message.authorEmail ?? message.author}
        >
          {who}
        </span>
        <span className={styles.time}>{timeOf(message.createdAt)}</span>
        {message.editedAt !== null && message.deletedAt === null && (
          <span className={styles.edited}>{strings.chatEdited}</span>
        )}
      </div>
      <p
        className={message.deletedAt === null ? styles.body : styles.withdrawn}
      >
        {message.deletedAt === null ? message.body : strings.chatWithdrawn}
      </p>
      {children}
    </article>
  );
}

export function ChatModule() {
  const api = useChatApi();
  const client = useJmapClient();
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
  const feedRef = useRef<HTMLDivElement | null>(null);

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
    if (openId === null) return;
    setMessages(null);
    // A thread belongs to the room it was opened in; changing rooms closes it
    // rather than leaving a panel of someone else's replies on screen.
    setThreadSeq(null);
    setReplies(null);
    void loadMessages(openId);
  }, [openId, loadMessages]);

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

  // Keep the newest line in view as the conversation grows.
  useEffect(() => {
    const feed = feedRef.current;
    if (feed !== null) feed.scrollTop = feed.scrollHeight;
  }, [messages]);

  async function send() {
    const body = draft.trim();
    if (body === "" || openId === null || sending) return;
    setSending(true);
    setError(null);
    setDraft("");
    try {
      const sent = await api.post(openId, body);
      // A message just said has no replies yet; the refetch will confirm it.
      setMessages((current) => [
        ...(current ?? []),
        { ...sent, replyCount: 0, lastReplyAt: null },
      ]);
      void loadChannels();
    } catch (failure) {
      setDraft(body); // give the words back rather than losing them
      setError(chatMessage(failure, strings.chatSendFailed));
    } finally {
      setSending(false);
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
    const name = window.prompt(strings.chatNewChannelPrompt)?.trim();
    if (name === undefined || name === "") return;
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
          <Button
            variant="primary"
            size="sm"
            onClick={() => void createChannel()}
            disabled={creating}
          >
            <MessageSquarePlus size={15} />
            {strings.chatNewChannel}
          </Button>
        </header>
        {channels === null ? (
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
                  {channel.unread > 0 && (
                    <span className={styles.badge}>{channel.unread}</span>
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
              <h3 className={styles.roomName}>{channelLabel(open)}</h3>
              {open.topic !== null && (
                <p className={styles.roomTopic}>{open.topic}</p>
              )}
            </header>

            <div className={styles.feed} ref={feedRef}>
              {messages === null ? (
                <p className={styles.feedNote}>
                  <Loader2 className={styles.spin} size={14} />{" "}
                  {strings.chatLoading}
                </p>
              ) : messages.length === 0 ? (
                <p className={styles.feedNote}>{strings.chatNoMessagesYet}</p>
              ) : (
                messages.map((message) => (
                  <MessageLine key={message.id} message={message}>
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
                <input
                  className={styles.input}
                  value={draft}
                  onChange={(event) => setDraft(event.target.value)}
                  placeholder={strings.chatComposerPlaceholder(
                    channelLabel(open),
                  )}
                  aria-label={strings.chatComposerLabel}
                  autoComplete="off"
                />
                <Button
                  type="submit"
                  variant="primary"
                  disabled={draft.trim() === "" || sending}
                >
                  <Send size={15} />
                  {strings.chatSend}
                </Button>
              </form>
            )}
          </>
        )}
      </section>

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
            <MessageLine message={threadRoot} />
            <hr className={styles.threadRule} />
            {replies === null ? (
              <p className={styles.feedNote}>
                <Loader2 className={styles.spin} size={14} />{" "}
                {strings.chatLoading}
              </p>
            ) : replies.length === 0 ? (
              <p className={styles.feedNote}>{strings.chatThreadEmpty}</p>
            ) : (
              replies.map((reply) => (
                <MessageLine key={reply.id} message={reply} />
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
