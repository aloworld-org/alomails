// One candidate, opened from the board: what the application said, the notes
// the people who met them wrote, the CV on file, and the acts a record about a
// person needs — moving them, writing them into the directory when they took
// the job, and erasing them when its date has passed.
//
// The stage picker is here **as well as** the drag on the board, and not as a
// convenience: a board that can only be worked by dragging cannot be worked
// from a keyboard at all, and deciding somebody's candidacy is the last place
// an interface may require a mouse.
//
// Erasing is confirmed rather than undone — the one place in this module that
// asks. Undo is the law everywhere it can be honoured
// (`docs/design/ux-principles.md`), and here it cannot: the record, its notes
// and the CV are actually gone, which is the entire point of the deadline.
//
// **Nothing here reads the CV.** It is downloaded through Drive's own HR-gated
// path, byte for byte, and never parsed, extracted, indexed or scored.
import { useCallback, useEffect, useState } from "react";
import { Download, FileText, Pencil, Trash2, Upload, UserCheck, X } from "lucide-react";

import { saveBlob } from "../drive";
import { Button, IconButton, Spinner, useDialogs } from "../ds";
import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { hrMessage, useHrApi } from "./api";
import { dayLabel, momentLabel, stageLabel } from "./format";
import { HIRED_STAGE } from "./hire";
import { Chip, ErrorBanner } from "./parts";
import type { HrApplicant, HrApplicantNote } from "./types";
import styles from "./hr.module.css";

interface Props {
  applicantId: string;
  onClose: () => void;
  /** Something about this candidate changed: the board re-reads. */
  onChanged: () => void;
  /** Correct what was written down — the form the screen owns, opened on the
   *  record as it stands here. It is also where a CV is attached, replaced or
   *  taken off: one upload path, not two. */
  onEdit: (applicant: HrApplicant) => void;
  /** They took the job: write them into the directory (B6.08c). The form is the
   *  screen's, because where it lands afterwards is the screen's business. */
  onHire: (applicant: HrApplicant) => void;
  /** Called after an erasure: the record this drawer is about no longer
   *  exists, so the screen closes it and re-reads the board. */
  onGone: () => void;
}

