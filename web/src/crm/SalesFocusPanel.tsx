import { AlertCircle, ArrowRight, CalendarClock, CircleDot, History } from "lucide-react";

import { strings } from "../i18n";
import { dayLabel } from "./format";
import { dealAttention, salesFocus } from "./salesFocus";
import type { CrmDeal } from "./types";
import styles from "./CrmModule.module.css";

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
    <section className={styles.focusPanel} aria-labelledby="sales-focus-title">
      <div className={styles.focusHeading}>
        <div>
          <p className={styles.focusEyebrow}>{strings.crmFocusEyebrow}</p>
          <h2 id="sales-focus-title" className={styles.focusTitle}>{strings.crmFocusTitle}</h2>
        </div>
        <p className={styles.focusHint}>{strings.crmFocusHint}</p>
      </div>
      <div className={styles.focusMetrics}>
        {metrics.map(({ Icon, label, value }) => (
          <div className={styles.focusMetric} key={label}>
            <span className={styles.focusMetricIcon}><Icon size={16} /></span>
            <span className={styles.focusMetricValue}>{value}</span>
            <span className={styles.focusMetricLabel}>{label}</span>
          </div>
        ))}
      </div>
      {focus.attention.length > 0 && (
        <div className={styles.attentionQueue}>
          <div className={styles.attentionHead}>
            <strong>{strings.crmAttentionTitle}</strong>
            <span>{strings.crmAttentionCount(focus.attention.length)}</span>
          </div>
          <div className={styles.attentionDeals}>
            {focus.attention.slice(0, 3).map((deal) => {
              const attention = dealAttention(deal, now);
              return (
                <button
                  type="button"
                  className={styles.attentionDeal}
                  key={deal.id}
                  aria-label={strings.crmAttentionOpen(deal.title)}
                  onClick={() => onOpen(deal.id)}
                >
                  <span>
                    <strong>{deal.companyName || deal.contactName}</strong>
                    <small>{deal.contactName || deal.contactEmail}</small>
                  </span>
                  <span className={attention === "overdue" ? styles.attentionDanger : styles.attentionQuiet}>
                    {attention === "overdue" && deal.expectedClose !== null
                      ? strings.crmAttentionOverdue(dayLabel(deal.expectedClose))
                      : strings.crmAttentionQuiet}
                  </span>
                  <ArrowRight size={16} />
                </button>
              );
            })}
          </div>
        </div>
      )}
    </section>
  );
}
