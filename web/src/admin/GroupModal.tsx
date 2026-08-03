// Create a group, or manage one (set its distribution-list address, add/remove
// members, delete). All writes go through the admin-gated /admin/groups routes.
import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { X } from "lucide-react";

import { strings } from "../i18n";
import { Button, Spinner } from "../ds";
import { useJmapClient } from "../jmap";
import type { AdminGroup, AdminUser } from "../jmap";
import styles from "./admin.module.css";

interface GroupModalProps {
  group?: AdminGroup;
  onClose: () => void;
  onChanged: () => void;
}

export function GroupModal({ group, onClose, onChanged }: GroupModalProps) {
  const client = useJmapClient();
  const [name, setName] = useState("");
  const [listName, setListName] = useState(group?.name ?? "");
  const [members, setMembers] = useState(group?.members ?? []);
  const [address, setAddress] = useState(group?.address ?? "");
  const [allUsers, setAllUsers] = useState<AdminUser[]>([]);
  const [pick, setPick] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (group === undefined) return;
    void client
      .listUsers()
      .then(setAllUsers)
      .catch(() => {
        // the member picker just stays empty on failure
      });
  }, [client, group]);

  async function create(e: FormEvent) {
    e.preventDefault();
    if (name.trim().length === 0) return;
    setBusy(true);
    setError(null);
    try {
      await client.createGroup(name.trim());
      onChanged();
    } catch {
      setError(strings.groupCreateError);
      setBusy(false);
    }
  }

  async function saveName() {
    if (group === undefined || busy || listName.trim().length === 0) return;
    setBusy(true);
    setError(null);
    try {
      await client.renameGroup(group.id, listName.trim());
      onChanged();
    } catch {
      setError(strings.groupActionError);
      setBusy(false);
    }
  }

  async function saveAddress() {
    if (group === undefined || busy) return;
    setBusy(true);
    setError(null);
    try {
      await client.setGroupAddress(group.id, address.trim().length > 0 ? address.trim() : null);
      onChanged();
    } catch {
      setError(strings.groupAddressError);
      setBusy(false);
    }
  }

  async function clearAddress() {
    if (group === undefined || busy) return;
    setBusy(true);
    setError(null);
    try {
      await client.setGroupAddress(group.id, null);
      onChanged();
    } catch {
      setError(strings.groupActionError);
      setBusy(false);
    }
  }

  async function addMember(userId: string) {
    if (group === undefined || userId.length === 0) return;
    const u = allUsers.find((x) => x.id === userId);
    if (u === undefined || members.some((m) => m.id === userId)) return;
    setBusy(true);
    try {
      await client.addGroupMember(group.id, userId);
      setMembers([...members, { id: u.id, email: u.email }]);
      setPick("");
    } catch {
      setError(strings.groupActionError);
    } finally {
      setBusy(false);
    }
  }

  async function removeMember(userId: string) {
    if (group === undefined) return;
    setBusy(true);
    try {
      await client.removeGroupMember(group.id, userId);
      setMembers(members.filter((m) => m.id !== userId));
    } catch {
      setError(strings.groupActionError);
    } finally {
      setBusy(false);
    }
  }

  async function del() {
    if (group === undefined || !window.confirm(strings.groupDeleteConfirm(group.name))) return;
    setBusy(true);
    try {
      await client.deleteGroup(group.id);
      onChanged();
    } catch {
      setError(strings.groupActionError);
      setBusy(false);
    }
  }

  if (group === undefined) {
    return (
      <div className={styles.overlay} onMouseDown={onClose}>
        <div
          className={styles.modal}
          role="dialog"
          aria-modal="true"
          aria-label={strings.adminNewGroup}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <form onSubmit={create}>
            <div className={styles.modalHead}>
              <h2>{strings.adminNewGroup}</h2>
              <button type="button" className={styles.iconBtn} onClick={onClose} aria-label={strings.groupClose}>
                <X size={18} />
              </button>
            </div>
            <div className={styles.modalBody}>
              <label className={styles.field}>
                <span className={styles.label}>{strings.groupName}</span>
                <input
                  className={styles.input}
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="Team"
                  autoFocus
                />
              </label>
              {error !== null && (
                <p className={styles.error} role="alert">
                  {error}
                </p>
              )}
            </div>
            <div className={styles.modalFoot}>
              <div className={styles.footSpacer} />
              <button type="button" className={styles.textBtn} onClick={onClose}>
                {strings.providerCancel}
              </button>
              <Button type="submit" disabled={busy}>
                {busy ? <Spinner size={16} /> : strings.groupCreate}
              </Button>
            </div>
          </form>
        </div>
      </div>
    );
  }

  const addable = allUsers.filter((u) => !members.some((m) => m.id === u.id));

  return (
    <div className={styles.overlay} onMouseDown={onClose}>
      <div
        className={styles.modal}
        role="dialog"
        aria-modal="true"
        aria-label={group.name}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className={styles.modalHead}>
          <h2>{group.name}</h2>
          <button type="button" className={styles.iconBtn} onClick={onClose} aria-label={strings.groupClose}>
            <X size={18} />
          </button>
        </div>
        <div className={styles.modalBody}>
          <div className={styles.field}>
            <span className={styles.label}>{strings.groupName}</span>
            <div className={styles.keyRow}>
              <input
                className={styles.input}
                value={listName}
                onChange={(e) => setListName(e.target.value)}
              />
              <button
                type="button"
                className={styles.ghost}
                onClick={() => void saveName()}
                disabled={busy || listName.trim().length === 0 || listName.trim() === group.name}
              >
                {strings.groupRename}
              </button>
            </div>
          </div>

          <div className={styles.field}>
            <span className={styles.label}>{strings.groupListAddress}</span>
            <div className={styles.keyRow}>
              <input
                className={styles.input}
                value={address}
                onChange={(e) => setAddress(e.target.value)}
                placeholder="team@namel3ss.com"
              />
              <button
                type="button"
                className={styles.ghost}
                onClick={() => void saveAddress()}
                disabled={busy || address.trim().length === 0}
              >
                {strings.groupAddressSave}
              </button>
              {(group.address ?? "").length > 0 && (
                <button type="button" className={styles.ghost} onClick={() => void clearAddress()} disabled={busy}>
                  {strings.groupAddressClear}
                </button>
              )}
            </div>
            <span className={styles.hint}>{strings.groupListAddressHint}</span>
          </div>

          <div className={styles.field}>
            <span className={styles.label}>{strings.groupMembers}</span>
            <div className={styles.chips}>
              {members.length === 0 && <span className={styles.hint}>{strings.groupNoMembers}</span>}
              {members.map((m) => (
                <span key={m.id} className={styles.chip}>
                  <span className={styles.chipLabel}>{m.email}</span>
                  <button
                    type="button"
                    className={styles.chipX}
                    onClick={() => void removeMember(m.id)}
                    aria-label={strings.providerRemoveModel(m.email)}
                  >
                    <X size={12} />
                  </button>
                </span>
              ))}
            </div>
            <div className={styles.keyRow}>
              <select
                className={styles.input}
                value={pick}
                onChange={(e) => setPick(e.target.value)}
                disabled={addable.length === 0}
              >
                <option value="">{strings.groupAddMember}…</option>
                {addable.map((u) => (
                  <option key={u.id} value={u.id}>
                    {u.email}
                  </option>
                ))}
              </select>
              <button
                type="button"
                className={styles.ghost}
                onClick={() => void addMember(pick)}
                disabled={busy || pick.length === 0}
              >
                {strings.groupAddMember}
              </button>
            </div>
          </div>

          {error !== null && (
            <p className={styles.error} role="alert">
              {error}
            </p>
          )}
        </div>
        <div className={styles.modalFoot}>
          <button type="button" className={styles.dangerBtn} onClick={() => void del()} disabled={busy}>
            {strings.groupDelete}
          </button>
          <div className={styles.footSpacer} />
          <button type="button" className={styles.primary} onClick={onClose}>
            {strings.groupClose}
          </button>
        </div>
      </div>
    </div>
  );
}
