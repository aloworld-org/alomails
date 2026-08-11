// Somebody took the job: the form that writes them into the directory (alo HR,
// ADR 0035, wave B6.08c).
//
// The bridge a hiring board owes the rest of the module, and the four decisions
// it makes.
//
//   - **It is offered, never automatic.** Moving a card to `hired` records an
//     outcome; it does not create a colleague. Somebody presses this, fills in
//     the day the terms begin, and the record is written with their id on it.
//     A board that created people would create them on a mis-drop.
//   - **The prefill is a prefill.** The name is split by a heuristic
//     (`hire.ts`), the role comes from the round that was advertised, and every
//     field is editable before anything is sent.
//   - **It asks the directory first, and warns rather than refuses.** The
//     server keeps no unique index on a work address; somebody who left and
//     came back is a genuine second record, and only the person here knows
//     which case this is.
//   - **It creates no login.** Said in the form, not only in a design note: a
//     write path from HR into identity is a permanent non-goal
//     (`docs/design/hr.md` § Out of scope), and the mailbox is a task on the
//     onboarding checklist (B6.05).
import { useEffect, useState } from "react";
import { UserCheck } from "lucide-react";

import { strings } from "../i18n";
import { hrMessage, useHrApi } from "./api";
import { alreadyInDirectory, canHire, employeeDraft, hirePrefill } from "./hire";
import { kindLabel } from "./format";
import { DialogFrame, Field } from "./parts";
import type { HrApplicant, HrCreatedEmployee, HrDirectoryEntry, HrOpening } from "./types";
import { EMPLOYMENT_KINDS } from "./types";
import styles from "./hr.module.css";

interface Props {
  /** The candidate who took the job. */
  applicant: HrApplicant;
  /** The round on screen — the role they were hired into, when it is theirs. */
  opening: HrOpening | null;
  onClose: () => void;
  /** They are in the directory now: the screen says where. */
  onHired: (employee: HrCreatedEmployee) => void;
}

export function HireDialog({ applicant, opening, onClose, onHired }: Props) {
  const api = useHrApi();
  const [fields, setFields] = useState(() => ({
    ...hirePrefill(applicant, opening),
    startedOn: "",
  }));
  const [people, setPeople] = useState<HrDirectoryEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // The directory, once, to answer one question: is this address already
  // somebody's? Including the people who have left, because a returning
  // colleague is exactly the case worth naming. A directory that fails to load
  // leaves the form usable — it is a warning, not a gate.
  useEffect(() => {
    let live = true;
    api
      .directory(true)
      .then((directory) => {
        if (live) setPeople(directory.employees);
      })
      .catch(() => {
        if (live) setPeople([]);
      });
    return () => {
      live = false;
    };
  }, [api]);

  const existing = alreadyInDirectory(people, fields.workEmail);

  function set(field: keyof typeof fields, value: string) {
    setFields((current) => ({ ...current, [field]: value }));
  }

  async function save() {
    setBusy(true);
    setError(null);
    try {
      onHired(await api.createEmployee(employeeDraft(fields)));
    } catch (err) {
      setError(hrMessage(err, strings.hrSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={UserCheck}
      title={strings.hrHire}
      subtitle={strings.hrHireSubtitle}
      error={error}
      busy={busy}
      canSubmit={canHire(fields)}
      submitLabel={strings.hrHireSubmit}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      {existing !== null && (
        <p className={styles.dialogNotice} role="status">
          {existing.archived
            ? strings.hrHireKnownLeft(existing.name)
            : strings.hrHireKnown(existing.name)}
        </p>
      )}

      <div className={styles.row}>
        <Field label={strings.hrFieldGivenName} hint={strings.hrHireNameHint}>
          <input
            className={styles.input}
            value={fields.givenName}
            onChange={(e) => set("givenName", e.target.value)}
            autoFocus
            required
          />
        </Field>
        <Field label={strings.hrFieldFamilyName}>
          <input
            className={styles.input}
            value={fields.familyName}
            onChange={(e) => set("familyName", e.target.value)}
            required
          />
        </Field>
      </div>

      <Field label={strings.hrFieldWorkEmail} hint={strings.hrHireEmailHint}>
        <input
          className={styles.input}
          type="email"
          value={fields.workEmail}
          onChange={(e) => set("workEmail", e.target.value)}
        />
      </Field>

      <div className={styles.row}>
        <Field label={strings.hrFieldJobTitle}>
          <input
            className={styles.input}
            value={fields.jobTitle}
            onChange={(e) => set("jobTitle", e.target.value)}
          />
        </Field>
        <Field label={strings.hrFieldTeam}>
          <input
            className={styles.input}
            value={fields.team}
            onChange={(e) => set("team", e.target.value)}
          />
        </Field>
      </div>

      <div className={styles.row}>
        <Field label={strings.hrFieldEmployment}>
          <select
            className={styles.input}
            value={fields.contractKind}
            onChange={(e) => set("contractKind", e.target.value)}
          >
            {/* A word this build does not know — an older round on a newer
                server — stays selected rather than becoming the first in the
                list, and the server refuses anything it does not accept. */}
            {fields.contractKind === "" ||
            (EMPLOYMENT_KINDS as readonly string[]).includes(fields.contractKind) ? null : (
              <option value={fields.contractKind}>{kindLabel(fields.contractKind)}</option>
            )}
            <option value="">{strings.hrHireNoKind}</option>
            {EMPLOYMENT_KINDS.map((kind) => (
              <option key={kind} value={kind}>
                {kindLabel(kind)}
              </option>
            ))}
          </select>
        </Field>
        <Field label={strings.hrFieldStartedOn} hint={strings.hrHireStartHint}>
          <input
            className={styles.input}
            type="date"
            value={fields.startedOn}
            onChange={(e) => set("startedOn", e.target.value)}
            required
          />
        </Field>
      </div>

      <p className={styles.hint}>{strings.hrHireNoAccount}</p>
    </DialogFrame>
  );
}
