// The "New task" modal: a proper create form over the real task fields (name,
// project, assignee, due date, priority, description, subtasks). Everything it
// sets is persisted in one createTask call (plus a subtask call per checklist
// line). "Create another" keeps it open and clears it for fast entry.
import { useRef, useState } from "react";
import type { FormEvent } from "react";
import { FolderClosed, HardDrive, LoaderCircle, Paperclip, Plus, SquareCheckBig, Trash2, Upload, User, X } from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient, type DriveNodeDto, type TaskPriority, type TaskProject } from "../jmap";
import { Button, DatePicker } from "../ds";
import { DriveAttachmentPicker } from "./DriveAttachmentPicker";

interface Props {
  projects: TaskProject[];
  defaultProjectId?: string | undefined;
  defaultStatus?: string | undefined;
  defaultDueDate?: string | undefined;
  onClose: () => void;
  onCreated: () => void;
}

const PRIOS: { key: TaskPriority; label: string; dot: string; active: string }[] = [
  { key: "low", label: "", dot: "bg-success", active: "border-success text-success" },
  { key: "medium", label: "", dot: "bg-warning", active: "border-warning text-warning" },
  { key: "high", label: "", dot: "bg-danger", active: "border-danger text-danger" },
];

const fieldClass = "w-full rounded-lg border border-default bg-surface px-4 py-3 text-base text-primary outline-none placeholder:text-tertiary focus:border-accent focus:ring-3 focus:ring-accent/15";
const labelClass = "text-sm font-semibold text-primary";

