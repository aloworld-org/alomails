// A record's history, as a panel on the record itself (wave B2.13).
//
// The question this answers is always asked from a record and never from a
// console: "who changed this, and when". So it lives beside the record, reads
// only when it is opened, and says nothing at all until it has an answer —
// a history that flashes "nothing yet" while loading tells a lie once per open.
//
// Read-only by construction: there is no verb on this panel, because entries
// exist only because something else happened.
import { useCallback, useEffect, useState } from "react";
import { History } from "lucide-react";

import { strings, useLocale } from "../i18n";
import { auditMessage, useAuditApi } from "./api";
import { actionLabel, actorLabel } from "./label";
import type { AuditEntry } from "./types";
import styles from "./RecordHistory.module.css";

interface Props {
  /** `billing.invoice`, `crm.deal`, … — the server's own record kind. */
  entityType: string;
  entityId: string;
}

export function RecordHistory({ entityType, entityId }: Props) {
  const api = useAuditApi();
  const locale = useLocale();
  const [entries, setEntries] = useState<AuditEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setEntries(await api.history({ entityType, entityId }));
      setError(null);
    } catch (err) {
      setError(auditMessage(err, strings.auditLoadFailed));
    }
  }, [api, entityType, entityId]);

  useEffect(() => {
    void load();
  }, [load]);

  function moment(at: string): string {
    const instant = new Date(at);
    if (Number.isNaN(instant.getTime())) return at;
    return instant.toLocaleString(locale, { dateStyle: "medium", timeStyle: "short" });
  }

  return (
    <section className={styles.history}>
      <h2 className={styles.title}>
        <History size={13} /> {strings.auditHistoryTitle}
      </h2>
      {error !== null && <p className={styles.error}>{error}</p>}
      {error === null && entries !== null && entries.length === 0 && (
        <p className={styles.empty}>{strings.auditHistoryEmpty}</p>
      )}
      {entries !== null && entries.length > 0 && (
        <ol className={styles.entries}>
          {entries.map((entry) => (
            <li key={entry.id} className={styles.entry}>
              <span className={styles.action}>{actionLabel(entry.action, entry.entityType)}</span>
              <span className={styles.actor}>{strings.auditBy(actorLabel(entry.actor))}</span>
              <time className={styles.moment} dateTime={entry.at}>
                {moment(entry.at)}
              </time>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
