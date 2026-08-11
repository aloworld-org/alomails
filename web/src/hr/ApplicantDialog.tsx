// Record somebody who applied, or correct what was written down about them.
//
// Two things this form deliberately cannot do.
//
//   - **It cannot move anybody.** `stage` is not a writable field on the record
//     route: a corrected telephone number must never be able to reorder a
//     candidacy. Moving is the board, or the drawer's stage picker, and it is
//     audited with the deciding person's id on it.
//   - **It does not attach a CV.** Uploading one is the record route's own
//     `cv` field over a Drive blob, and the screen for it belongs with the rest
//     of the candidate's paperwork (B6.08c). Until then a CV recorded through
//     the API is shown and can be downloaded here; it is never read by
//     anything (`docs/design/hr.md` § The EU AI Act posture).
import { useState } from "react";
import { UserPlus } from "lucide-react";

import { strings } from "../i18n";
import { hrMessage, useHrApi } from "./api";
import { DialogFrame, Field } from "./parts";
import type { ApplicantDraft, HrApplicant } from "./types";
import styles from "./hr.module.css";

interface Props {
  /** The candidate being corrected, or `null` to record one. */
  applicant: HrApplicant | null;
  /** The round the new application belongs to. Ignored when correcting. */
  openingId: string;
  onClose: () => void;
  onSaved: (applicant: HrApplicant) => void;
}

export function ApplicantDialog({ applicant, openingId, onClose, onSaved }: Props) {
  const api = useHrApi();
  const [name, setName] = useState(applicant?.name ?? "");
  const [email, setEmail] = useState(applicant?.email ?? "");
  const [phone, setPhone] = useState(applicant?.phone ?? "");
  const [source, setSource] = useState(applicant?.source ?? "");
  const [retainUntil, setRetainUntil] = useState(applicant?.retainUntil ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function save() {
    setBusy(true);
    setError(null);
    try {
      const draft: ApplicantDraft = {};
      if (applicant === null || name.trim() !== applicant.name) draft.name = name.trim();
      if (applicant === null ? phone.trim() !== "" : phone.trim() !== applicant.phone) {
        draft.phone = phone.trim();
      }
      if (applicant === null ? source.trim() !== "" : source.trim() !== applicant.source) {
        draft.source = source.trim();
      }
      // An emptied address is an explicit `null` — "we have no address for
      // them" is a fact, and absent would mean "leave the old one".
      const address = email.trim() === "" ? null : email.trim();
      if (applicant === null ? address !== null : address !== applicant.email) {
        draft.email = address;
      }
      // A blank deadline on a new application means "the server's default" —
      // six months — rather than "no deadline", which is not a thing a
      // candidate's record is allowed to be.
      if (
        retainUntil.trim() !== "" &&
        (applicant === null || retainUntil.trim() !== applicant.retainUntil)
      ) {
        draft.retainUntil = retainUntil.trim();
      }
      onSaved(
        applicant === null
          ? await api.recordApplicant(openingId, draft)
          : await api.updateApplicant(applicant.id, draft),
      );
    } catch (err) {
      setError(hrMessage(err, strings.hrSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={UserPlus}
      title={applicant === null ? strings.hrAddCandidate : strings.hrEditCandidate}
      subtitle={strings.hrCandidateSubtitle}
      error={error}
      busy={busy}
      canSubmit={name.trim() !== ""}
      submitLabel={applicant === null ? strings.hrCreate : strings.hrSave}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <Field label={strings.hrFieldName}>
        <input
          className={styles.input}
          value={name}
          onChange={(e) => setName(e.target.value)}
          autoFocus
          required
        />
      </Field>

      <div className={styles.row}>
        <Field label={strings.hrFieldEmail}>
          <input
            className={styles.input}
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
          />
        </Field>
        <Field label={strings.hrFieldPhone}>
          <input
            className={styles.input}
            value={phone}
            onChange={(e) => setPhone(e.target.value)}
          />
        </Field>
      </div>

      <Field label={strings.hrFieldSource} hint={strings.hrSourceHint}>
        <input
          className={styles.input}
          value={source}
          onChange={(e) => setSource(e.target.value)}
        />
      </Field>

      <Field label={strings.hrFieldRetainUntil} hint={strings.hrRetainHint}>
        <input
          className={styles.input}
          type="date"
          value={retainUntil}
          onChange={(e) => setRetainUntil(e.target.value)}
        />
      </Field>
    </DialogFrame>
  );
}
