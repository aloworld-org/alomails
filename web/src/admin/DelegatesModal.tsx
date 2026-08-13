// Admin — manage who can access a user's mailbox (ADR 0017 delegation). Each
// delegate has an access level (read-only vs manage) and a send mode (none /
// send-as / send-on-behalf), editable inline; an admin can add a user or revoke
// access. All writes go through the admin-gated /admin/delegates routes.
import { useCallback, useEffect, useState } from "react";
import { FolderCog, X } from "lucide-react";

import { strings } from "../i18n";
import { Button, Checkbox, IconButton, Modal, Select, Spinner } from "../ds";
import { useJmapClient } from "../jmap";
import type { AdminUser, Delegate, SendMode } from "../jmap";
import styles from "./admin.module.css";

interface DelegatesModalProps {
  owner: AdminUser;
  users: AdminUser[];
  onClose: () => void;
}

export function DelegatesModal({ owner, users, onClose }: DelegatesModalProps) {
  const client = useJmapClient();
  const [delegates, setDelegates] = useState<Delegate[] | null>(null);
  const [folders, setFolders] = useState<{ id: string; name: string }[]>([]);
  const [pick, setPick] = useState("");
  // Which delegate's folder scope is being edited inline.
  const [editing, setEditing] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    setError(null);
    void client
      .listDelegates(owner.id)
      .then(setDelegates)
      .catch(() => setError(strings.delegateError));
    void client
      .adminUserMailboxes(owner.id)
      .then(setFolders)
      .catch(() => undefined);
  }, [client, owner.id]);
  useEffect(load, [load]);

  const addable = users.filter(
    (u) => u.id !== owner.id && !(delegates ?? []).some((d) => d.id === u.id),
  );
  const folderName = (id: string) => folders.find((f) => f.id === id)?.name ?? id;

  async function grant(
    delegateId: string,
    canWrite: boolean,
    sendMode: SendMode,
    folderIds: string[],
  ) {
    setBusy(true);
    setError(null);
    try {
      await client.grantDelegate(owner.id, delegateId, canWrite, sendMode, folderIds);
      setPick("");
      load();
    } catch {
      setError(strings.delegateError);
    } finally {
      setBusy(false);
    }
  }

  async function revoke(delegateId: string) {
    setBusy(true);
    try {
      await client.revokeDelegate(owner.id, delegateId);
      load();
    } catch {
      setError(strings.delegateError);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      title={strings.delegateTitle(owner.email)}
      onClose={onClose}
      actions={<IconButton label={strings.groupClose} icon={<X size={18} />} onClick={onClose} />}
      footer={
        <>
          <div className={styles.footSpacer} />
          <Button onClick={onClose}>{strings.groupClose}</Button>
        </>
      }
    >
      <p className={styles.hint}>{strings.delegateIntro}</p>

      {delegates === null ? (
        <Spinner size={18} />
      ) : delegates.length === 0 ? (
        <p className={styles.hint}>{strings.delegateNone}</p>
      ) : (
        <ul className={styles.delegateList}>
          {delegates.map((d) => (
            <li key={d.id} className={styles.delegateRow}>
              <div className={styles.delegateMain}>
                <span className={styles.delegateEmail}>{d.email}</span>
                <AccessControls
                  canWrite={d.canWrite}
                  sendMode={d.sendMode}
                  disabled={busy}
                  onChange={(w, s) => void grant(d.id, w, s, d.folders)}
                />
                {folders.length > 0 && (
                  <IconButton
                    size="sm"
                    label={strings.delegateFoldersFor(d.email)}
                    icon={<FolderCog size={15} />}
                    aria-expanded={editing === d.id}
                    onClick={() => setEditing(editing === d.id ? null : d.id)}
                  />
                )}
                <IconButton
                  size="sm"
                  label={strings.delegateRemoveFor(d.email)}
                  icon={<X size={16} />}
                  onClick={() => void revoke(d.id)}
                />
              </div>
              <span className={styles.delegateScope}>
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
                    void grant(d.id, d.canWrite, d.sendMode, ids);
                  }}
                  onCancel={() => setEditing(null)}
                />
              )}
            </li>
          ))}
        </ul>
      )}

      <div className={styles.keyRow}>
        {/* Named by the question. Unnamed, it was announced as its own current
            value — "combo box, Add delegate…" — which reads as an answer. */}
        <Select
          aria-label={strings.delegateAdd}
          value={pick}
          onChange={(e) => setPick(e.target.value)}
          disabled={addable.length === 0}
          placeholder={`${strings.delegateAdd}…`}
        >
          {addable.map((u) => (
            <option key={u.id} value={u.id}>
              {u.email}
            </option>
          ))}
        </Select>
        <Button
          variant="ghost"
          onClick={() => void grant(pick, true, "none", [])}
          disabled={busy || pick.length === 0}
        >
          {strings.delegateAdd}
        </Button>
      </div>

      {error !== null && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}

/** An inline folder multi-select confining a grant to specific folders. Saving
 * an empty selection clears the restriction back to whole-mailbox. */
function FolderScope({
  folders,
  selected,
  disabled,
  onSave,
  onCancel,
}: {
  folders: { id: string; name: string }[];
  selected: ReadonlySet<string>;
  disabled: boolean;
  onSave: (ids: string[]) => void;
  onCancel: () => void;
}) {
  const [set, setSet] = useState<ReadonlySet<string>>(selected);
  const toggle = (id: string) =>
    setSet((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  return (
    <div className={styles.scopeEdit}>
      <div className={styles.scopeChecklist}>
        {folders.map((f) => (
          <Checkbox
            key={f.id}
            checked={set.has(f.id)}
            onChange={() => toggle(f.id)}
            label={f.name}
          />
        ))}
      </div>
      <div className={styles.scopeActions}>
        <Button variant="ghost" disabled={disabled} onClick={() => onSave([...set])}>
          {strings.delegateFoldersSave}
        </Button>
        <Button variant="ghost" size="sm" onClick={onCancel}>
          {strings.delegateFoldersCancel}
        </Button>
      </div>
    </div>
  );
}

/** The access-level + send-mode selects for one delegate. Sending implies
 * manage, so choosing a send mode also upgrades access. */
export function AccessControls({
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
    <span className={styles.accessControls}>
      <select
        className={styles.accessSelect}
        value={sendMode === "none" && !canWrite ? "read" : "manage"}
        disabled={disabled || sendMode !== "none"}
        onChange={(e) => onChange(e.target.value === "manage", sendMode)}
        aria-label={strings.delegateAccessLabel}
      >
        <option value="read">{strings.delegateReadOnly}</option>
        <option value="manage">{strings.delegateManage}</option>
      </select>
      <select
        className={styles.accessSelect}
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
