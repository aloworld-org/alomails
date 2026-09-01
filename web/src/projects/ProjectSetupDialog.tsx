import { useState } from "react";
import type { ReactNode } from "react";
import { CalendarDays, FolderKanban, ListChecks, MessageSquare, Sparkles } from "lucide-react";

import { Field, Input } from "../ds";
import { strings } from "../i18n";
import { DialogFrame } from "./parts";
import { projectsMessage, useProjectsApi } from "./api";
import type { ProjectSetup } from "./types";

interface Props {
  projectId: string;
  projectName: string;
  onClose: () => void;
  onSaved: (setup: ProjectSetup) => void;
}

function initialKickoff(): string {
  const date = new Date();
  date.setDate(date.getDate() + 1);
  date.setHours(10, 0, 0, 0);
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, 16);
}

function Choice({
  icon,
  label,
  detail,
  checked,
  onChange,
}: {
  icon: ReactNode;
  label: string;
  detail: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-3 rounded-xl border border-subtle bg-surface p-4 transition-colors hover:bg-raised">
      <input
        type="checkbox"
        className="mt-1 size-4 accent-[var(--accent)]"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span className="rounded-lg bg-accent-soft p-2 text-accent" aria-hidden="true">
        {icon}
      </span>
      <span className="min-w-0">
        <span className="block text-sm font-semibold text-primary">{label}</span>
        <span className="mt-1 block text-xs leading-5 text-secondary">{detail}</span>
      </span>
    </label>
  );
}

/** Reviewable optional setup. Opening or cancelling it creates nothing. */
export function ProjectSetupDialog({ projectId, projectName, onClose, onSaved }: Props) {
  const api = useProjectsApi();
  const [files, setFiles] = useState(true);
  const [chat, setChat] = useState(true);
  const [tasks, setTasks] = useState(true);
  const [kickoff, setKickoff] = useState(false);
  const [startsAt, setStartsAt] = useState(initialKickoff);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const starts = new Date(startsAt);
  const validKickoff = !kickoff || !Number.isNaN(starts.getTime());

  async function submit() {
    setBusy(true);
    setError(null);
    try {
      const setup = await api.setupProject(projectId, {
        createFilesSpace: files,
        createChatRoom: chat,
        starterTasks: tasks
          ? [strings.projectsSetupTaskScope, strings.projectsSetupTaskKickoff, strings.projectsSetupTaskPlan]
          : [],
        ...(kickoff && validKickoff
          ? {
              kickoff: {
                startsAt: starts.toISOString(),
                endsAt: new Date(starts.getTime() + 60 * 60_000).toISOString(),
                timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
              },
            }
          : {}),
      });
      onSaved(setup);
    } catch (reason) {
      setError(projectsMessage(reason, strings.projectsSetupFailed));
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Sparkles}
      title={strings.projectsSetupTitle}
      subtitle={strings.projectsSetupSubtitle(projectName)}
      error={error}
      busy={busy}
      canSubmit={validKickoff && (files || chat || tasks || kickoff)}
      submitLabel={strings.projectsSetupConfirm}
      onClose={onClose}
      onSubmit={() => void submit()}
    >
      <div className="grid grid-cols-2 gap-3 max-sm:grid-cols-1">
        <Choice icon={<FolderKanban size={18} />} label={strings.projectsSetupFiles} detail={strings.projectsSetupFilesDetail} checked={files} onChange={setFiles} />
        <Choice icon={<MessageSquare size={18} />} label={strings.projectsSetupChat} detail={strings.projectsSetupChatDetail} checked={chat} onChange={setChat} />
        <Choice icon={<ListChecks size={18} />} label={strings.projectsSetupTasks} detail={strings.projectsSetupTasksDetail} checked={tasks} onChange={setTasks} />
        <Choice icon={<CalendarDays size={18} />} label={strings.projectsSetupKickoff} detail={strings.projectsSetupKickoffDetail} checked={kickoff} onChange={setKickoff} />
      </div>
      {kickoff && (
        <Field label={strings.projectsSetupKickoffTime}>
          {(control) => (
            <Input {...control} type="datetime-local" value={startsAt} onChange={(event) => setStartsAt(event.target.value)} />
          )}
        </Field>
      )}
      <p className="m-0 rounded-lg bg-secondary px-3 py-2 text-xs leading-5 text-secondary">
        {strings.projectsSetupReviewNote}
      </p>
    </DialogFrame>
  );
}
