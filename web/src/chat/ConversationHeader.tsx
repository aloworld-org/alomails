import { Archive, Bot, ChevronDown, ChevronLeft, Hash, MoreHorizontal, Pencil, UserPlus, Video } from "lucide-react";

import { Avatar, IconButton } from "../ds";
import { strings } from "../i18n";
import type { Meeting } from "../meet/api";
import { channelLabel, directMessageName } from "./presentation";
import type { ChannelSummary } from "./types";

type Props = {
  room: ChannelSummary;
  mobile: boolean;
  liveMeeting: Meeting | null;
  onBack: () => void;
  onMeet: () => void;
  onPeople: () => void;
  onRename: () => void;
  onArchive: () => void;
};

// Phone widths compact the chrome — tighter padding, smaller controls, the
// decorative dividers and the # glyph dropped — so the room's NAME keeps the
// remaining width. At 360px the desktop paddings alone left the title 0px.
export function ConversationHeader({ room, mobile, liveMeeting, onBack, onMeet, onPeople, onRename, onArchive }: Props) {
  if (room.kind === "dm" || room.kind === "agent_dm") {
    const name = directMessageName(room);
    return (
      <header className="shrink-0 bg-surface px-6 py-4 max-md:px-3 max-md:py-2">
        <div className="flex min-h-24 items-center gap-5 rounded-2xl border border-subtle bg-surface px-6 shadow-sm max-md:min-h-16 max-md:gap-2 max-md:px-3">
          <button type="button" className="flex size-11 shrink-0 items-center justify-center rounded-xl border-0 bg-transparent text-primary hover:bg-raised" onClick={onBack} aria-label={strings.chatBackToList}><ChevronLeft size={23} /></button>
          {room.kind === "agent_dm" ? <span className="flex size-12 shrink-0 items-center justify-center rounded-xl bg--tint text-accent max-md:size-9"><Bot size={23} /></span> : <Avatar name={name} email={room.counterpart ?? undefined} size={mobile ? "md" : "lg"} />}
          <div className="min-w-0 flex-1">
            <h3 className="m-0 flex items-center gap-2 truncate text-lg font-bold text-primary">{name}{room.kind === "agent_dm" && <span className="rounded border border-subtle px-1.5 text-[0.65rem] font-bold uppercase tracking-wide text-accent">{strings.chatAgentTag}</span>}</h3>
            <p className="mb-0 mt-0.5 truncate text-sm text-tertiary">{room.counterpart}</p>
          </div>
          <div className="flex shrink-0 items-center gap-3 max-md:gap-1">
            <button type="button" className={`flex size-11 items-center justify-center rounded-lg border-0 bg-transparent text-primary transition-colors hover:bg-raised max-md:size-9 ${liveMeeting !== null ? "text-accent" : ""}`} onClick={onMeet} aria-label={liveMeeting !== null ? strings.meetJoin : strings.meetStart} title={liveMeeting !== null ? strings.meetJoin : strings.meetStart}><Video size={22} strokeWidth={1.9} /></button>
            <span className="block h-10 w-px shrink-0 bg-[#ded5ca] max-md:hidden" aria-hidden="true" />
            <button type="button" className="flex size-11 items-center justify-center rounded-lg border-0 bg-transparent text-primary transition-colors hover:bg-raised max-md:size-9" onClick={onPeople} title={strings.chatMembersAndAgents}><UserPlus size={23} strokeWidth={1.9} /><span className="sr-only">{strings.chatMembersAndAgents}</span></button>
            <span className="block h-10 w-px shrink-0 bg-[#ded5ca] max-md:hidden" aria-hidden="true" />
            <button type="button" className="flex size-11 items-center justify-center rounded-lg border-0 bg-transparent text-primary transition-colors hover:bg-raised max-md:size-9" onClick={onArchive} aria-label={strings.chatArchiveAction} title={strings.chatArchiveAction}><Archive size={22} strokeWidth={1.9} /></button>
          </div>
        </div>
      </header>
    );
  }

  const squareButton = mobile
    ? "flex size-11 items-center justify-center rounded-xl border border-subtle bg-surface text-primary hover:bg-raised"
    : "flex size-16 items-center justify-center rounded-2xl border border-subtle bg-surface text-primary hover:bg-raised";
  const squareIcon = mobile ? "!size-11 !rounded-xl" : "!size-16 !rounded-2xl";
  return (
    <header className={mobile ? "flex min-h-20 shrink-0 items-center gap-2 border-b border-subtle bg-surface px-3" : "flex min-h-[8.5rem] shrink-0 items-center gap-5 border-b border-subtle bg-surface px-12"}>
      <button type="button" className={squareButton} onClick={onBack} aria-label={strings.chatBackToList}><ChevronLeft size={mobile ? 23 : 27} /></button>
      {!mobile && <Hash size={38} strokeWidth={1.8} className="shrink-0 text-primary" />}
      <div className="min-w-0 flex-1">
        <h3 className={`m-0 flex items-center gap-2 truncate font-bold text-primary ${mobile ? "text-lg" : "text-2xl"}`}>{channelLabel(room)}<ChevronDown size={20} /></h3>
        <p className={`mb-0 mt-1 flex items-center gap-2 truncate text-tertiary ${mobile ? "text-sm" : "text-base"}`}>{room.topic ?? ""}</p>
      </div>
      <div className={`flex shrink-0 items-center ${mobile ? "gap-1" : "gap-3"}`}>
        <IconButton size="md" className={squareIcon} label={liveMeeting !== null ? strings.meetJoin : strings.meetStart} icon={<Video size={22} />} onClick={onMeet} active={liveMeeting !== null} />
        <button type="button" className={squareButton} onClick={onPeople} title={strings.chatMembersAndAgents}><UserPlus size={24} /><span className="sr-only">{strings.chatMembersAndAgents}</span></button>
        {room.kind === "channel" && room.archivedAt === null && <>
          <IconButton size="md" className={squareIcon} label={strings.chatRename} icon={<Pencil size={22} />} onClick={onRename} />
          <IconButton size="md" className={squareIcon} label={strings.chatArchiveAction} icon={<MoreHorizontal size={24} />} onClick={onArchive} />
        </>}
      </div>
    </header>
  );
}