export function NewTaskModal({ projects, defaultProjectId, defaultStatus, defaultDueDate, onClose, onCreated }: Props) {
  const client = useJmapClient();
  const personal = projects.find((p) => p.kind === "personal") ?? projects[0];
  const [name, setName] = useState("");
  const [projectId, setProjectId] = useState(defaultProjectId ?? personal?.id ?? "");
  const [assignee, setAssignee] = useState("");
  const [dueDate, setDueDate] = useState(defaultDueDate ?? "");
  const [priority, setPriority] = useState<TaskPriority>("none");
  const [description, setDescription] = useState("");
  const [subtasks, setSubtasks] = useState<string[]>([]);
  const [createAnother, setCreateAnother] = useState(false);
  const [busy, setBusy] = useState(false);
  const [deviceFiles, setDeviceFiles] = useState<File[]>([]);
  const [driveFiles, setDriveFiles] = useState<DriveNodeDto[]>([]);
  const [driveOpen, setDriveOpen] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  const prioLabel = (k: TaskPriority) =>
    k === "low"
      ? strings.taskPrioLow
      : k === "medium"
        ? strings.taskPrioMedium
        : strings.taskPrioHigh;

  function reset() {
    setName("");
    setAssignee("");
    setDueDate("");
    setPriority("none");
    setDescription("");
    setSubtasks([]);
    setDeviceFiles([]);
    setDriveFiles([]);
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    const title = name.trim();
    if (title === "" || projectId === "" || busy) return;
    setBusy(true);
    try {
      const input: Parameters<typeof client.createTask>[0] = { projectId, title };
      if (description.trim() !== "") input.description = description.trim();
      if (assignee.trim() !== "") input.assignee = assignee.trim();
      if (dueDate !== "") input.dueAt = `${dueDate}T12:00:00Z`;
      if (priority !== "none") input.priority = priority;
      if (defaultStatus !== undefined && defaultStatus !== "todo") input.status = defaultStatus;
      const created = await client.createTask(input);
      for (const st of subtasks.map((s) => s.trim()).filter((s) => s !== "")) {
        await client.addSubtask(created.id, st);
      }
      for (const file of deviceFiles) {
        const { blobId, size } = await client.uploadFile(file);
        await client.addTaskAttachment(created.id, blobId, file.name, size);
      }
      for (const file of driveFiles) {
        if (file.blobId !== null) {
          await client.addTaskAttachment(created.id, file.blobId, file.name, file.size);
        }
      }
      onCreated();
      if (createAnother) reset();
      else onClose();
    } catch {
      /* leave the form up so nothing is lost */
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 z-modal flex items-center justify-center bg-overlay p-6" role="dialog" aria-modal="true" onMouseDown={onClose}>
      <form className="flex max-h-[92vh] w-full max-w-2xl flex-col overflow-hidden rounded-2xl bg-surface shadow-xl" onSubmit={submit} onMouseDown={(e) => e.stopPropagation()}>
        <div className="flex shrink-0 items-start gap-3 border-b border-subtle px-6 py-5">
          <span className="flex size-11 shrink-0 items-center justify-center rounded-xl bg-accent-tint text-accent-hover">
            <SquareCheckBig size={20} />
          </span>
          <div className="min-w-0 flex-1">
            <h2 className="m-0 text-xl font-bold text-primary">{strings.taskNew}</h2>
            <p className="mt-0.5 text-sm text-secondary">{strings.taskNewSubtitle}</p>
          </div>
          <button type="button" className="shrink-0 rounded-lg p-2 text-tertiary hover:bg-raised hover:text-primary" onClick={onClose} aria-label={strings.taskCancel}>
            <X size={18} />
          </button>
        </div>

        <div className="flex min-h-0 flex-col gap-5 overflow-y-auto px-6 py-5">
          <label className="flex min-w-0 flex-col gap-1.5">
            <span className={labelClass}>
              {strings.taskColName} <span className="text-danger">*</span>
            </span>
            <input
              className={fieldClass}
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={strings.taskNamePlaceholder}
              autoFocus
              required
            />
          </label>

          <div className="grid grid-cols-1 gap-5 sm:grid-cols-2">
            <label className="flex min-w-0 flex-col gap-1.5">
              <span className={labelClass}>{strings.taskColProject}</span>
              <span className="flex h-11 min-w-0 items-center gap-2 rounded-lg border border-default bg-surface px-3 focus-within:border-accent focus-within:ring-3 focus-within:ring-accent/15">
                <FolderClosed size={16} className="shrink-0 text-tertiary" />
                <select className="min-w-0 flex-1 bg-transparent text-primary outline-none" value={projectId} onChange={(e) => setProjectId(e.target.value)}>
                  {projects.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </span>
            </label>
            <label className="flex min-w-0 flex-col gap-1.5">
              <span className={labelClass}>{strings.taskColAssignee}</span>
              <span className="flex h-11 min-w-0 items-center gap-2 rounded-lg border border-default bg-surface px-3 focus-within:border-accent focus-within:ring-3 focus-within:ring-accent/15">
                <User size={16} className="shrink-0 text-tertiary" />
                <input
                  className="min-w-0 flex-1 bg-transparent text-primary outline-none placeholder:text-tertiary"
                  value={assignee}
                  onChange={(e) => setAssignee(e.target.value)}
                  placeholder={strings.taskAssigneePlaceholder}
                  inputMode="email"
                  autoCapitalize="none"
                  autoCorrect="off"
                  spellCheck={false}
                />
              </span>
            </label>
          </div>

          <div className="grid grid-cols-1 gap-5 sm:grid-cols-2">
            <div className="flex min-w-0 flex-col gap-1.5">
              <span className={labelClass}>{strings.taskColDue}</span>
              <DatePicker value={dueDate} onChange={setDueDate} placeholder={strings.taskColDue} />
            </div>
            <div className="flex min-w-0 flex-col gap-1.5">
              <span className={labelClass}>{strings.taskColPriority}</span>
              <div className="grid grid-cols-3 gap-2">
                {PRIOS.map((p) => (
                  <button
                    key={p.key}
                    type="button"
                    className={`inline-flex h-11 items-center justify-center gap-1.5 rounded-lg border bg-surface px-3 text-sm font-medium transition-colors hover:bg-raised ${priority === p.key ? p.active : "border-default text-secondary"}`}
                    onClick={() => setPriority((cur) => (cur === p.key ? "none" : p.key))}
                  >
                    <span className={`size-2 rounded-full ${p.dot}`} aria-hidden />
                    {prioLabel(p.key)}
                  </button>
                ))}
              </div>
            </div>
          </div>

          <label className="flex min-w-0 flex-col gap-1.5">
            <span className={labelClass}>{strings.taskDescription}</span>
            <textarea
              className={`${fieldClass} min-h-24 resize-y`}
              rows={3}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder={strings.taskDescriptionPlaceholder}
            />
          </label>

          <div className="flex min-w-0 flex-col gap-2">
            <span className={labelClass}>{strings.taskSubtasks}</span>
            {subtasks.map((st, i) => (
              <span key={i} className="flex items-center gap-2">
                <input
                  className="min-w-0 flex-1 rounded-lg border border-default bg-surface px-3 py-2.5 text-primary outline-none placeholder:text-tertiary focus:border-accent"
                  value={st}
                  onChange={(e) =>
                    setSubtasks((cur) => cur.map((v, j) => (j === i ? e.target.value : v)))
                  }
                  placeholder={strings.taskAddSubtask}
                  autoFocus
                />
                <button
                  type="button"
                  className="shrink-0 rounded-lg p-2 text-tertiary hover:bg-raised hover:text-danger"
                  onClick={() => setSubtasks((cur) => cur.filter((_, j) => j !== i))}
                  aria-label={strings.taskDelete}
                >
                  <Trash2 size={15} />
                </button>
              </span>
            ))}
            <button
              type="button"
              className="inline-flex h-11 w-full items-center justify-center gap-2 rounded-lg border border-default bg-surface px-4 text-sm font-semibold text-accent hover:bg-raised"
              onClick={() => setSubtasks((cur) => [...cur, ""])}
            >
              <Plus size={16} /> {strings.taskAddSubtask}
            </button>
          </div>

          <div className="flex min-w-0 flex-col gap-2">
            <span className={labelClass}>{strings.taskAttachments}</span>
            <div className="flex flex-wrap gap-2">
              <input
                ref={fileRef}
                className="sr-only"
                type="file"
                multiple
                onChange={(event) => {
                  if (event.target.files !== null) {
                    setDeviceFiles((current) => [...current, ...Array.from(event.target.files ?? [])]);
                  }
                  event.target.value = "";
                }}
              />
              <button type="button" className="inline-flex h-10 items-center gap-2 rounded-lg bg-raised px-4 text-sm font-medium text-primary hover:bg-accent-tint" onClick={() => fileRef.current?.click()}>
                <Upload size={16} /> {strings.taskAddAttachment}
              </button>
              <button type="button" className="inline-flex h-10 items-center gap-2 rounded-lg bg-raised px-4 text-sm font-medium text-primary hover:bg-accent-tint" onClick={() => setDriveOpen(true)}>
                <HardDrive size={16} /> {strings.taskChooseFromDrive}
              </button>
            </div>
            {(deviceFiles.length > 0 || driveFiles.length > 0) && (
              <div className="flex flex-wrap gap-2">
                {deviceFiles.map((file, index) => (
                  <span key={`${file.name}-${index}`} className="inline-flex min-w-0 items-center gap-1.5 rounded-full bg-raised px-3 py-1.5 text-sm text-secondary">
                    <Paperclip size={14} className="shrink-0" /><span className="max-w-48 truncate">{file.name}</span>
                    <button className="rounded-full p-0.5 hover:bg-surface hover:text-danger" type="button" onClick={() => setDeviceFiles((current) => current.filter((_, i) => i !== index))} aria-label={strings.taskDelete}><X size={13} /></button>
                  </span>
                ))}
                {driveFiles.map((file) => (
                  <span key={file.id} className="inline-flex min-w-0 items-center gap-1.5 rounded-full bg-raised px-3 py-1.5 text-sm text-secondary">
                    <HardDrive size={14} className="shrink-0" /><span className="max-w-48 truncate">{file.name}</span>
                    <button className="rounded-full p-0.5 hover:bg-surface hover:text-danger" type="button" onClick={() => setDriveFiles((current) => current.filter((item) => item.id !== file.id))} aria-label={strings.taskDelete}><X size={13} /></button>
                  </span>
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-t border-subtle px-6 py-4">
          <label className="inline-flex items-center gap-2 text-sm text-secondary">
            <input
              type="checkbox"
              checked={createAnother}
              onChange={(e) => setCreateAnother(e.target.checked)}
            />
            {strings.taskCreateAnother}
          </label>
          <div className="flex items-center gap-2">
            <button type="button" className="h-11 rounded-lg bg-raised px-5 text-base font-medium text-primary hover:bg-accent-tint disabled:opacity-60" onClick={onClose} disabled={busy}>
              {strings.taskCancel}
            </button>
            <Button
              type="submit"
              className="h-11 min-w-36 px-5 font-semibold disabled:opacity-60 [&_svg]:shrink-0"
              disabled={busy || name.trim() === "" || projectId === ""}
              icon={busy ? <LoaderCircle size={16} /> : <Plus size={16} />}
            >
              {busy ? strings.taskCreating : strings.taskCreate}
            </Button>
          </div>
        </div>
      </form>
      {driveOpen && (
        <DriveAttachmentPicker
          onClose={() => setDriveOpen(false)}
          onAttach={async (nodes) => {
            setDriveFiles((current) => {
              const merged = new Map(current.map((node) => [node.id, node]));
              for (const node of nodes) merged.set(node.id, node);
              return Array.from(merged.values());
            });
            setDriveOpen(false);
          }}
        />
      )}
    </div>
  );
}
