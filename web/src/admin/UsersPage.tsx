// Admin — Users & mailboxes. Lists the tenant's users with their usage and
// aliases; create users, reset passwords, toggle admin, manage aliases, and
// delete. Self-destructive actions are disabled for the signed-in admin (the
// server also refuses them).
import { useCallback, useEffect, useState } from "react";
import { Plus, Trash2 } from "lucide-react";

import { strings } from "../i18n";
import { Avatar, Spinner, Toggle, useDialogs } from "../ds";
import { useJmapClient } from "../jmap";
import type { AdminUser } from "../jmap";
import { formatBytes } from "../mail/format";
import { UserModal } from "./UserModal";
import { DelegatesModal } from "./DelegatesModal";
import styles from "./admin.module.css";

type Editing = { user?: AdminUser } | null;

export function UsersPage() {
  const { confirm } = useDialogs();
  const client = useJmapClient();
  const [users, setUsers] = useState<AdminUser[] | null>(null);
  const [selfId, setSelfId] = useState("");
  const [error, setError] = useState(false);
  const [editing, setEditing] = useState<Editing>(null);
  const [sharing, setSharing] = useState<AdminUser | null>(null);

  const load = useCallback(() => {
    setError(false);
    client
      .listUsers()
      .then(setUsers)
      .catch(() => setError(true));
  }, [client]);

  useEffect(() => {
    load();
    void client
      .accountId()
      .then(setSelfId)
      .catch(() => {
        // best-effort; self-guards fall back to the server's checks
      });
  }, [load, client]);

  async function toggleAdmin(u: AdminUser) {
    setUsers((prev) =>
      (prev ?? []).map((x) =>
        x.id === u.id ? { ...x, isAdmin: !x.isAdmin } : x,
      ),
    );
    try {
      await client.setUserAdmin(u.id, !u.isAdmin);
    } finally {
      load();
    }
  }

  async function remove(u: AdminUser) {
    if (
      !(await confirm({
        message: strings.userDeleteConfirm(u.email),
        danger: true,
      }))
    )
      return;
    try {
      await client.deleteUser(u.id);
    } finally {
      load();
    }
  }

  return (
    <div className={styles.page}>
      <header className={styles.pageHead}>
        <div>
          <h1>{strings.adminUsers}</h1>
          <p className={styles.pageIntro}>{strings.adminUsersIntro}</p>
        </div>
        <button
          type="button"
          className={styles.primary}
          onClick={() => setEditing({})}
        >
          <Plus size={16} />
          <span>{strings.adminAddUser}</span>
        </button>
      </header>

      {users === null && !error && (
        <div className={styles.state}>
          <Spinner size={22} />
        </div>
      )}
      {error && (
        <div className={styles.state}>
          <p>{strings.adminUsersError}</p>
          <button type="button" className={styles.textBtn} onClick={load}>
            {strings.mailRetry}
          </button>
        </div>
      )}

      {users !== null && (
        <ul className={styles.userList}>
          {users.map((u) => (
            <li key={u.id} className={styles.userRow}>
              <Avatar name={u.email} email={u.email} size="md" />
              <div className={styles.userText}>
                <div className={styles.userName}>
                  <strong>{u.email}</strong>
                  {u.isAdmin && (
                    <span className={styles.defaultBadge}>
                      {strings.userAdminBadge}
                    </span>
                  )}
                  {u.roles.includes("accountant") && (
                    <span className={styles.defaultBadge}>
                      {strings.userAccountantBadge}
                    </span>
                  )}
                </div>
                <div className={styles.userMeta}>
                  {strings.userUsage(
                    u.messageCount,
                    formatBytes(u.storageBytes),
                  )}
                  {u.aliases.length > 0 && ` · ${u.aliases.join(", ")}`}
                </div>
              </div>
              <div className={styles.userActions}>
                <button
                  type="button"
                  className={styles.ghost}
                  onClick={() => setEditing({ user: u })}
                >
                  {strings.userManage}
                </button>
                <button
                  type="button"
                  className={styles.ghost}
                  onClick={() => setSharing(u)}
                >
                  {strings.userShareAccess}
                </button>
                {/* Twenty rows, twenty switches, and the only name each had
                    was a `title` on an empty `<label>` — a tooltip is not a
                    name, so all twenty were announced as "checkbox, not
                    checked". Each now says whose admin access it grants, and
                    says it as a switch. The name is read but not drawn: the
                    column already carries the word. */}
                <Toggle
                  checked={u.isAdmin}
                  disabled={u.id === selfId}
                  onChange={() => void toggleAdmin(u)}
                  label={strings.userAdminRoleFor(u.email)}
                  hideLabel
                />
                <button
                  type="button"
                  className={styles.iconBtn}
                  disabled={u.id === selfId}
                  onClick={() => void remove(u)}
                  aria-label={strings.userDelete}
                >
                  <Trash2 size={16} />
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}

      {editing !== null && (
        <UserModal
          {...(editing.user !== undefined ? { user: editing.user } : {})}
          isSelf={editing.user?.id === selfId}
          onClose={() => setEditing(null)}
          onChanged={() => {
            setEditing(null);
            load();
          }}
          // Reload the list but leave the dialog open. `onChanged` dismisses
          // it, which is right after creating or deleting somebody and wrong
          // for an invitation: the setup link is minted once and only its hash
          // is kept, so closing the dialog loses it for good.
          onSaved={load}
        />
      )}

      {sharing !== null && (
        <DelegatesModal
          owner={sharing}
          users={users ?? []}
          onClose={() => setSharing(null)}
        />
      )}
    </div>
  );
}
