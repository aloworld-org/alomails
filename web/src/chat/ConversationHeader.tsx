import { Archive, ChevronDown, ChevronLeft, Hash, MoreHorizontal, Pencil, UserPlus, Video } from "lucide-react";

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

export function ConversationHeader({ room, liveMeeting, onBack, onMeet, onPeople, onRename, onArchive }: Props) {
  if (room.kind === "dm") {
    const name = directMessageName(room);
    return (
      <header className="shrink-0 bg-surface px-6 py-4">
        <div className="flex min-h-24 items-center gap-5 rounded-2xl border border-subtle bg-surface px-6 shadow-sm">
          <button type="button" className="flex size-11 shrink-0 items-center justify-center rounded-xl border-0 bg-transparent text-primary hover:bg-raised" onClick={onBack} aria-label={strings.chatBackToList}><ChevronLeft size={23} /></button>
          <Avatar name={name} email={room.counterpart ?? undefined} size="lg" />
          <div className="min-w-0 flex-1">
            <h3 className="m-0 truncate text-lg font-bold text-primary">{name}</h3>
            <p className="mb-0 mt-0.5 truncate text-sm text-tertiary">{room.counterpart}</p>
          </div>
          <div className="flex shrink-0 items-center gap-3">
            <button type="button" className={`flex size-11 items-center justify-center rounded-lg border-0 bg-transparent text-primary transition-colors hover:bg-raised ${liveMeeting !== null ? "text-accent" : ""}`} onClick={onMeet} aria-label={liveMeeting !== null ? strings.meetJoin : strings.meetStart} title={liveMeeting !== null ? strings.meetJoin : strings.meetStart}><Video size={22} strokeWidth={1.9} /></button>
            <span className="block h-10 w-px shrink-0 bg-[#ded5ca]" aria-hidden="true" />
            <button type="button" className="flex size-11 items-center justify-center rounded-lg border-0 bg-transparent text-primary transition-colors hover:bg-raised" onClick={onPeople} title={strings.chatMembersAndAgents}><UserPlus size={23} strokeWidth={1.9} /><span className="sr-only">{strings.chatMembersAndAgents}</span></button>
            <span className="block h-10 w-px shrink-0 bg-[#ded5ca]" aria-hidden="true" />
            <button type="button" className="flex size-11 items-center justify-center rounded-lg border-0 bg-transparent text-primary transition-colors hover:bg-raised" onClick={onArchive} aria-label={strings.chatArchiveAction} title={strings.chatArchiveAction}><Archive size={22} strokeWidth={1.9} /></button>
          </div>
        </div>
      </header>
    );
  }

  return (
    <header className="flex min-h-[8.5rem] shrink-0 items-center gap-5 border-b border-subtle bg-surface px-12">
      <button type="button" className="flex size-16 items-center justify-center rounded-2xl border border-subtle bg-surface text-primary hover:bg-raised" onClick={onBack} aria-label={strings.chatBackToList}><ChevronLeft size={27} /></button>
      <Hash size={38} strokeWidth={1.8} className="shrink-0 text-primary" />
      <div className="min-w-0 flex-1">
        <h3 className="m-0 flex items-center gap-2 truncate text-2xl font-bold text-primary">{channelLabel(room)}<ChevronDown size={20} /></h3>
        <p className="mb-0 mt-1 flex items-center gap-2 truncate text-base text-tertiary">{room.topic ?? ""}</p>
      </div>
      <div className="flex shrink-0 items-center gap-3">
        <IconButton size="md" className="!size-16 !rounded-2xl" label={liveMeeting !== null ? strings.meetJoin : strings.meetStart} icon={<Video size={22} />} onClick={onMeet} active={liveMeeting !== null} />
        <button type="button" className="flex size-16 items-center justify-center rounded-2xl border border-subtle bg-surface text-primary hover:bg-raised" onClick={onPeople} title={strings.chatMembersAndAgents}><UserPlus size={24} /><span className="sr-only">{strings.chatMembersAndAgents}</span></button>
        {room.kind === "channel" && room.archivedAt === null && <>
          <IconButton size="md" className="!size-16 !rounded-2xl" label={strings.chatRename} icon={<Pencil size={22} />} onClick={onRename} />
          <IconButton size="md" className="!size-16 !rounded-2xl" label={strings.chatArchiveAction} icon={<MoreHorizontal size={24} />} onClick={onArchive} />
        </>}
      </div>
    </header>
  );
}
