import { strings } from "../i18n";
import type { BillingQuoteSummary } from "./types";
import { StatusChip } from "./StatusChip";
import { quoteStatusLabel, quoteStatusTone } from "./statusLogic";

export function QuoteChips({ quote }: { quote: BillingQuoteSummary }) {
  return <><StatusChip tone={quoteStatusTone(quote.status)} label={quoteStatusLabel(quote.status)} />{quote.status === "sent" && quote.expired && <StatusChip tone="warn" label={strings.billingQuoteLapsed} />}</>;
}
