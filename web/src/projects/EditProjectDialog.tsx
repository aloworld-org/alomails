import { useState } from "react";
import { BriefcaseBusiness } from "lucide-react";

import { strings } from "../i18n";
import { DialogFrame, Field } from "./parts";
import type { Project, ProjectDraft } from "./types";

const statuses: Project["status"][] = ["planned", "active", "on_hold", "completed", "cancelled"];

function statusLabel(status: Project["status"]): string {
  return {
    planned: strings.projectsStatusPlanned,
    active: strings.projectsStatusActive,
    on_hold: strings.projectsStatusOnHold,
    completed: strings.projectsStatusCompleted,
    cancelled: strings.projectsStatusCancelled,
  }[status];
}

export function EditProjectDialog({ project, onClose, onSave }: {
  project: Project;
  onClose: () => void;
  onSave: (draft: ProjectDraft) => Promise<void>;
}) {
  const [name, setName] = useState(project.name);
  const [description, setDescription] = useState(project.description ?? "");
  const [status, setStatus] = useState(project.status);
  const [startsOn, setStartsOn] = useState(project.startsOn ?? "");
  const [targetOn, setTargetOn] = useState(project.targetOn ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const datesValid = startsOn === "" || targetOn === "" || targetOn >= startsOn;

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await onSave({
        name: name.trim(),
        description: description.trim() || null,
        status,
        startsOn: startsOn || null,
        targetOn: targetOn || null,
      });
    } catch {
      setError(strings.projectsSaveFailed);
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={BriefcaseBusiness}
      title={strings.projectsDetailsTitle}
      subtitle={strings.projectsDetailsSubtitle}
      error={error}
      busy={busy}
      canSubmit={name.trim() !== "" && datesValid}
      submitLabel={strings.projectsSave}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <Field label={strings.projectsName}>
        <input autoFocus className="min-h-11 w-full rounded-md border border-default bg-surface px-3 py-2 text-sm text-primary focus-visible:outline-2 focus-visible:outline-accent" value={name} maxLength={120} onChange={(event) => setName(event.target.value)} />
      </Field>
      <Field label={strings.projectsDescription}>
        <textarea className="min-h-24 w-full resize-y rounded-md border border-default bg-surface px-3 py-2 text-sm leading-6 text-primary focus-visible:outline-2 focus-visible:outline-accent" value={description} maxLength={2000} onChange={(event) => setDescription(event.target.value)} />
      </Field>
      <Field label={strings.projectsStatus}>
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
          {statuses.map((option) => (
            <button
              key={option}
              type="button"
              aria-pressed={status === option}
              className={`min-h-10 rounded-md border px-3 py-2 text-sm font-medium transition-colors focus-visible:outline-2 focus-visible:outline-accent ${status === option ? "border-accent bg-accent-soft text-accent" : "border-default bg-surface text-secondary hover:bg-raised hover:text-primary"}`}
              onClick={() => setStatus(option)}
            >
              {statusLabel(option)}
            </button>
          ))}
        </div>
      </Field>
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label={strings.projectsStartsOn}>
          <input type="date" className="min-h-11 w-full rounded-md border border-default bg-surface px-3 py-2 text-sm text-primary accent-accent focus-visible:outline-2 focus-visible:outline-accent" value={startsOn} onChange={(event) => setStartsOn(event.target.value)} />
        </Field>
        <Field label={strings.projectsTargetOn} error={datesValid ? undefined : strings.projectsDatesInvalid}>
          <input type="date" className="min-h-11 w-full rounded-md border border-default bg-surface px-3 py-2 text-sm text-primary accent-accent focus-visible:outline-2 focus-visible:outline-accent" value={targetOn} onChange={(event) => setTargetOn(event.target.value)} />
        </Field>
      </div>
    </DialogFrame>
  );
}
