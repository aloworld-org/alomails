// Admin — Security & trust. Runs live deliverability checks (SPF, DKIM, DMARC,
// MX, reverse DNS, MTA-STS) against the mail domain and shows pass / attention /
// action-needed with what to fix. Every run is a real DNS + HTTPS query.
import { useCallback, useEffect, useState } from "react";
import { AlertTriangle, CheckCircle2, RefreshCw, XCircle } from "lucide-react";

import { strings } from "../i18n";
import { Spinner, cx } from "../ds";
import { useJmapClient } from "../jmap";
import type { SecurityCheck } from "../jmap";
import styles from "./admin.module.css";

const STATUS = {
  pass: { Icon: CheckCircle2, cls: "chkPass", label: strings.securityPass },
  warn: { Icon: AlertTriangle, cls: "chkWarn", label: strings.securityWarn },
  fail: { Icon: XCircle, cls: "chkFail", label: strings.securityFail },
} as const;

export function SecurityPage() {
  const client = useJmapClient();
  const [data, setData] = useState<{ domain: string; checks: SecurityCheck[] } | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);

  const run = useCallback(() => {
    setLoading(true);
    setError(false);
    client
      .securityChecks()
      .then(setData)
      .catch(() => setError(true))
      .finally(() => setLoading(false));
  }, [client]);

  useEffect(run, [run]);

  return (
    <div className={styles.page}>
      <header className={styles.pageHead}>
        <div>
          <h1>{strings.adminSecurity}</h1>
          <p className={styles.pageIntro}>{strings.adminSecurityIntro}</p>
        </div>
        <button type="button" className={styles.ghost} onClick={run} disabled={loading}>
          {loading ? <Spinner size={14} /> : <RefreshCw size={14} />}
          <span>{strings.securityRecheck}</span>
        </button>
      </header>

      {loading && data === null && (
        <div className={styles.state}>
          <Spinner size={22} />
          <p>{strings.securityChecking}</p>
        </div>
      )}
      {error && (
        <div className={styles.state}>
          <p>{strings.securityError}</p>
          <button type="button" className={styles.textBtn} onClick={run}>
            {strings.mailRetry}
          </button>
        </div>
      )}

      {data !== null && (
        <>
          <p className={styles.checksFor}>{strings.securityFor(data.domain)}</p>
          <ul className={styles.checkList}>
            {data.checks.map((c) => {
              const s = STATUS[c.status];
              return (
                <li key={c.key} className={styles.checkRow}>
                  <span className={cx(styles.checkIcon, styles[s.cls])}>
                    <s.Icon size={20} />
                  </span>
                  <div className={styles.checkText}>
                    <div className={styles.checkTitle}>
                      <strong>{c.title}</strong>
                      <span className={cx(styles.checkBadge, styles[s.cls])}>{s.label}</span>
                    </div>
                    <p className={styles.checkDetail}>{c.detail}</p>
                  </div>
                </li>
              );
            })}
          </ul>
        </>
      )}
    </div>
  );
}
