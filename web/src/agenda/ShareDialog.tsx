// Share a calendar the viewer owns with people (by email) or teams (a group),
// at viewer or editor access. Lists the current shares and lets the owner add
// or remove them. All calls go through the authenticated /calendar API; only a
// calendar the caller owns can be shared (enforced server-side).
import { useCallback, useEffect, useState } from "react";
import { X } from "lucide-react";

import { strings } from "../i18n";
import { Button } from "../ds";
import {
  useJmapClient,
  type Calendar,
  type CalendarGrant,
  type ShareableGroup,
} from "../jmap";
import styles from "./AgendaModule.module.css";

interface Props {
  calendar: Calendar;
  onClose: () => void;
}

type Kind = "user" | "group";
type Role = "viewer" | "editor";

export function ShareDialog({ calendar, onClose }: Props) {
  const client = useJmapClient();
  const [grants, setGrants] = useState<CalendarGrant[]>([]);
  const [groups, setGroups] = useState<ShareableGroup[]>([]);
  const [kind, setKind] = useState<Kind>("user");
  const [email, setEmail] = useState("");
  const [groupId, setGroupId] = useState("");
  const [role, setRole] = useState<Role>("viewer");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [gs, grp] = await Promise.all([
        client.calendarGrants(calendar.id),
        client.shareableGroups(),
      ]);
      setGrants(gs);
      setGroups(grp);
    } catch {
      setError(strings.agendaShareLoadError);
    }
  }, [client, calendar.id]);

  useEffect(() => {
    void load();
  }, [load]);

  async function add() {
    const subject = kind === "user" ? email.trim() : groupId;
    if (subject === "") return;
    setBusy(true);
    setError(null);
    try {
      await client.shareCalendar(calendar.id, kind, subject, role);
      setEmail("");
      setGroupId("");
      await load();
    } catch {
      setError(strings.agendaShareError);
    } finally {
      setBusy(false);
    }
  }

  async function remove(g: CalendarGrant) {
    setBusy(true);
    setError(null);
    try {
      await client.unshareCalendar(calendar.id, g.kind, g.subject);
      await load();
    } catch {
      setError(strings.agendaShareError);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className={styles.modalScrim}
      role="dialog"
      aria-modal="true"
      onMouseDown={onClose}
    >
      <div className={styles.modal} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.modalHead}>
          <h2>{strings.agendaShareTitle(calendar.name)}</h2>
          <button
            type="button"
            className={styles.iconBtn}
            onClick={onClose}
            aria-label={strings.agendaClose}
          >
            <X size={18} />
          </button>
        </div>

        {grants.length > 0 ? (
          <ul className={styles.shareList}>
            {grants.map((g) => (
              <li key={`${g.kind}:${g.subject}`} className={styles.shareRow}>
                <span className={styles.shareLabel}>
                  {g.label}
                  {g.kind === "group" && (
                    <span className={styles.shareTag}>
                      {strings.agendaShareGroup}
                    </span>
                  )}
                </span>
                <span className={styles.shareRole}>
                  {g.role === "editor"
                    ? strings.agendaShareEditor
                    : strings.agendaShareViewer}
                </span>
                <button
                  type="button"
                  className={styles.linkBtn}
                  onClick={() => void remove(g)}
                  disabled={busy}
                >
                  {strings.agendaShareRemove}
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className={styles.fieldHint}>{strings.agendaShareEmpty}</p>
        )}

        <div className={styles.shareForm}>
          <label className={styles.field}>
            <span>{strings.agendaShareWith}</span>
            <select
              value={kind}
              onChange={(e) => setKind(e.target.value as Kind)}
            >
              <option value="user">{strings.agendaSharePerson}</option>
              <option value="group">{strings.agendaShareGroupOption}</option>
            </select>
          </label>

          {kind === "user" ? (
            <label className={styles.field}>
              <span>{strings.agendaShareEmail}</span>
              <input
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder={strings.agendaShareEmailPlaceholder}
                inputMode="email"
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
              />
            </label>
          ) : (
            <label className={styles.field}>
              <span>{strings.agendaShareGroupOption}</span>
              <select
                value={groupId}
                onChange={(e) => setGroupId(e.target.value)}
              >
                <option value="">{strings.agendaShareGroupPick}</option>
                {groups.map((g) => (
                  <option key={g.id} value={g.id}>
                    {g.name}
                  </option>
                ))}
              </select>
            </label>
          )}

          <label className={styles.field}>
            <span>{strings.agendaShareAccess}</span>
            <select
              value={role}
              onChange={(e) => setRole(e.target.value as Role)}
            >
              <option value="viewer">{strings.agendaShareViewer}</option>
              <option value="editor">{strings.agendaShareEditor}</option>
            </select>
          </label>
        </div>

        {error !== null && (
          <p className={styles.modalError} role="alert">
            {error}
          </p>
        )}

        <div className={styles.modalActions}>
          <div className={styles.modalActionsRight}>
            <button
              type="button"
              className={styles.linkBtn}
              onClick={onClose}
              disabled={busy}
            >
              {strings.agendaClose}
            </button>
            <Button
              type="button"
              onClick={() => void add()}
              disabled={
                busy || (kind === "user" ? email.trim() === "" : groupId === "")
              }
            >
              {strings.agendaShareAdd}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
