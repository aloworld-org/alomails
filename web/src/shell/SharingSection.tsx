// Self-service mailbox sharing (ADR 0017) — the Gmail-style "grant access to
// your account" surface inside Settings. A user lists who can access their own
// mailbox, adds a colleague by email with an access level (read-only / manage),
// a send mode (none / send-as / send-on-behalf), and an optional per-folder
// restriction (whole mailbox, or only chosen folders). No admin needed; the
// server always treats the caller as the owner.
import { useCallback, useEffect, useState } from "react";
import type { FormEvent } from "react";
import { FolderCog, X } from "lucide-react";

import { strings } from "../i18n";
import { Spinner } from "../ds";
import { useJmapClient } from "../jmap";
import type { Delegate, Mailbox, SendMode } from "../jmap";
import styles from "./SharingSection.module.css";

export function SharingSection() {
  const client = useJmapClient();
  const [delegates, setDelegates] = useState<Delegate[] | null>(null);
  const [folders, setFolders] = useState<Mailbox[]>([]);
  const [email, setEmail] = useState("");
  const [canWrite, setCanWrite] = useState(true);
  const [sendMode, setSendMode] = useState<SendMode>("none");
  // The add form's folder scope: empty set = whole mailbox.
  const [scope, setScope] = useState<ReadonlySet<string>>(new Set());
  const [limitFolders, setLimitFolders] = useState(false);
  // Which existing delegate's folder scope is being edited inline.
  const [editing, setEditing] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    setError(null);
    void client
      .myDelegates()
      .then(setDelegates)
      .catch(() => setError(strings.delegateError));
    // The owner's own folders, regardless of any active shared-mailbox selection.
    void client
      .ownAccountId()
      .then((id) => client.mailboxesFor(id))
      .then(setFolders)
      .catch(() => undefined);
  }, [client]);
  useEffect(load, [load]);

  const folderName = (id: string) =>
    folders.find((f) => f.id === id)?.name ?? id;

  async function share(
    e: string,
    w: boolean,
    s: SendMode,
    folderIds: string[],
  ): Promise<void> {
    setBusy(true);
    try {
      await client.shareMyMailbox(e, w, s, folderIds);
      load();
    } catch {
      setError(strings.sharingAddError);
    } finally {
      setBusy(false);
    }
  }

  async function add(e: FormEvent) {
    e.preventDefault();
    if (email.trim().length === 0 || busy) return;
    await share(
      email.trim(),
      canWrite,
      sendMode,
      limitFolders ? [...scope] : [],
    );
    setEmail("");
    setCanWrite(true);
    setSendMode("none");
    setScope(new Set());
    setLimitFolders(false);
  }

  async function remove(id: string) {
    setBusy(true);
    try {
      await client.unshareMyMailbox(id);
      load();
    } catch {
      setError(strings.delegateError);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={styles.wrap}>
      {delegates === null ? (
        <Spinner size={18} />
      ) : delegates.length === 0 ? (
        <p className={styles.none}>{strings.sharingNone}</p>
      ) : (
        <ul className={styles.list}>
          {delegates.map((d) => (
            <li key={d.id} className={styles.row}>
              <div className={styles.rowMain}>
                <span className={styles.email}>{d.email}</span>
                <Selects
                  canWrite={d.canWrite}
                  sendMode={d.sendMode}
                  disabled={busy}
                  onChange={(w, s) => void share(d.email, w, s, d.folders)}
                />
                <button
                  type="button"
                  className={styles.folderBtn}
                  onClick={() => setEditing(editing === d.id ? null : d.id)}
                  aria-label={strings.delegateFoldersLabel}
                  title={strings.delegateFoldersLabel}
                >
                  <FolderCog size={15} />
                </button>
                <button
                  type="button"
                  className={styles.remove}
                  onClick={() => void remove(d.id)}
                  aria-label={strings.delegateRemove}
                >
                  <X size={16} />
                </button>
              </div>
              <span className={styles.scopeLabel}>
                {d.folders.length === 0
                  ? strings.delegateWholeMailbox
                  : d.folders.map(folderName).join(", ")}
              </span>
              {editing === d.id && (
                <FolderScope
                  folders={folders}
                  selected={new Set(d.folders)}
                  disabled={busy}
                  onSave={(ids) => {
                    setEditing(null);
                    void share(d.email, d.canWrite, d.sendMode, ids);
                  }}
                  onCancel={() => setEditing(null)}
                />
              )}
            </li>
          ))}
        </ul>
      )}

      <form className={styles.addRow} onSubmit={add}>
        <input
          className={styles.input}
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          placeholder={strings.sharingEmailPlaceholder}
          aria-label={strings.sharingEmailPlaceholder}
        />
        <Selects
          canWrite={canWrite}
          sendMode={sendMode}
          disabled={busy}
          onChange={(w, s) => {
            setCanWrite(w);
            setSendMode(s);
          }}
        />
        <button
          type="submit"
          className={styles.add}
          disabled={busy || email.trim().length === 0}
        >
          {strings.sharingAdd}
        </button>
      </form>

      {folders.length > 0 && (
        <label className={styles.limitToggle}>
          <input
            type="checkbox"
            checked={limitFolders}
            onChange={(e) => setLimitFolders(e.target.checked)}
          />
          {strings.delegateLimitFolders}
        </label>
      )}
      {limitFolders && (
        <div className={styles.checklist}>
          {folders.map((f) => (
            <label key={f.id} className={styles.check}>
              <input
                type="checkbox"
                checked={scope.has(f.id)}
                onChange={() =>
                  setScope((prev) => {
                    const next = new Set(prev);
                    if (next.has(f.id)) next.delete(f.id);
                    else next.add(f.id);
                    return next;
                  })
                }
              />
              {f.name}
            </label>
          ))}
        </div>
      )}

      {error !== null && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

/** An inline folder multi-select for editing an existing grant's scope. Saving
 * an empty selection clears the restriction back to whole-mailbox. */
function FolderScope({
  folders,
  selected,
  disabled,
  onSave,
  onCancel,
}: {
  folders: Mailbox[];
  selected: ReadonlySet<string>;
  disabled: boolean;
  onSave: (ids: string[]) => void;
  onCancel: () => void;
}) {
  const [set, setSet] = useState<ReadonlySet<string>>(selected);
  return (
    <div className={styles.editScope}>
      <div className={styles.checklist}>
        {folders.map((f) => (
          <label key={f.id} className={styles.check}>
            <input
              type="checkbox"
              checked={set.has(f.id)}
              onChange={() =>
                setSet((prev) => {
                  const next = new Set(prev);
                  if (next.has(f.id)) next.delete(f.id);
                  else next.add(f.id);
                  return next;
                })
              }
            />
            {f.name}
          </label>
        ))}
      </div>
      <div className={styles.editActions}>
        <button
          type="button"
          className={styles.add}
          disabled={disabled}
          onClick={() => onSave([...set])}
        >
          {strings.delegateFoldersSave}
        </button>
        <button type="button" className={styles.cancel} onClick={onCancel}>
          {strings.delegateFoldersCancel}
        </button>
      </div>
    </div>
  );
}

/** Access-level + send-mode selects. Sending implies manage. */
function Selects({
  canWrite,
  sendMode,
  disabled,
  onChange,
}: {
  canWrite: boolean;
  sendMode: SendMode;
  disabled: boolean;
  onChange: (canWrite: boolean, sendMode: SendMode) => void;
}) {
  return (
    <span className={styles.selects}>
      <select
        className={styles.select}
        value={sendMode === "none" && !canWrite ? "read" : "manage"}
        disabled={disabled || sendMode !== "none"}
        onChange={(e) => onChange(e.target.value === "manage", sendMode)}
        aria-label={strings.delegateAccessLabel}
      >
        <option value="read">{strings.delegateReadOnly}</option>
        <option value="manage">{strings.delegateManage}</option>
      </select>
      <select
        className={styles.select}
        value={sendMode}
        disabled={disabled}
        onChange={(e) => {
          const s = e.target.value as SendMode;
          onChange(s === "none" ? canWrite : true, s);
        }}
        aria-label={strings.delegateSendLabel}
      >
        <option value="none">{strings.delegateSendNone}</option>
        <option value="as">{strings.delegateSendAs}</option>
        <option value="on_behalf">{strings.delegateSendOnBehalf}</option>
      </select>
    </span>
  );
}
