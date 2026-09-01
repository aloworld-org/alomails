import { AlertCircle, ArrowRight, CalendarClock, CircleDot, History } from "lucide-react";

import { Card } from "../ds";
import { strings } from "../i18n";
import { dayLabel } from "./format";
import { dealAttention, salesFocus } from "./salesFocus";
import type { CrmDeal } from "./types";

interface Props {
  deals: CrmDeal[];
  onOpen: (id: string) => void;
}

export function SalesFocusPanel({ deals, onOpen }: Props) {
  const now = new Date();
  const focus = salesFocus(deals, now);
  const metrics = [
    { Icon: CircleDot, label: strings.crmFocusOpen, value: focus.open.length },
    { Icon: CalendarClock, label: strings.crmFocusClosingSoon, value: focus.closingSoon.length },
    { Icon: AlertCircle, label: strings.crmFocusOverdue, value: focus.overdue.length },
    { Icon: History, label: strings.crmFocusQuiet, value: focus.quiet.length },
  ];

  return (
    <Card as="section" pad="none" className="overflow-hidden" aria-labelledby="sales-focus-title">
      <div className="flex items-start justify-between gap-6 border-b border-subtle px-6 py-5 max-md:flex-col max-md:gap-2">
        <div>
          <p className="m-0 text-xs font-semibold uppercase tracking-wide text-accent">{strings.crmFocusEyebrow}</p>
          <h2 id="sales-focus-title" className="mb-0 mt-1 text-lg font-semibold text-primary">{strings.crmFocusTitle}</h2>
        </div>
        <p className="m-0 max-w-xl text-right text-sm leading-5 text-secondary max-md:text-left">{strings.crmFocusHint}</p>
      </div>
      <div className="grid grid-cols-4 divide-x divide-subtle max-md:grid-cols-2 max-md:divide-x-0">
        {metrics.map(({ Icon, label, value }, index) => (
          <div className={`flex min-h-24 items-center gap-4 px-6 py-4 max-md:border-b max-md:border-subtle ${index % 2 === 0 ? "max-md:border-r" : ""}`} key={label}>
            <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent" aria-hidden="true"><Icon size={18} /></span>
            <span className="min-w-0">
              <strong className="block text-xl font-semibold tabular-nums text-primary">{value}</strong>
              <span className="mt-1 block text-xs leading-4 text-secondary">{label}</span>
            </span>
          </div>
        ))}
      </div>
      {focus.attention.length > 0 && (
        <div className="border-t border-subtle bg-raised/40 px-6 py-4">
          <div className="mb-3 flex items-center justify-between gap-4">
            <strong className="text-sm font-semibold text-primary">{strings.crmAttentionTitle}</strong>
            <span className="text-xs text-secondary">{strings.crmAttentionCount(focus.attention.length)}</span>
          </div>
          <div className="grid grid-cols-3 gap-3 max-lg:grid-cols-1">
            {focus.attention.slice(0, 3).map((deal) => {
              const attention = dealAttention(deal, now);
              return (
                <button
                  type="button"
                  className="grid min-h-14 grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-3 rounded-xl border border-subtle bg-surface !px-4 !py-3 text-left transition-[border-color,box-shadow] hover:border-default hover:shadow-sm focus-visible:outline-2 focus-visible:outline-accent"
                  key={deal.id}
                  aria-label={strings.crmAttentionOpen(deal.title)}
                  onClick={() => onOpen(deal.id)}
                >
                  <span className="min-w-0">
                    <strong className="block truncate text-sm font-semibold text-primary">{deal.companyName || deal.contactName}</strong>
                    <small className="mt-1 block truncate text-xs text-secondary">{deal.contactName || deal.contactEmail}</small>
                  </span>
                  <span className={`text-xs font-medium ${attention === "overdue" ? "text-danger" : "text-warning"}`}>
                    {attention === "overdue" && deal.expectedClose !== null ? strings.crmAttentionOverdue(dayLabel(deal.expectedClose)) : strings.crmAttentionQuiet}
                  </span>
                  <ArrowRight size={16} className="text-tertiary" aria-hidden="true" />
                </button>
              );
            })}
          </div>
        </div>
      )}
    </Card>
  );
}
