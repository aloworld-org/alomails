import { strings } from "../i18n";
import { StatusChip } from "./status";
import type { BillingScheduleSummary } from "./types";

export function ScheduleChips({ schedule }: { schedule: BillingScheduleSummary }) {
  if (!schedule.active) return <StatusChip tone="muted" label={strings.billingScheduleStatusPaused} />;
  if (schedule.ended) return <StatusChip tone="muted" label={strings.billingScheduleStatusEnded} />;
  return <><StatusChip tone="good" label={strings.billingScheduleStatusActive} />{schedule.due && <StatusChip tone="warn" label={strings.billingScheduleStatusDue} />}</>;
}
