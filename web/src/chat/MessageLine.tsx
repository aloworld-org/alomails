import type { ReactNode } from "react";
import { useCallback, useRef, useState } from "react";
import { MessagesSquare, Paperclip, Pencil, Reply, SmilePlus, Sparkles, Trash2, Video } from "lucide-react";

import { Avatar, Button, useDismiss } from "../ds";
import { fileSize } from "../drive";
import { strings } from "../i18n";
import { AgentActionCard } from "../shell/AgentActionCard";
import { personName, shortTime, standingOf, timeOf, withHandlesMarked } from "./presentation";
import { renderBody } from "./richText";
import type { Attachment, Message, Proposal } from "./types";
import styles from "./ChatModule.module.css";
export function MessageLine({
  message,
  palette,
  me,
  onReact,
  onOpenFile,
  onDecide,
  onEdit,
  onWithdraw,
  onReplyHere,
  onReplyPrivate,
  onJoinMeeting,
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
  onReplyHere?: ((message: Message) => void) | undefined;
  onReplyPrivate?: ((message: Message) => void) | undefined;
  /** Join the meeting this message announces. */
  onJoinMeeting?: ((id: string) => void) | undefined;
  /** This message continues the previous author's run: no avatar, no name, no
   *  timestamp â€” just the words, aligned under the ones above. */
  grouped?: boolean;
  children?: ReactNode;
}) {
  const namesMe = me !== null && message.mentions.includes(me);
  const [picking, setPicking] = useState(false);
  const pickerRef = useRef<HTMLSpanElement | null>(null);
  const closePicker = useCallback(() => setPicking(false), []);
  useDismiss(picking, pickerRef, closePicker);
  const [editing, setEditing] = useState<string | null>(null);
  // Mine to change: my own words, still standing, and never an agent's â€” an
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
  // Withdrawn words take no reactions â€” the server refuses, so the picker is
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
          {onReplyHere !== undefined && message.deletedAt === null && (
            <span className={styles.pickerWrap}>
              <button
                type="button"
                className={styles.tool}
                onClick={() => onReplyHere(message)}
                aria-label={strings.chatReplyHere}
                title={strings.chatReplyHere}
              >
                <Reply size={15} />
              </button>
              {onReplyPrivate !== undefined && (
                <button
                  type="button"
                  className={styles.tool}
                  onClick={() => onReplyPrivate(message)}
                  aria-label={strings.chatReplyPrivately}
                  title={strings.chatReplyPrivately}
                >
                  <MessagesSquare size={15} />
                </button>
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
      {message.body.startsWith("__meeting__:") ? (
        // The seam Teams leaves open, closed: the room knows a call is
        // happening and you join from where the conversation already is.
        <span className={styles.meetingCard}>
          <Video size={16} className={styles.meetingMark} />
          <span className={styles.meetingText}>{strings.meetStartedHere}</span>
          <button
            type="button"
            className={styles.meetingJoin}
            onClick={() =>
              onJoinMeeting?.(message.body.slice("__meeting__:".length))
            }
          >
            {strings.meetJoin}
          </button>
        </span>
      ) : editing !== null ? (
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