export function ApplicantDrawer({
  applicantId,
  onClose,
  onChanged,
  onEdit,
  onHire,
  onGone,
}: Props) {
  const api = useHrApi();
  const client = useJmapClient();
  const { confirm } = useDialogs();
  const [applicant, setApplicant] = useState<HrApplicant | null>(null);
  const [notes, setNotes] = useState<HrApplicantNote[]>([]);
  const [stages, setStages] = useState<string[]>([]);
  const [note, setNote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const detail = await api.applicant(applicantId);
      setApplicant(detail.applicant);
      setNotes(detail.notes);
      setStages(detail.stages);
      setError(null);
    } catch (err) {
      setError(hrMessage(err, strings.hrLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, applicantId]);

  useEffect(() => {
    void load();
  }, [load]);

  /** Where a person decided this candidate now stands. The record is redrawn
   *  from the server's answer, never from the word the picker holds. */
  async function move(stage: string) {
    setBusy(true);
    setError(null);
    try {
      setApplicant(await api.moveApplicant(applicantId, stage));
      onChanged();
    } catch (err) {
      setError(hrMessage(err, strings.hrSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  async function addNote() {
    const body = note.trim();
    if (body === "") return;
    setBusy(true);
    setError(null);
    try {
      const written = await api.addNote(applicantId, body);
      setNotes((all) => [written, ...all]);
      setNote("");
    } catch (err) {
      setError(hrMessage(err, strings.hrSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  async function erase() {
    if (applicant === null) return;
    const sure = await confirm({
      title: strings.hrErase,
      message: strings.hrEraseConfirm(applicant.name),
      confirmLabel: strings.hrErase,
      danger: true,
    });
    if (!sure) return;
    setBusy(true);
    setError(null);
    try {
      await api.eraseApplicant(applicantId);
      onGone();
    } catch (err) {
      setError(hrMessage(err, strings.hrSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  /** The CV, byte for byte, through Drive's own HR-gated download. */
  async function downloadCv() {
    if (applicant?.cvNodeId === null || applicant === null) return;
    setError(null);
    try {
      saveBlob(
        await client.driveDownload(applicant.cvNodeId),
        applicant.cvFileName ?? applicant.name,
      );
    } catch {
      setError(strings.hrCvFailed);
    }
  }

  return (
    <aside className={styles.drawer} aria-label={applicant?.name ?? strings.hrCandidate}>
      <div className={styles.drawerHead}>
        <div className={styles.drawerTitleRow}>
          <h2 className={styles.drawerTitle}>{applicant?.name ?? strings.hrCandidate}</h2>
          {loading && <Spinner size={16} />}
          {applicant !== null && (
            <IconButton
              label={strings.hrEditCandidate}
              icon={<Pencil size={16} />}
              onClick={() => onEdit(applicant)}
            />
          )}
          <IconButton label={strings.hrClose} icon={<X size={18} />} onClick={onClose} />
        </div>
        {applicant !== null && (
          <>
            <div className={styles.drawerFacts}>
              {applicant.email !== null && applicant.email !== "" && (
                <a className={styles.drawerLink} href={`mailto:${applicant.email}`}>
                  {applicant.email}
                </a>
              )}
              {applicant.phone !== "" && (
                <span className={styles.drawerFact}>{applicant.phone}</span>
              )}
              {applicant.source !== "" && (
                <span className={styles.drawerFact}>{applicant.source}</span>
              )}
            </div>
            <div className={styles.drawerActions}>
              <label className={styles.stagePicker}>
                <span className={styles.label}>{strings.hrStage}</span>
                <select
                  className={styles.input}
                  value={applicant.stage}
                  disabled={busy}
                  onChange={(e) => void move(e.target.value)}
                >
                  {/* A stage this build does not know about — an older record on
                      a newer server — stays selected rather than silently
                      becoming the first word in the list. */}
                  {stages.includes(applicant.stage) ? null : (
                    <option value={applicant.stage}>{stageLabel(applicant.stage)}</option>
                  )}
                  {stages.map((stage) => (
                    <option key={stage} value={stage}>
                      {stageLabel(stage)}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </>
        )}
      </div>

      {error !== null && <ErrorBanner message={error} />}

      {applicant !== null && (
        <div className={styles.drawerBody}>
          <section className={styles.panel}>
            <h3 className={styles.panelTitle}>
              <FileText size={15} aria-hidden="true" />
              {strings.hrCv}
            </h3>
            {applicant.cvNodeId === null ? (
              <p className={styles.panelEmpty}>{strings.hrCvNone}</p>
            ) : applicant.cvTrashed ? (
              <p className={styles.panelEmpty}>{strings.hrCvTrashed}</p>
            ) : (
              <button
                type="button"
                className={styles.linkAction}
                onClick={() => void downloadCv()}
              >
                <Download size={15} />
                {applicant.cvFileName ?? strings.hrCvDownload}
              </button>
            )}
            {/* Attaching one is the record form's, which is also where it is
                replaced or taken off: one upload path and one set of failure
                sentences, rather than a second control here that would have to
                agree with it. */}
            {(applicant.cvNodeId === null || applicant.cvTrashed) && (
              <button
                type="button"
                className={styles.linkAction}
                onClick={() => onEdit(applicant)}
              >
                <Upload size={15} />
                {strings.hrCvAttach}
              </button>
            )}
          </section>

          {/* Somebody who took the job is not yet a colleague: moving a card
              recorded an outcome, and a board that also created people would
              create them on a mis-drop. The act is offered here and taken by a
              person (`docs/design/hr.md` § As built (B6.08c)). */}
          {applicant.stage === HIRED_STAGE && (
            <section className={styles.panel}>
              <h3 className={styles.panelTitle}>
                <UserCheck size={15} aria-hidden="true" />
                {strings.hrHired}
              </h3>
              <p className={styles.panelEmpty}>{strings.hrHiredExplainer}</p>
              <Button icon={<UserCheck size={15} />} onClick={() => onHire(applicant)}>
                {strings.hrHire}
              </Button>
            </section>
          )}

          <section className={styles.panel}>
            <h3 className={styles.panelTitle}>{strings.hrNotes}</h3>
            <div className={styles.composer}>
              <textarea
                className={styles.textarea}
                rows={3}
                value={note}
                placeholder={strings.hrNotePlaceholder}
                onChange={(e) => setNote(e.target.value)}
              />
              <Button onClick={() => void addNote()} disabled={busy || note.trim() === ""}>
                {strings.hrAddNote}
              </Button>
            </div>
            {notes.length === 0 ? (
              <p className={styles.panelEmpty}>{strings.hrNotesEmpty}</p>
            ) : (
              <ul className={styles.entries}>
                {notes.map((written) => (
                  <li key={written.id} className={styles.entry}>
                    <div className={styles.entryHead}>
                      <span className={styles.entryWhen}>{momentLabel(written.createdAt)}</span>
                    </div>
                    <p className={styles.entryBody}>{written.body}</p>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section className={styles.panel}>
            <h3 className={styles.panelTitle}>{strings.hrRetention}</h3>
            <p className={styles.panelEmpty}>
              {strings.hrRetentionUntil(dayLabel(applicant.retainUntil))}
            </p>
            {applicant.retentionExpired && <Chip tone="bad">{strings.hrRetentionExpired}</Chip>}
            <p className={styles.panelEmpty}>{strings.hrRetentionExplainer}</p>
            <Button
              variant="danger"
              icon={<Trash2 size={15} />}
              onClick={() => void erase()}
              disabled={busy}
            >
              {strings.hrErase}
            </Button>
          </section>

          <p className={styles.drawerFact}>{strings.hrAppliedOn(momentLabel(applicant.createdAt))}</p>
        </div>
      )}
    </aside>
  );
}
