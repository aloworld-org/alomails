import { Sparkles } from "lucide-react";

import { strings } from "../i18n";
import type { Turn } from "./types";

export function ActiveTurns({ turns, onStop }: { turns: Turn[]; onStop: (turn: Turn) => void }) {
  if (turns.length === 0) return null;
  return <div className="flex shrink-0 flex-wrap gap-2 border-b border-subtle bg-surface px-4 py-2">
    {turns.map((turn) => <span key={turn.id} className="inline-flex min-h-8 items-center gap-2 rounded-full bg--tint px-3 text-xs text-primary">
      <Sparkles size={13} className="text-accent" />
      <span className="flex gap-1" aria-hidden="true"><i /><i /><i /></span>
      {strings.chatThinking(turn.handle)}
      {turn.mine && <button type="button" className="ml-1 rounded-full border border-subtle bg-transparent px-2 py-1 text-xs text-primary hover:bg-raised" onClick={() => onStop(turn)}>{strings.chatStop}</button>}
    </span>)}
  </div>;
}
