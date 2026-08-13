// Admin — Groups & lists. Lists the tenant's groups; a group with a list
// address is a distribution list (mail to it reaches every member). Create,
// manage members and the list address, and delete.
import { useCallback, useEffect, useState } from "react";
import { AtSign, Plus, Trash2, Users } from "lucide-react";

import { strings } from "../i18n";
import { Card, Spinner, cx, useDialogs } from "../ds";
import { useJmapClient } from "../jmap";
import type { AdminGroup } from "../jmap";
import { GroupModal } from "./GroupModal";
import styles from "./admin.module.css";

type Editing = { group?: AdminGroup } | null;

export function GroupsPage() {
  const { confirm } = useDialogs();
  const client = useJmapClient();
  const [groups, setGroups] = useState<AdminGroup[] | null>(null);
  const [error, setError] = useState(false);
  const [editing, setEditing] = useState<Editing>(null);

  const load = useCallback(() => {
    setError(false);
    client
      .listGroups()
      .then(setGroups)
      .catch(() => setError(true));
  }, [client]);

  useEffect(load, [load]);

  async function remove(g: AdminGroup) {
    if (!(await confirm({ message: strings.groupDeleteConfirm(g.name), danger: true }))) return;
    try {
      await client.deleteGroup(g.id);
    } finally {
      load();
    }
  }

  return (
    <div className={styles.page}>
      <header className={styles.pageHead}>
        <div>
          <h1>{strings.adminGroups}</h1>
          <p className={styles.pageIntro}>{strings.adminGroupsIntro}</p>
        </div>
        <button type="button" className={styles.primary} onClick={() => setEditing({})}>
          <Plus size={16} />
          <span>{strings.adminNewGroup}</span>
        </button>
      </header>

      {groups === null && !error && (
        <div className={styles.state}>
          <Spinner size={22} />
        </div>
      )}
      {error && (
        <div className={styles.state}>
          <p>{strings.adminGroupsError}</p>
          <button type="button" className={styles.textBtn} onClick={load}>
            {strings.mailRetry}
          </button>
        </div>
      )}

      {groups !== null && (
        <ul className={styles.userList}>
          {groups.map((g) => {
            const isList = (g.address ?? "").length > 0;
            return (
              <Card as="li" key={g.id} className={cx(styles.cardRow, isList && styles.cardDefault)}>
                <span className={styles.cardIcon}>
                  {isList ? <AtSign size={20} strokeWidth={1.75} /> : <Users size={20} strokeWidth={1.75} />}
                </span>
                <div className={styles.cardText}>
                  <div className={styles.cardName}>
                    <strong>{g.name}</strong>
                    {isList && <span className={styles.defaultBadge}>{strings.groupListBadge}</span>}
                  </div>
                  <div className={styles.cardDesc}>
                    {strings.groupMemberCount(g.memberCount)}
                    {isList && ` · ${g.address ?? ""}`}
                  </div>
                </div>
                <div className={styles.cardActions}>
                  <button type="button" className={styles.ghost} onClick={() => setEditing({ group: g })}>
                    {strings.userManage}
                  </button>
                  <button
                    type="button"
                    className={styles.iconBtn}
                    onClick={() => void remove(g)}
                    aria-label={strings.groupDelete}
                  >
                    <Trash2 size={16} />
                  </button>
                </div>
              </Card>
            );
          })}
        </ul>
      )}

      {editing !== null && (
        <GroupModal
          {...(editing.group !== undefined ? { group: editing.group } : {})}
          onClose={() => setEditing(null)}
          onChanged={() => {
            setEditing(null);
            load();
          }}
        />
      )}
    </div>
  );
}
