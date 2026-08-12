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
import { Button, Checkbox, IconButton, Input, Select, Spinner } from "../ds";
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
                {/* Both buttons name the person the row is about. "Remove
                    access", five times in a list of five colleagues, is the
                    same sentence with nothing in it about whose. */}
                <IconButton
                  label={strings.delegateFoldersFor(d.email)}
                  icon={<FolderCog />}
                  onClick={() => setEditing(editing === d.id ? null : d.id)}
                  aria-expanded={editing === d.id}
                  className={styles.rowButton}
                />
                <IconButton
                  label={strings.delegateRemoveFor(d.email)}
                  icon={<X />}
                  onClick={() => void remove(d.id)}
                  className={styles.remove}
                />
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
        <Input
          className={styles.grow}
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
        {/* `type="submit"` said out loud: `ds/Button` defaults to "button", so
            leaving it off would make Add do nothing at all. */}
        <Button
          type="submit"
          disabled={busy || email.trim().length === 0}
          className={styles.add}
        >
          {strings.sharingAdd}
        </Button>
      </form>

      {folders.length > 0 && (
        <Checkbox
          checked={limitFolders}
          onChange={setLimitFolders}
          label={strings.delegateLimitFolders}
          className={styles.limit}
        />
      )}
      {limitFolders && (
        <div className={styles.checklist}>
          {folders.map((f) => (
            <Checkbox
              key={f.id}
              checked={scope.has(f.id)}
              onChange={() =>
                setScope((prev) => {
                  const next = new Set(prev);
                  if (next.has(f.id)) next.delete(f.id);
                  else next.add(f.id);
                  return next;
                })
              }
              label={f.name}
            />
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
          <Checkbox
            key={f.id}
            checked={set.has(f.id)}
            onChange={() =>
              setSet((prev) => {
                const next = new Set(prev);
                if (next.has(f.id)) next.delete(f.id);
                else next.add(f.id);
                return next;
              })
            }
            label={f.name}
          />
        ))}
      </div>
      <div className={styles.editActions}>
        <Button size="sm" disabled={disabled} onClick={() => onSave([...set])}>
          {strings.delegateFoldersSave}
        </Button>
        <Button size="sm" variant="ghost" onClick={onCancel}>
          {strings.delegateFoldersCancel}
        </Button>
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
    <span className={styles.access}>
      <Select
        value={sendMode === "none" && !canWrite ? "read" : "manage"}
        disabled={disabled || sendMode !== "none"}
        onChange={(e) => onChange(e.target.value === "manage", sendMode)}
        aria-label={strings.delegateAccessLabel}
      >
        <option value="read">{strings.delegateReadOnly}</option>
        <option value="manage">{strings.delegateManage}</option>
      </Select>
      <Select
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
      </Select>
    </span>
  );
}
