// Admin — Audit log. Who did what, to which target, and when — the tenant's
// administrative actions, newest first. Read-only. Actions on this tenant made
// by a platform operator show up here too (actor "operator").
import { useCallback, useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";

import { strings } from "../i18n";
import { Spinner } from "../ds";
import { useJmapClient } from "../jmap";
import type { AuditEntry } from "../jmap";
import styles from "./admin.module.css";

// Known action keys → a human label. Unknown keys fall back to the key itself,
// so a new action is never invisible, just unstyled.
const ACTION_LABELS: Record<string, string> = {
  "user.create": strings.auditUserCreate,
  "user.delete": strings.auditUserDelete,
  "user.admin": strings.auditUserAdmin,
  "alias.add": strings.auditAliasAdd,
  "alias.remove": strings.auditAliasRemove,
  "group.create": strings.auditGroupCreate,
  "group.delete": strings.auditGroupDelete,
  "group.address": strings.auditGroupAddress,
  "domain.register": strings.auditDomainRegister,
  "domain.verify": strings.auditDomainVerify,
  "domain.delete": strings.auditDomainDelete,
  "tenant.create": strings.auditTenantCreate,
  "tenant.status": strings.auditTenantStatus,
  "tenant.quota": strings.auditTenantQuota,
};

function when(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
}

export function AuditPage() {
  const client = useJmapClient();
  const [entries, setEntries] = useState<AuditEntry[] | null>(null);
  const [error, setError] = useState(false);

  const load = useCallback(() => {
    setError(false);
    setEntries(null);
    client
      .adminAuditLog()
      .then(setEntries)
      .catch(() => setError(true));
  }, [client]);

  useEffect(load, [load]);

  return (
    <div className={styles.page}>
      <header className={styles.pageHead}>
        <div>
          <h1>{strings.adminAudit}</h1>
          <p className={styles.pageIntro}>{strings.adminAuditIntro}</p>
        </div>
        <button type="button" className={styles.ghost} onClick={load}>
          <RefreshCw size={14} />
          <span>{strings.mailRetry}</span>
        </button>
      </header>

      {entries === null && !error && (
        <div className={styles.state}>
          <Spinner size={22} />
        </div>
      )}
      {error && (
        <div className={styles.state}>
          <p>{strings.adminAuditError}</p>
          <button type="button" className={styles.textBtn} onClick={load}>
            {strings.mailRetry}
          </button>
        </div>
      )}

      {entries !== null && entries.length === 0 && (
        <div className={styles.state}>
          <p>{strings.adminAuditEmpty}</p>
        </div>
      )}

      {entries !== null && entries.length > 0 && (
        <ul className={styles.userList}>
          {entries.map((e) => (
            <li key={e.id} className={styles.userRow}>
              <div className={styles.userText}>
                <div className={styles.userName}>
                  <strong>{ACTION_LABELS[e.action] ?? e.action}</strong>
                  {e.target !== null && <span className={styles.pillMuted}>{e.target}</span>}
                </div>
                <div className={styles.userMeta}>
                  {strings.auditBy(e.actor ?? strings.auditUnknownActor)} · {when(e.at)}
                  {e.detail !== null && ` · ${e.detail}`}
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
