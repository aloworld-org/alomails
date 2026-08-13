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

const messageClass = "group relative mt-4 flex w-fit max-w-4xl flex-col items-start pl-12 pr-3";
const mineClass = "group relative mt-4 flex w-fit max-w-4xl flex-col items-end self-end pl-3";
const toolClass = "flex size-7 items-center justify-center rounded-sm border-0 bg-transparent text-secondary hover:bg-raised hover:text-primary focus-visible:outline-2 focus-visible:outline-accent";
const bubbleClass = "m-0 max-w-full whitespace-pre-wrap break-words rounded-xl border border-subtle bg-surface px-4 py-3 text-sm leading-relaxed text-primary group-hover:border-default";
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
  const authoredByMe = me !== null && message.authorKind === "user" && message.author === me;
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
      className={`${authoredByMe ? mineClass : messageClass} ${grouped ? "mt-0 py-1" : ""}`}
    >
      {editing === null && (
        <div className="pointer-events-none absolute -top-4 right-2 z-10 flex gap-1 rounded-md border border-subtle bg-surface p-1 opacity-0 shadow-md transition-opacity group-hover:pointer-events-auto group-hover:opacity-100 focus-within:pointer-events-auto focus-within:opacity-100">
          {reactable && (
            <span className="relative inline-flex" ref={pickerRef}>
              <button
                type="button"
                className={toolClass}
                onClick={() => setPicking((open) => !open)}
                aria-label={strings.chatAddReaction}
                title={strings.chatAddReaction}
                aria-expanded={picking}
              >
                <SmilePlus size={16} />
              </button>
              {picking && (
                <span className="absolute bottom-full left-0 z-20 mb-1 flex gap-1 rounded-md border border-subtle bg-raised p-1 shadow-md" role="menu">
                  {palette.map((emoji) => (
                    <button
                      key={emoji}
                      type="button"
                      role="menuitem"
                      className="flex size-8 items-center justify-center rounded-sm border-0 bg-transparent text-base hover:bg--tint focus-visible:bg--tint"
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
            <span className="relative inline-flex">
              <button
                type="button"
                className={toolClass}
                onClick={() => onReplyHere(message)}
                aria-label={strings.chatReplyHere}
                title={strings.chatReplyHere}
              >
                <Reply size={15} />
              </button>
              {onReplyPrivate !== undefined && (
                <button
                  type="button"
                  className={toolClass}
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
                className={toolClass}
                onClick={() => setEditing(message.body)}
                aria-label={strings.chatEditAction}
                title={strings.chatEditAction}
              >
                <Pencil size={15} />
              </button>
              <button
                type="button"
                className={toolClass}
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
        <span className="absolute left-0 top-1 w-10 overflow-hidden whitespace-nowrap pr-1 text-right text-xs tabular-nums text-tertiary opacity-0 group-hover:opacity-100">
          {shortTime(message.createdAt)}
        </span>
      ) : (
        <div className="mb-1 flex items-center gap-2">
          {isAgent ? (
            <span className="flex size-6 shrink-0 items-center justify-center rounded-sm bg--tint text-accent" aria-hidden="true">
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
            className={isAgent ? "text-sm font-semibold text-accent" : "text-sm font-bold text-primary"}
            // The full address on hover: the local part is what people say, the
            // address is what settles who it was.
            title={message.authorEmail ?? message.author}
          >
            {who}
          </span>
          {isAgent && (
            // An agent is not a colleague and must never be mistaken for one.
            <span className="shrink-0 rounded-sm border border-subtle px-2 text-xs font-semibold uppercase tracking-wide text-tertiary">{strings.chatAgentTag}</span>
          )}
          <span className="text-xs tabular-nums text-tertiary">{timeOf(message.createdAt)}</span>
          {message.editedAt !== null && message.deletedAt === null && (
            <span className="text-xs tabular-nums text-tertiary">{strings.chatEdited}</span>
          )}
        </div>
      )}
      {message.body.startsWith("__meeting__:") ? (
        // The seam Teams leaves open, closed: the room knows a call is
        // happening and you join from where the conversation already is.
        <span className="inline-flex items-center gap-2 rounded-lg border border-subtle bg--tint px-3 py-2">
          <Video size={16} className="shrink-0 text-accent" />
          <span className="text-sm text-primary">{strings.meetStartedHere}</span>
          <Button
            size="sm"
            variant="primary"
            onClick={() =>
              onJoinMeeting?.(message.body.slice("__meeting__:".length))
            }
          >
            {strings.meetJoin}
          </Button>
        </span>
      ) : editing !== null ? (
        <form
          className="mt-1 flex max-w-full flex-col items-end gap-2 rounded-lg border border-subtle bg-surface p-2"
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
            className="min-h-16 max-h-96 w-full resize-y rounded-sm border border-accent bg-app p-2 font-ui text-sm leading-relaxed text-primary outline-none focus:ring-2 focus:ring--tint"
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
          className={message.deletedAt === null ? `${bubbleClass} ${authoredByMe ? "bg--soft" : namesMe ? "bg--tint" : ""}` : `${bubbleClass} italic text-tertiary`}
        >
          {message.deletedAt === null
            ? renderBody(message.body, withHandlesMarked)
            : strings.chatWithdrawn}
        </p>
      )}

      {message.proposal !== null && (
        <div className="mt-2 max-w-xl">
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
        <ul className="mt-1 flex list-none flex-col gap-1 p-0">
          {message.attachments.map((file) => (
            <li key={file.node}>
              {/* A plain link to Drive's own download route: the file is not
                  copied here, so opening it is Drive's business and Drive's
                  permission check. */}
              <button
                type="button"
                className="inline-flex min-h-10 max-w-full items-center gap-2 rounded-sm border border-subtle bg-surface px-2 text-sm text-primary hover:border-default hover:bg-raised"
                onClick={() => void onOpenFile(file)}
                title={strings.chatOpenFile}
              >
                <Paperclip size={14} className="shrink-0 text-tertiary" />
                <span className="truncate">{file.name}</span>
                <span className="shrink-0 text-xs tabular-nums text-tertiary">{fileSize(file.size)}</span>
                {file.trashed && (
                  <span className="shrink-0 text-xs italic text-tertiary">
                    {strings.chatFileTrashed}
                  </span>
                )}
              </button>
            </li>
          ))}
        </ul>
      )}

      {message.reactions.length > 0 && (
        <div className="mt-1 flex flex-wrap items-center gap-1">
          {message.reactions.map((reaction) => (
            <button
              key={reaction.emoji}
              type="button"
              className={`inline-flex min-h-7 items-center gap-1 rounded-lg border px-2 text-xs ${reaction.mine ? "border-accent bg--tint font-semibold text-primary" : "border-subtle bg-surface text-secondary hover:border-default"}`}
              onClick={() => onReact(reaction.emoji)}
              // The chip is a toggle, and says which way it will go.
              aria-pressed={reaction.mine}
              disabled={!reactable}
            >
              <span aria-hidden="true">{reaction.emoji}</span>
              <span className="tabular-nums">{reaction.count}</span>
            </button>
          ))}
        </div>
      )}

      {children}
    </article>
  );
}
