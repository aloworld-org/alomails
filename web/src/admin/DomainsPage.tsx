// Admin — Domains. This tenant's domains and their verification state. Add a
// domain, publish the shown DNS TXT record, then verify — a domain must be
// verified before the tenant can assign addresses in it (ADR 0012). All calls
// are tenant-scoped on the server; a tenant admin can only touch its own.
import { useCallback, useEffect, useState } from "react";
import type { FormEvent } from "react";
import { CheckCircle2, Clock, Plus, Trash2 } from "lucide-react";

import { strings } from "../i18n";
import { Button, Input, Spinner, cx, useDialogs } from "../ds";
import { useJmapClient } from "../jmap";
import type { ControlDomain } from "../jmap";
import styles from "./admin.module.css";

export function DomainsPage() {
  const { confirm } = useDialogs();
  const client = useJmapClient();
  const [domains, setDomains] = useState<ControlDomain[] | null>(null);
  const [error, setError] = useState(false);
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const load = useCallback(() => {
    setError(false);
    client
      .adminListDomains()
      .then(setDomains)
      .catch(() => setError(true));
  }, [client]);

  useEffect(load, [load]);

  async function register(e: FormEvent) {
    e.preventDefault();
    if (!draft.includes(".")) {
      setNote(strings.domainInvalid);
      return;
    }
    setBusy(true);
    setNote(null);
    try {
      await client.adminCreateDomain(draft.trim().toLowerCase());
      setDraft("");
      setAdding(false);
      load();
    } catch {
      setNote(strings.domainCreateError);
    } finally {
      setBusy(false);
    }
  }

  async function verify(d: ControlDomain) {
    setNote(null);
    try {
      const res = await client.adminVerifyDomain(d.domain);
      setNote(res.verified ? strings.domainVerifiedOk(d.domain) : strings.domainVerifyPending(d.domain));
    } catch {
      setNote(strings.domainActionError);
    } finally {
      load();
    }
  }

  async function remove(d: ControlDomain) {
    if (!(await confirm({ message: strings.domainDeleteConfirm(d.domain), danger: true }))) return;
    try {
      await client.adminDeleteDomain(d.domain);
    } finally {
      load();
    }
  }

  async function rotateDkim(d: ControlDomain) {
    if (!(await confirm({ message: strings.dkimRotateConfirm(d.domain), danger: true }))) return;
    setNote(null);
    try {
      await client.adminRotateDkim(d.domain);
      setNote(strings.dkimRotated(d.domain));
    } catch {
      setNote(strings.domainActionError);
    } finally {
      load();
    }
  }

  return (
    <div className={styles.page}>
      <header className={styles.pageHead}>
        <div>
          <h1>{strings.adminDomains}</h1>
          <p className={styles.pageIntro}>{strings.adminDomainsIntro}</p>
        </div>
        <button type="button" className={styles.primary} onClick={() => setAdding((v) => !v)}>
          <Plus size={16} />
          <span>{strings.adminAddDomain}</span>
        </button>
      </header>

      {adding && (
        <form className={styles.keyRow} onSubmit={register}>
          <Input
            className={styles.keyRowGrow}
            aria-label={strings.adminAddDomain}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="customer.example"
            autoFocus
          />
          <Button type="submit" disabled={busy}>
            {busy ? <Spinner size={16} /> : strings.domainRegister}
          </Button>
        </form>
      )}

      {note !== null && <p className={styles.checksFor}>{note}</p>}

      {domains === null && !error && (
        <div className={styles.state}>
          <Spinner size={22} />
        </div>
      )}
      {error && (
        <div className={styles.state}>
          <p>{strings.adminDomainsError}</p>
          <button type="button" className={styles.textBtn} onClick={load}>
            {strings.mailRetry}
          </button>
        </div>
      )}

      {domains !== null && domains.length === 0 && !adding && (
        <div className={styles.state}>
          <p>{strings.adminDomainsEmpty}</p>
        </div>
      )}

      {domains !== null && domains.length > 0 && (
        <ul className={styles.checkList}>
          {domains.map((d) => (
            <li key={d.domain} className={styles.checkRow}>
              <span className={cx(styles.checkIcon, d.verified ? styles.chkPass : styles.chkWarn)}>
                {d.verified ? <CheckCircle2 size={20} /> : <Clock size={20} />}
              </span>
              <div className={styles.checkText}>
                <div className={styles.checkTitle}>
                  <strong>{d.domain}</strong>
                  <span
                    className={cx(styles.checkBadge, d.verified ? styles.chkPass : styles.chkWarn)}
                  >
                    {d.verified ? strings.domainVerified : strings.domainUnverified}
                  </span>
                </div>
                {!d.verified && (
                  <p className={styles.checkDetail}>
                    {strings.domainPublishIntro(d.domain)} — {d.verifyRecord.name} TXT ={" "}
                    {d.verifyRecord.value}
                  </p>
                )}
                {d.verified && d.dkim != null && (
                  <p className={styles.checkDetail}>
                    {strings.dkimPublish} — {d.dkim.name} TXT = {d.dkim.value}
                  </p>
                )}
              </div>
              <div className={styles.userActions}>
                {!d.verified && (
                  <button type="button" className={styles.ghost} onClick={() => void verify(d)}>
                    {strings.domainVerify}
                  </button>
                )}
                {d.verified && d.dkim != null && (
                  <button type="button" className={styles.ghost} onClick={() => void rotateDkim(d)}>
                    {strings.dkimRotate}
                  </button>
                )}
                <button
                  type="button"
                  className={styles.iconBtn}
                  onClick={() => void remove(d)}
                  aria-label={strings.domainDelete}
                >
                  <Trash2 size={16} />
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
