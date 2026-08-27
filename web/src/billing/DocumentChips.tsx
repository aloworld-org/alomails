import { strings } from "../i18n";
import type { BillingInvoiceSummary } from "./types";
import { StatusChip } from "./StatusChip";
import { statusLabel, statusTone } from "./statusLogic";

export function DocumentChips({ invoice }: { invoice: BillingInvoiceSummary }) {
  return <>{invoice.creditNote && <StatusChip tone="warn" label={strings.billingCreditNote} />}<StatusChip tone={statusTone(invoice.status)} label={statusLabel(invoice.status)} />{invoice.overdue && <StatusChip tone="warn" label={strings.billingStatusOverdue} />}{invoice.scheduleId !== null && <StatusChip tone="info" label={strings.billingRecurringChip} />}</>;
}
