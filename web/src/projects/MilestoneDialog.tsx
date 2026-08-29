// One milestone: what the date is for, and which day it is.
//
// The same form plans a new milestone and corrects an existing one, because a
// milestone is one whole statement — the server's edit shape is "the milestone
// now says this" — so a form that showed a subset would be quietly clearing
// what it did not show. The entry form (B3.04) reads the same way for the same
// reason.
//
// **Reached is not a field here.** Closing a deliverable is its own action on
// the timeline, with its own line in the audit trail; a form that could close a
// milestone while fixing a typo would file a closed deliverable as a spelling
// correction (`docs/design/projects.md`, "Milestones and templates" — a
// milestone is done when a human says so).
import { useState } from "react";
import { Flag } from "lucide-react";

import { Field, Input, useDialogs } from "../ds";
import { strings } from "../i18n";
import { projectsMessage, useProjectsApi } from "./api";
import { DialogFrame } from "./parts";
import type { Milestone } from "./types";

export function MilestoneDialog({
  milestone,
  projectId,
  projectName,
  defaultDay,
  onClose,
  onSaved,
}: {
  /** The milestone being corrected, or `null` when planning a new one. */
  milestone: Milestone | null;
  /** The project a new milestone belongs to. A milestone never moves board:
   *  the plan it is part of is the project's. */
  projectId: string;
  projectName: string;
  /** `YYYY-MM-DD` a new milestone opens on — today, so the field is never
   *  empty, and always the caller's own day rather than the server's. */
  defaultDay: string;
  onClose: () => void;
  onSaved: () => void;
}) {
  const api = useProjectsApi();
  const dialogs = useDialogs();

  const [name, setName] = useState(milestone?.name ?? "");
  const [dueOn, setDueOn] = useState(milestone?.dueOn ?? defaultDay);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // The name's own rules — blank, too long — are the server's and are shown as
  // its sentence when it refuses. What is checked here is only that there is
  // something to send at all, so the button is not offered for an empty form.
  const canSubmit = name.trim() !== "" && dueOn !== "";

  async function save() {
    setBusy(true);
    setError(null);
    try {
      const draft = { name: name.trim(), dueOn };
      if (milestone === null) await api.createMilestone(projectId, draft);
      else await api.updateMilestone(milestone.id, draft);
      onSaved();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsSaveFailed));
      setBusy(false);
    }
  }

  async function remove() {
    if (milestone === null) return;
    const sure = await dialogs.confirm({
      title: strings.projectsMilestoneDeleteTitle,
      message: strings.projectsMilestoneDeleteBody,
      confirmLabel: strings.projectsMilestoneDelete,
      danger: true,
    });
    if (!sure) return;
    setBusy(true);
    setError(null);
    try {
      await api.deleteMilestone(milestone.id);
      onSaved();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsSaveFailed));
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Flag}
      title={milestone === null ? strings.projectsMilestoneNew : milestone.name}
      subtitle={projectName}
      error={error}
      busy={busy}
      canSubmit={canSubmit}
      submitLabel={strings.projectsSave}
      extraAction={
        milestone === null
          ? undefined
          : { label: strings.projectsMilestoneDelete, onClick: () => void remove() }
      }
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <Field label={strings.projectsMilestoneName} hint={strings.projectsMilestoneNameHint}>
        {(control) => (
          <Input
            id={control.id}
            aria-describedby={control["aria-describedby"]}
            autoFocus
            value={name}
            maxLength={120}
            onChange={(e) => setName(e.target.value)}
          />
        )}
      </Field>
      <Field label={strings.projectsMilestoneDue} hint={strings.projectsMilestoneDueHint}>
        {(control) => (
          <Input
            id={control.id}
            aria-describedby={control["aria-describedby"]}
            type="date"
            value={dueOn}
            onChange={(e) => setDueOn(e.target.value)}
          />
        )}
      </Field>
    </DialogFrame>
  );
}
