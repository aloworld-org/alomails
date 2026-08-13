import { Archive, ChevronLeft, Pencil, Users, Video } from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
import type { Meeting } from "../meet";
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

export function ConversationHeader({ room, mobile, liveMeeting, onBack, onMeet, onPeople, onRename, onArchive }: Props) {
  return (
    <header className="flex min-h-16 shrink-0 items-center gap-3 border-b border-subtle bg-surface px-4">
      {mobile && <button type="button" className="flex size-10 items-center justify-center rounded-md border-0 bg-transparent text-primary hover:bg-raised" onClick={onBack} aria-label={strings.chatBackToList}><ChevronLeft size={18} /></button>}
      <div className="min-w-0 flex-1">
        <h3 className="m-0 truncate text-base font-bold text-primary">{channelLabel(room)}</h3>
        {room.topic !== null && <p className="m-0 truncate text-xs text-tertiary">{room.topic}</p>}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <Button variant="secondary" size="sm" icon={<Video size={15} />} onClick={onMeet} title={liveMeeting !== null ? strings.meetJoin : strings.meetStart}>{liveMeeting !== null ? strings.meetLive : strings.meetStart}</Button>
        <Button variant="ghost" size="sm" icon={<Users size={15} />} onClick={onPeople} title={strings.chatMembersAndAgents}>{strings.chatMembersAndAgents}</Button>
        {room.kind === "channel" && room.archivedAt === null && <>
          <button type="button" className="flex size-9 items-center justify-center rounded-md border-0 bg-transparent text-tertiary hover:bg-raised hover:text-primary" onClick={onRename} aria-label={strings.chatRename} title={strings.chatRename}><Pencil size={15} /></button>
          <button type="button" className="flex size-9 items-center justify-center rounded-md border-0 bg-transparent text-tertiary hover:bg-raised hover:text-primary" onClick={onArchive} aria-label={strings.chatArchiveAction} title={strings.chatArchiveAction}><Archive size={15} /></button>
        </>}
      </div>
    </header>
  );
}
