import type { RefObject } from "react";
import { Fragment } from "react";
import { Hash, Loader2, Pencil } from "lucide-react";

import { strings } from "../i18n";
import { MessageLine } from "./MessageLine";
import { channelLabel, continues, dayOf } from "./presentation";
import type { Attachment, ChannelSummary, FeedMessage, Message, Proposal } from "./types";

type Props = {
  room: ChannelSummary;
  messages: FeedMessage[] | null;
  feedRef: RefObject<HTMLDivElement | null>;
  moreBehind: boolean;
  loadingOlder: boolean;
  readUpTo: number | null;
  palette: string[];
  me: string | null;
  onOlder: () => void;
  onReact: (message: Message, emoji: string) => void;
  onOpenFile: (file: Attachment) => void;
  onDecide: (proposal: Proposal, approve: boolean) => void;
  onEdit: (message: Message, body: string) => void;
  onWithdraw: (message: Message) => void;
  onReplyHere: (message: Message) => void;
  onReplyPrivate: (message: Message) => void;
  onDescription: () => void;
};

export function MessageFeed({ room, messages, feedRef, moreBehind, loadingOlder, readUpTo, palette, me, onOlder, onReact, onOpenFile, onDecide, onEdit, onWithdraw, onReplyHere, onReplyPrivate, onDescription }: Props) {
  return <div className="flex min-h-0 flex-1 flex-col overflow-y-auto bg-surface px-8 pb-8" ref={feedRef}>
    {!moreBehind && messages !== null && <div className="mx-auto my-10 flex w-full max-w-6xl items-center gap-6 rounded-2xl border border-subtle bg-surface p-8">
      {room.kind === "channel" && <span className="flex size-16 shrink-0 items-center justify-center rounded-2xl bg--tint text-accent"><Hash size={36} /></span>}
      <div className="min-w-0 flex-1">
        <h4 className="m-0 text-xl font-bold text-primary">{room.kind === "dm" ? strings.chatBeginningDm : strings.chatBeginning(`#${channelLabel(room)}`)}</h4>
        {room.topic !== null && <p className="mb-0 mt-2 text-base text-tertiary">{room.topic}</p>}
      </div>
      {room.kind === "channel" && room.archivedAt === null && <button type="button" className="flex min-h-12 shrink-0 items-center gap-3 rounded-xl border border-subtle bg-surface px-5 text-base font-medium text-secondary hover:bg-raised" onClick={onDescription}><Pencil size={19} />{room.topic === null ? strings.chatAddDescription : strings.chatEditDescription}</button>}
    </div>}
    {moreBehind && messages !== null && <button type="button" className="mx-auto my-3 min-h-9 rounded-full border border-subtle bg-surface px-4 text-sm text-secondary hover:bg-raised disabled:opacity-60" onClick={onOlder} disabled={loadingOlder}>{loadingOlder ? strings.chatLoading : strings.chatOlder}</button>}
    {messages === null ? <p className="m-auto flex items-center gap-2 text-sm text-tertiary"><Loader2 className="animate-spin" size={14} /> {strings.chatLoading}</p> : messages.length === 0 ? <p className="m-auto text-sm text-tertiary">{strings.chatNoMessagesYet}</p> : messages.map((message, index) => <Fragment key={message.id}>
      {readUpTo !== null && message.seq > readUpTo && (messages[index - 1]?.seq ?? 0) <= readUpTo && index > 0 && <div className="my-4 flex items-center gap-3 text-accent before:h-px before:flex-1 before:bg-accent after:h-px after:flex-1 after:bg-accent"><span className="text-xs font-semibold uppercase tracking-wide">{strings.chatNewMessages}</span></div>}
      {(index === 0 || dayOf(message.createdAt) !== dayOf(messages[index - 1]!.createdAt)) && <div className="my-4 flex items-center gap-3 before:h-px before:flex-1 before:bg-subtle after:h-px after:flex-1 after:bg-subtle"><span className="rounded-full border border-subtle bg-surface px-3 py-1 text-xs font-semibold text-tertiary">{dayOf(message.createdAt)}</span></div>}
      <MessageLine message={message} grouped={continues(message, messages[index - 1])} palette={room.archivedAt === null ? palette : []} me={me} onReact={(emoji) => onReact(message, emoji)} onOpenFile={onOpenFile} onDecide={onDecide} onEdit={onEdit} onWithdraw={onWithdraw} onReplyHere={room.archivedAt === null ? onReplyHere : undefined} onReplyPrivate={room.kind !== "dm" && message.authorKind === "user" && message.author !== me ? onReplyPrivate : undefined} />
    </Fragment>)}
  </div>;
}
