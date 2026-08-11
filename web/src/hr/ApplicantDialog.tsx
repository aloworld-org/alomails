// Record somebody who applied, or correct what was written down about them —
// their paperwork included.
//
// What this form deliberately cannot do: **it cannot move anybody.** `stage` is
// not a writable field on the record route, so a corrected telephone number can
// never reorder a candidacy. Moving is the board, or the drawer's stage picker,
// and it is audited with the deciding person's id on it.
//
// What it does do, as of B6.08c, is attach the CV (until then a CV could only
// arrive through the API). Three decisions there:
//
//   - **The file is uploaded when the form is submitted, not when it is
//     chosen.** A form somebody closes has uploaded nothing, and the record and
//     its paper are written in one act rather than two that can half-fail.
//   - **This is the only upload control**, and the drawer sends people here
//     rather than growing a second one — one path, one set of failure
//     sentences.
//   - **Taking a CV off is an explicit `cv: null`**, which trashes the file in
//     the HR area. Absent would mean "leave what is there", and a form cannot
//     tell those two apart by looking at an empty file input.
//
// **Nothing here reads the file.** It is a blob handed to the record route,
// which files it in the tenant's HR area — no parse, no extract, no score
// (`docs/design/hr.md` § The EU AI Act posture).
import { useState } from "react";
import { UserPlus } from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
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
  const client = useJmapClient();
  const [name, setName] = useState(applicant?.name ?? "");
  const [email, setEmail] = useState(applicant?.email ?? "");
  const [phone, setPhone] = useState(applicant?.phone ?? "");
  const [source, setSource] = useState(applicant?.source ?? "");
  const [retainUntil, setRetainUntil] = useState(applicant?.retainUntil ?? "");
  const [cv, setCv] = useState<File | null>(null);
  const [removeCv, setRemoveCv] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  /** Whether there is a file on this record to replace or take off. A CV that
   *  has since been trashed through Drive is not one: the record keeps the
   *  honest statement that there was one, and the only useful act left is
   *  attaching another. */
  const hasCv = applicant !== null && applicant.cvNodeId !== null && !applicant.cvTrashed;

  async function save() {
    setBusy(true);
    setError(null);
    try {
      const draft: ApplicantDraft = {};
      // The upload first, and its own sentence when it fails: a candidate whose
      // details saved but whose CV silently did not is worse than a form that
      // says which half went wrong.
      if (cv !== null) {
        try {
          const uploaded = await client.uploadFile(cv);
          draft.cv = {
            blobId: uploaded.blobId,
            name: cv.name,
            // The server's own measurement of what it stored, not the File's:
            // the two agree, and only one of them is the thing on disk.
            size: uploaded.size,
            contentType: uploaded.type === "" ? null : uploaded.type,
          };
        } catch {
          setError(strings.hrCvUploadFailed);
          return;
        }
      } else if (removeCv && hasCv) {
        draft.cv = null;
      }
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

      <Field
        label={hasCv ? strings.hrCvReplace : strings.hrCv}
        hint={hasCv ? strings.hrCvOnFile(applicant.cvFileName ?? "") : strings.hrCvHint}
      >
        <input
          className={styles.fileInput}
          type="file"
          onChange={(e) => {
            setCv(e.target.files?.[0] ?? null);
            setRemoveCv(false);
          }}
        />
      </Field>

      {hasCv && (
        <label className={styles.toggle}>
          <input
            type="checkbox"
            checked={removeCv}
            disabled={cv !== null}
            onChange={(e) => setRemoveCv(e.target.checked)}
          />
          {strings.hrCvRemove}
        </label>
      )}

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
