import { Bot, Hash, Users } from "lucide-react";

import { strings } from "../i18n";
import { MODAL_BACKDROP_CLASS } from "../ds";
import { channelLabel } from "./presentation";
import type { ChannelSummary } from "./types";

export function ChatSwitcher({ query, rooms, onQuery, onChoose, onClose }: { query: string; rooms: ChannelSummary[]; onQuery: (query: string) => void; onChoose: (id: string) => void; onClose: () => void }) {
  return (
    <div className={`fixed inset-0 z-50 flex items-start justify-center bg-overlay px-4 pt-16 ${MODAL_BACKDROP_CLASS}`} role="dialog" aria-modal="true" aria-label={strings.chatJumpTo} onClick={onClose}>
      <div className="w-full max-w-xl overflow-hidden rounded-xl border border-subtle bg-surface shadow-lg" onClick={(event) => event.stopPropagation()}>
        <input className="min-h-12 w-full border-0 border-b border-subtle bg-transparent px-4 text-base text-primary outline-none placeholder:text-tertiary" value={query} onChange={(event) => onQuery(event.target.value)} placeholder={strings.chatJumpTo} aria-label={strings.chatJumpTo} autoFocus onKeyDown={(event) => {
          if (event.key === "Escape") onClose();
          if (event.key === "Enter" && rooms[0] !== undefined) onChoose(rooms[0].id);
        }} />
        <ul className="m-0 max-h-80 list-none overflow-y-auto p-2">
          {rooms.length === 0 ? <li className="px-3 py-6 text-center text-sm text-tertiary">{strings.chatNoRoom}</li> : rooms.map((room, index) => (
            <li key={room.id}><button type="button" className={`flex min-h-11 w-full items-center gap-2 rounded-md border-0 px-3 text-left text-sm transition-colors hover:bg-accent-soft hover:text-accent ${index === 0 ? "bg-accent-soft text-accent" : "bg-transparent text-secondary"}`} onClick={() => onChoose(room.id)}>{room.kind === "agent_dm" ? <Bot size={15} className="text-accent" /> : room.kind === "dm" ? <Users size={15} className="text-tertiary" /> : <Hash size={15} className="text-tertiary" />}{channelLabel(room)}</button></li>
          ))}
        </ul>
      </div>
    </div>
  );
}
