import { useState } from "react";
import { BriefcaseBusiness } from "lucide-react";

import { strings } from "../i18n";
import { DialogFrame, Field } from "./parts";
import { ProjectStatusSchedule } from "./ProjectStatusSchedule";
import type { Project, ProjectDraft } from "./types";

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
      <ProjectStatusSchedule status={status} startsOn={startsOn} targetOn={targetOn} datesValid={datesValid} onStatusChange={setStatus} onStartsOnChange={setStartsOn} onTargetOnChange={setTargetOn} />
    </DialogFrame>
  );
}
