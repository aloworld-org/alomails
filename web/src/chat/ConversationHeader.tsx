import { Archive, ChevronLeft, Pencil, UserPlus, Video } from "lucide-react";

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
    <header className="flex min-h-20 shrink-0 items-center gap-4 border-b border-subtle bg-surface px-7">
      <button type="button" className="flex size-10 items-center justify-center rounded-lg border-0 bg-transparent text-primary hover:bg-raised" onClick={onBack} aria-label={strings.chatBackToList}><ChevronLeft size={20} /></button>
      {room.kind === "dm" && <Avatar name={channelLabel(room)} email={room.counterpart ?? undefined} size="md" />}
      <div className="min-w-0 flex-1">
        <h3 className="m-0 truncate text-base font-bold text-primary">{channelLabel(room)}</h3>
        <p className="m-0 flex items-center gap-2 truncate text-xs text-tertiary">{room.topic ?? (room.kind === "dm" ? "Online" : "")}</p>
      </div>
      <div className="flex shrink-0 items-center gap-3">
        <IconButton size="md" label={liveMeeting !== null ? strings.meetJoin : strings.meetStart} icon={<Video size={18} />} onClick={onMeet} active={liveMeeting !== null} />
        <button type="button" className="flex size-10 items-center justify-center rounded-lg border border-subtle bg-surface text-primary hover:bg-raised" onClick={onPeople} title={strings.chatMembersAndAgents}><UserPlus size={18} /><span className="sr-only">{strings.chatMembersAndAgents}</span></button>
        {room.kind === "channel" && room.archivedAt === null && <>
          <IconButton size="md" label={strings.chatRename} icon={<Pencil size={17} />} onClick={onRename} />
          <IconButton size="md" label={strings.chatArchiveAction} icon={<Archive size={17} />} onClick={onArchive} />
        </>}
      </div>
    </header>
  );
}
