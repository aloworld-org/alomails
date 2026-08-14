import { ChevronDown, ChevronLeft, Hash, MoreHorizontal, Pencil, UserPlus, Video } from "lucide-react";

import { Avatar, IconButton } from "../ds";
import { strings } from "../i18n";
import type { Meeting } from "../meet/api";
import { channelLabel } from "./presentation";
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
  return (
    <header className="flex min-h-[8.5rem] shrink-0 items-center gap-5 border-b border-subtle bg-surface px-12">
      <button type="button" className="flex size-16 items-center justify-center rounded-2xl border border-subtle bg-surface text-primary hover:bg-raised" onClick={onBack} aria-label={strings.chatBackToList}><ChevronLeft size={27} /></button>
      {room.kind === "dm" && <Avatar name={channelLabel(room)} email={room.counterpart ?? undefined} size="md" />}
      {room.kind === "channel" && <Hash size={38} strokeWidth={1.8} className="shrink-0 text-primary" />}
      <div className="min-w-0 flex-1">
        <h3 className="m-0 flex items-center gap-2 truncate text-2xl font-bold text-primary">{channelLabel(room)}{room.kind === "channel" && <ChevronDown size={20} />}</h3>
        <p className="mb-0 mt-1 flex items-center gap-2 truncate text-base text-tertiary">{room.topic ?? (room.kind === "dm" ? "Online" : "")}</p>
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
