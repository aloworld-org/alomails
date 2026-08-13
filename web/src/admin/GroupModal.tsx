// Create a group, or manage one (set its distribution-list address, add/remove
// members, delete). All writes go through the admin-gated /admin/groups routes.
import { useEffect, useId, useState } from "react";
import type { FormEvent } from "react";
import { X } from "lucide-react";

import { strings } from "../i18n";
import {
  Button,
  Chip,
  Field,
  IconButton,
  Input,
  Modal,
  Select,
  Spinner,
  useDialogs,
} from "../ds";
import { useJmapClient } from "../jmap";
import type { AdminGroup, AdminUser } from "../jmap";
import styles from "./admin.module.css";

interface GroupModalProps {
  group?: AdminGroup;
  onClose: () => void;
  onChanged: () => void;
}

export function GroupModal({ group, onClose, onChanged }: GroupModalProps) {
  // The create form lives in the dialog's body and its submit button in the
  // dialog's footer, which are siblings rather than ancestor and descendant —
  // so the button is tied to the form by id, and Enter in the name field still
  // creates the group.
  const formId = useId();
  const { confirm } = useDialogs();
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
    if (group === undefined || !(await confirm({ message: strings.groupDeleteConfirm(group.name), danger: true }))) return;
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
      <Modal
        title={strings.adminNewGroup}
        onClose={onClose}
        actions={<IconButton label={strings.groupClose} icon={<X size={18} />} onClick={onClose} />}
        footer={
          <>
            <div className={styles.footSpacer} />
            <Button variant="ghost" onClick={onClose}>
              {strings.providerCancel}
            </Button>
            <Button type="submit" form={formId} disabled={busy}>
              {busy ? <Spinner size={16} /> : strings.groupCreate}
            </Button>
          </>
        }
      >
        <form id={formId} onSubmit={create}>
          <Field label={strings.groupName}>
            {(control) => (
              <Input
                {...control}
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Team"
                autoFocus
              />
            )}
          </Field>
        </form>
        {error !== null && (
          <p className={styles.error} role="alert">
            {error}
          </p>
        )}
      </Modal>
    );
  }

  const addable = allUsers.filter((u) => !members.some((m) => m.id === u.id));

  return (
    <Modal
      title={group.name}
      onClose={onClose}
      actions={<IconButton label={strings.groupClose} icon={<X size={18} />} onClick={onClose} />}
      footer={
        <>
          <button type="button" className={styles.dangerBtn} onClick={() => void del()} disabled={busy}>
            {strings.groupDelete}
          </button>
          <div className={styles.footSpacer} />
          <Button onClick={onClose}>{strings.groupClose}</Button>
        </>
      }
    >
      <Field label={strings.groupName}>
        {(control) => (
          <div className={styles.keyRow}>
            <Input
              {...control}
              className={styles.keyRowGrow}
              value={listName}
              onChange={(e) => setListName(e.target.value)}
            />
            <Button
              variant="ghost"
              onClick={() => void saveName()}
              disabled={busy || listName.trim().length === 0 || listName.trim() === group.name}
            >
              {strings.groupRename}
            </Button>
          </div>
        )}
      </Field>

      <Field label={strings.groupListAddress} hint={strings.groupListAddressHint}>
        {(control) => (
          <div className={styles.keyRow}>
            <Input
              {...control}
              className={styles.keyRowGrow}
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              placeholder="team@namel3ss.com"
            />
            <Button
              variant="ghost"
              onClick={() => void saveAddress()}
              disabled={busy || address.trim().length === 0}
            >
              {strings.groupAddressSave}
            </Button>
            {(group.address ?? "").length > 0 && (
              <Button variant="ghost" onClick={() => void clearAddress()} disabled={busy}>
                {strings.groupAddressClear}
              </Button>
            )}
          </div>
        )}
      </Field>

      {/* Not a `ds/Field`: "Members" names a section — a list of people and a
          way to add one — rather than a single control, and a `<label>` bound
          to whichever control happened to come first would be a lie. The
          picker carries its own name. */}
      <div className={styles.block}>
        <span className={styles.label}>{strings.groupMembers}</span>
        <div className={styles.chipRow}>
          {members.length === 0 && <span className={styles.hint}>{strings.groupNoMembers}</span>}
          {members.map((m) => (
            <Chip
              key={m.id}
              onRemove={() => void removeMember(m.id)}
              removeLabel={strings.providerRemoveModel(m.email)}
            >
              {m.email}
            </Chip>
          ))}
        </div>
        <div className={styles.keyRow}>
          <Select
            className={styles.keyRowGrow}
            aria-label={strings.groupAddMember}
            value={pick}
            onChange={(e) => setPick(e.target.value)}
            disabled={addable.length === 0}
            placeholder={`${strings.groupAddMember}…`}
          >
            {addable.map((u) => (
              <option key={u.id} value={u.id}>
                {u.email}
              </option>
            ))}
          </Select>
          <Button
            variant="ghost"
            onClick={() => void addMember(pick)}
            disabled={busy || pick.length === 0}
          >
            {strings.groupAddMember}
          </Button>
        </div>
      </div>

      {error !== null && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
