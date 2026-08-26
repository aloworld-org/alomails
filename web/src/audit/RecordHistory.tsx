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

interface Props {
  /** `billing.invoice`, `crm.deal`, … — the server's own record kind. */
  entityType: string;
  entityId: string;
  /** Optional internal context that belongs with the audit trail, not the document. */
  note?: string | undefined;
}

export function RecordHistory({ entityType, entityId, note }: Props) {
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
    <section className="flex flex-col gap-2">
      <h2 className="m-0 flex items-center gap-1 text-sm font-semibold uppercase tracking-[0.04em] text-tertiary">
        <History size={13} /> {strings.auditHistoryTitle}
      </h2>
      {note !== undefined && <p className="m-0 text-sm text-secondary">{note}</p>}
      {error !== null && <p className="m-0 text-sm text-danger">{error}</p>}
      {note === undefined && error === null && entries !== null && entries.length === 0 && (
        <p className="m-0 text-sm text-tertiary">{strings.auditHistoryEmpty}</p>
      )}
      {entries !== null && entries.length > 0 && (
        <ol className="m-0 flex list-none flex-col gap-1 p-0">
          {entries.map((entry) => (
            <li key={entry.id} className="flex flex-wrap items-baseline gap-x-2 gap-y-1 text-sm text-secondary">
              <span className="font-medium text-primary">
                {actionLabel(entry.action, entry.entityType)}
              </span>
              <span className="break-words text-secondary">
                {strings.auditBy(actorLabel(entry.actor))}
              </span>
              <time className="ml-auto whitespace-nowrap text-tertiary" dateTime={entry.at}>
                {moment(entry.at)}
              </time>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
