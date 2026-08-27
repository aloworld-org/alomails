import { Badge } from "../ds";
import { BADGE_TONE, type ChipTone } from "./statusLogic";

export function StatusChip({ tone, label }: { tone: ChipTone; label: string }) {
  return <Badge tone={BADGE_TONE[tone]}>{label}</Badge>;
}
