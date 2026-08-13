// Share a calendar the viewer owns with people (by email) or teams (a group),
// at viewer or editor access. Lists the current shares and lets the owner add
// or remove them. All calls go through the authenticated /calendar API; only a
// calendar the caller owns can be shared (enforced server-side).
import { useCallback, useEffect, useState } from "react";
import { X } from "lucide-react";

import { strings } from "../i18n";
import { Button, Field, IconButton, Input, Modal, Select } from "../ds";
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
    <Modal
      title={strings.agendaShareTitle(calendar.name)}
      onClose={onClose}
      actions={
        <IconButton
          label={strings.agendaClose}
          icon={<X size={18} />}
          onClick={onClose}
        />
      }
      footer={
        <>
          <div className={styles.footSpacer} />
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {strings.agendaClose}
          </Button>
          <Button
            onClick={() => void add()}
            disabled={
              busy || (kind === "user" ? email.trim() === "" : groupId === "")
            }
          >
            {strings.agendaShareAdd}
          </Button>
        </>
      }
    >
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
              {/* One "Remove" per row is a list of identical buttons read
                  aloud; each says whose access it takes away. */}
              <Button
                variant="ghost"
                size="sm"
                onClick={() => void remove(g)}
                disabled={busy}
                aria-label={strings.agendaShareRemoveFor(g.label)}
              >
                {strings.agendaShareRemove}
              </Button>
            </li>
          ))}
        </ul>
      ) : (
        <p className={styles.fieldHint}>{strings.agendaShareEmpty}</p>
      )}

      <div className={styles.shareForm}>
        <Field label={strings.agendaShareWith}>
          {(control) => (
            <Select
              {...control}
              value={kind}
              onChange={(e) => setKind(e.target.value as Kind)}
            >
              <option value="user">{strings.agendaSharePerson}</option>
              <option value="group">{strings.agendaShareGroupOption}</option>
            </Select>
          )}
        </Field>

        {kind === "user" ? (
          <Field label={strings.agendaShareEmail}>
            {(control) => (
              <Input
                {...control}
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder={strings.agendaShareEmailPlaceholder}
                inputMode="email"
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
              />
            )}
          </Field>
        ) : (
          <Field label={strings.agendaShareGroupOption}>
            {(control) => (
              <Select
                {...control}
                value={groupId}
                onChange={(e) => setGroupId(e.target.value)}
                placeholder={strings.agendaShareGroupPick}
              >
                {groups.map((g) => (
                  <option key={g.id} value={g.id}>
                    {g.name}
                  </option>
                ))}
              </Select>
            )}
          </Field>
        )}

        <Field label={strings.agendaShareAccess}>
          {(control) => (
            <Select
              {...control}
              value={role}
              onChange={(e) => setRole(e.target.value as Role)}
            >
              <option value="viewer">{strings.agendaShareViewer}</option>
              <option value="editor">{strings.agendaShareEditor}</option>
            </Select>
          )}
        </Field>
      </div>

      {error !== null && (
        <p className={styles.modalError} role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
