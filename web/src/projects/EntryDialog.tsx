// One cell of the week: the hours somebody worked on a project on a day, and
// what they were doing.
//
// The same form writes a new entry and corrects an existing one, because they
// are the same statement — the server's edit shape is "the entry now says
// this", a whole record and not a patch, so a form that showed a subset would
// be quietly clearing what it did not show.
//
// Two fields are deliberately absent from a correction, and both absences are
// the store's rules rather than this screen's: the **project** (moving an hour
// to another engagement changes who is billed for it, which is a new record,
// not a correction) and the **rate** (a snapshot taken when the work was
// written down; repricing it is not a correction of what happened either).
//
// A duration is typed the way people type one — `90`, `1:30`, `1,5` — and read
// back the same way. It is minutes on the wire, always; no float ever reaches
// the API.
import { useState } from "react";
import { Clock } from "lucide-react";

import { Checkbox, Field, Input, useDialogs } from "../ds";
import { strings } from "../i18n";
import { projectsMessage, useProjectsApi } from "./api";
import { dayLabel, durationInput, parseDuration } from "./format";
import { DialogFrame } from "./parts";
import type { Project, TimeEntry } from "./types";

export function EntryDialog({
  entry,
  projectId,
  workDate,
  projects,
  onClose,
  onSaved,
}: {
  /** The entry being corrected, or `null` when writing a new one. */
  entry: TimeEntry | null;
  /** The project a new entry is for — the row of the grid cell that was
   *  clicked. Ignored when correcting: the project of an entry cannot move. */
  projectId: string;
  /** The day a new entry is for — the column of the grid cell. */
  workDate: string;
  projects: Project[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const api = useProjectsApi();
  const dialogs = useDialogs();

  const [duration, setDuration] = useState(entry === null ? "" : durationInput(entry.minutes));
  const [billable, setBillable] = useState(entry?.billable ?? true);
  const [note, setNote] = useState(entry?.note ?? "");
  const [day, setDay] = useState(entry?.workDate ?? workDate);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const project = projects.find((p) => p.id === (entry?.projectId ?? projectId));
  const minutes = parseDuration(duration);
  const durationError =
    duration.trim() !== "" && minutes === null ? strings.projectsDurationInvalid : undefined;
  const canSubmit = minutes !== null && day !== "";

  async function save() {
    if (minutes === null) return;
    setBusy(true);
    setError(null);
    try {
      const draft = { workDate: day, minutes, billable, note };
      if (entry === null) await api.logTime({ projectId, ...draft });
      else await api.updateTime(entry.id, draft);
      onSaved();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsSaveFailed));
      setBusy(false);
    }
  }

  async function remove() {
    if (entry === null) return;
    const sure = await dialogs.confirm({
      title: strings.projectsDeleteEntryTitle,
      message: strings.projectsDeleteEntryBody,
      confirmLabel: strings.projectsDeleteEntry,
      danger: true,
    });
    if (!sure) return;
    setBusy(true);
    setError(null);
    try {
      await api.deleteTime(entry.id);
      onSaved();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsSaveFailed));
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Clock}
      title={project?.name ?? strings.projectsProject}
      subtitle={dayLabel(day)}
      error={error}
      busy={busy}
      canSubmit={canSubmit}
      submitLabel={strings.projectsSave}
      extraAction={
        entry === null
          ? undefined
          : { label: strings.projectsDeleteEntry, onClick: () => void remove() }
      }
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <div className="flex flex-wrap gap-3 [&>*]:min-w-40 [&>*]:flex-1">
        <Field
          label={strings.projectsDuration}
          hint={strings.projectsDurationHint}
          error={durationError}
        >
          {/* Focused on open: this is the one field a cell was clicked to fill
              in, and anything else costs the person a second click per day of
              the week. */}
          {(control) => (
            <Input
              id={control.id}
              aria-describedby={control["aria-describedby"]}
              invalid={control.invalid}
              autoFocus
              inputMode="text"
              value={duration}
              onChange={(e) => setDuration(e.target.value)}
            />
          )}
        </Field>
        <Field label={strings.projectsDay}>
          {(control) => (
            <Input
              id={control.id}
              aria-describedby={control["aria-describedby"]}
              type="date"
              value={day}
              onChange={(e) => setDay(e.target.value)}
            />
          )}
        </Field>
      </div>

      <Checkbox
        checked={billable}
        onChange={setBillable}
        label={strings.projectsBillable}
      />

      {/* Multi-line: stays a bare textarea until ds/ has a multi-line control.
          The Field still binds it and announces the hint. */}
      <Field label={strings.projectsNote} hint={strings.projectsNoteHint}>
        {(control) => (
          <textarea
            id={control.id}
            aria-describedby={control["aria-describedby"]}
            className="min-h-20 w-full resize-y rounded-md border border-default bg-surface px-3 py-2 text-sm text-primary focus-visible:outline-2 focus-visible:outline-accent"
            value={note}
            maxLength={500}
            onChange={(e) => setNote(e.target.value)}
          />
        )}
      </Field>
    </DialogFrame>
  );
}
