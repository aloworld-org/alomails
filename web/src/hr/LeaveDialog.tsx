// Asking for time off.
//
// The form is three fields and one thing that is not a field: **who else is
// already off between the dates**, read as they are chosen. That is the
// recognition-over-recall law of `docs/design/ux-principles.md` applied to the
// question people actually ask — "can I take that week" — which is answered by
// looking at the team rather than by asking a person and finding out after.
//
// Nothing here judges the request. Whether the balance covers it, whether those
// days are already booked, whether the policy is still run, whether the dates
// fall inside the employment: every one of those is the server's, and its
// sentence is what this form shows. The form's only rule is that it cannot send
// a request with no policy and no dates, because there would be nothing to send.
//
// It also computes no cost. The days a request costs are folded server-side
// over the person's working pattern and the tenant's public holidays, and they
// arrive on the record the moment it is written — so the list behind this
// dialog shows the true figure a second later rather than a guess a moment
// earlier.
import { useEffect, useState } from "react";
import { CalendarPlus } from "lucide-react";

import { DatePicker } from "../ds";
import { strings } from "../i18n";
import { hrMessage, useHrApi } from "./api";
import { peopleAway } from "./leave";
import { DialogFrame, Field } from "./parts";
import type { HrAbsentPerson, HrLeavePolicy, HrLeaveRequest } from "./types";
import styles from "./hr.module.css";

interface Props {
  /** The policies this person may ask on — the live ones from their own
   *  balance read, so the picker offers exactly what the server would take. */
  policies: HrLeavePolicy[];
  onClose: () => void;
  /** The caller re-reads from the server; the record is passed on only as what
   *  was written, never as the state of the screen. */
  onAsked: (request: HrLeaveRequest) => void;
}

export function LeaveDialog({ policies, onClose, onAsked }: Props) {
  const api = useHrApi();
  const [policyId, setPolicyId] = useState(policies[0]?.id ?? "");
  const [fromDay, setFromDay] = useState("");
  const [toDay, setToDay] = useState("");
  const [note, setNote] = useState("");
  const [away, setAway] = useState<HrAbsentPerson[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const policy = policies.find((candidate) => candidate.id === policyId) ?? null;
  const ranged = fromDay !== "" && toDay !== "" && fromDay <= toDay;

  // Who else is off between these dates. A window that is not a window yet is
  // not asked about, and a failed read costs the hint and never the form: this
  // is context, not a rule.
  useEffect(() => {
    if (!ranged) {
      setAway(null);
      return undefined;
    }
    let live = true;
    api
      .absences(fromDay, toDay)
      .then((days) => {
        if (live) setAway(peopleAway(days, null));
      })
      .catch(() => {
        if (live) setAway(null);
      });
    return () => {
      live = false;
    };
  }, [api, ranged, fromDay, toDay]);

  async function ask() {
    setBusy(true);
    setError(null);
    try {
      const draft = { policyId, fromDay, toDay, ...(note.trim() === "" ? {} : { note: note.trim() }) };
      onAsked(await api.createLeaveRequest(draft));
    } catch (err) {
      setError(hrMessage(err, strings.hrSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={CalendarPlus}
      title={strings.hrAskForLeave}
      subtitle={strings.hrAskSubtitle}
      error={error}
      busy={busy}
      canSubmit={policyId !== "" && ranged}
      submitLabel={strings.hrAskSubmit}
      onClose={onClose}
      onSubmit={() => void ask()}
    >
      <Field
        label={strings.hrLeaveKind}
        {...(policy !== null && !policy.requiresApproval
          ? { hint: strings.hrPolicyRecordedHint }
          : {})}
      >
        <select
          className={styles.input}
          value={policyId}
          onChange={(e) => setPolicyId(e.target.value)}
          autoFocus
        >
          {policies.map((candidate) => (
            <option key={candidate.id} value={candidate.id}>
              {candidate.name}
            </option>
          ))}
        </select>
      </Field>

      <div className={styles.row}>
        <Field label={strings.hrFieldFirstDay}>
          <DatePicker value={fromDay} onChange={setFromDay} />
        </Field>
        <Field
          label={strings.hrFieldLastDay}
          hint={strings.hrLastDayHint}
          {...(fromDay !== "" && toDay !== "" && toDay < fromDay
            ? { error: strings.hrRangeBackwards }
            : {})}
        >
          <DatePicker value={toDay} onChange={setToDay} />
        </Field>
      </div>

      {/* The team, behind the choice. Shown as soon as there is a window to ask
          about, and silent about anything but a name — the absence layer carries
          no policy, no kind and no note by construction. */}
      {ranged && (
        <div className={styles.panel}>
          <h3 className={styles.panelTitle}>{strings.hrAlsoAway}</h3>
          {away === null || away.length === 0 ? (
            <p className={styles.panelEmpty}>{strings.hrNobodyAway}</p>
          ) : (
            <p className={styles.awayNames}>{away.map((person) => person.name).join(", ")}</p>
          )}
        </div>
      )}

      <Field label={strings.hrLeaveWhy} hint={strings.hrWhyHint}>
        <textarea
          className={styles.textarea}
          rows={3}
          value={note}
          onChange={(e) => setNote(e.target.value)}
        />
      </Field>
    </DialogFrame>
  );
}
