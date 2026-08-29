// The "New task" modal: a proper create form over the real task fields (name,
// project, assignee, due date, priority, description, subtasks). Everything it
// sets is persisted in one createTask call (plus a subtask call per checklist
// line). "Create another" keeps it open and clears it for fast entry.
//
// The dialog is `ds/Modal` (D2.11). The hand-rolled overlay this replaces had
// no focus trap, no Escape handling, and a `z-modal` class the theme does not
// generate — so it shipped with no z-index at all and Tab walked out of it
// onto the page behind.
import { useId, useRef, useState } from "react";
import type { FormEvent } from "react";
import { HardDrive, LoaderCircle, Paperclip, Plus, SquareCheckBig, Trash2, Upload, X } from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient, type DriveNodeDto, type TaskPriority, type TaskProject } from "../jmap";
import { Button, Checkbox, Chip, DatePicker, Field, IconButton, Input, Modal, Select } from "../ds";
import { DriveAttachmentPicker } from "./DriveAttachmentPicker";

interface Props {
  projects: TaskProject[];
  defaultProjectId?: string | undefined;
  defaultStatus?: string | undefined;
  defaultDueDate?: string | undefined;
  onClose: () => void;
  onCreated: () => void;
}

const PRIOS: { key: TaskPriority; dot: string; active: string }[] = [
  { key: "low", dot: "bg-success", active: "border-success text-success" },
  { key: "medium", dot: "bg-warning", active: "border-warning text-warning" },
  { key: "high", dot: "bg-danger", active: "border-danger text-danger" },
];

const labelClass = "text-sm font-semibold text-primary";

export function NewTaskModal({ projects, defaultProjectId, defaultStatus, defaultDueDate, onClose, onCreated }: Props) {
  const client = useJmapClient();
  const formId = useId();
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
    <>
      <Modal
        title={strings.taskNew}
        onClose={onClose}
        icon={<SquareCheckBig size={19} />}
        wide
        actions={<IconButton label={strings.taskCancel} icon={<X size={18} />} onClick={onClose} />}
        footer={
          <>
            <Checkbox checked={createAnother} onChange={setCreateAnother} label={strings.taskCreateAnother} />
            <span className="flex-1" />
            <Button variant="ghost" onClick={onClose} disabled={busy}>
              {strings.taskCancel}
            </Button>
            <Button
              type="submit"
              form={formId}
              disabled={busy || name.trim() === "" || projectId === ""}
              icon={busy ? <LoaderCircle size={16} /> : <Plus size={16} />}
            >
              {busy ? strings.taskCreating : strings.taskCreate}
            </Button>
          </>
        }
      >
        <p className="m-0 text-sm text-tertiary">{strings.taskNewSubtitle}</p>
        <form id={formId} className="flex flex-col gap-5" onSubmit={submit}>
          <Field label={strings.taskColName}>
            {(control) => (
              <Input
                {...control}
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={strings.taskNamePlaceholder}
                required
              />
            )}
          </Field>

          <div className="grid grid-cols-1 gap-5 sm:grid-cols-2">
            <Field label={strings.taskColProject}>
              {(control) => (
                <Select {...control} fullWidth value={projectId} onChange={(e) => setProjectId(e.target.value)}>
                  {projects.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
            <Field label={strings.taskColAssignee}>
              {(control) => (
                <Input
                  {...control}
                  value={assignee}
                  onChange={(e) => setAssignee(e.target.value)}
                  placeholder={strings.taskAssigneePlaceholder}
                  inputMode="email"
                  autoCapitalize="none"
                  autoCorrect="off"
                  spellCheck={false}
                />
              )}
            </Field>
          </div>

          <div className="grid grid-cols-1 gap-5 sm:grid-cols-2">
            <Field label={strings.taskColDue}>
              {(control) => <DatePicker id={control.id} value={dueDate} onChange={setDueDate} placeholder={strings.taskColDue} />}
            </Field>
            <div className="flex min-w-0 flex-col gap-2">
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

          {/* The textarea stays bare — there is no multi-line control in `ds/`
              yet (tasks joins the areas waiting for one) — but it takes the
              Field's id and description so it is at least announced. */}
          <Field label={strings.taskDescription}>
            {(control) => (
              <textarea
                id={control.id}
                aria-describedby={control["aria-describedby"]}
                className="min-h-24 w-full resize-y rounded-md border border-default bg-surface px-3 py-2.5 text-base text-primary outline-none placeholder:text-tertiary focus:border-accent"
                rows={3}
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder={strings.taskDescriptionPlaceholder}
              />
            )}
          </Field>

          <div className="flex min-w-0 flex-col gap-2">
            <span className={labelClass}>{strings.taskSubtasks}</span>
            {subtasks.map((st, i) => (
              <span key={i} className="flex items-center gap-2">
                <Input
                  value={st}
                  onChange={(e) =>
                    setSubtasks((cur) => cur.map((v, j) => (j === i ? e.target.value : v)))
                  }
                  placeholder={strings.taskAddSubtask}
                  aria-label={strings.taskAddSubtask}
                  autoFocus
                />
                <IconButton
                  label={strings.taskDelete}
                  icon={<Trash2 size={15} />}
                  onClick={() => setSubtasks((cur) => cur.filter((_, j) => j !== i))}
                />
              </span>
            ))}
            <Button variant="secondary" block icon={<Plus size={16} />} onClick={() => setSubtasks((cur) => [...cur, ""])}>
              {strings.taskAddSubtask}
            </Button>
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
              <Button variant="secondary" icon={<Upload size={16} />} onClick={() => fileRef.current?.click()}>
                {strings.taskAddAttachment}
              </Button>
              <Button variant="secondary" icon={<HardDrive size={16} />} onClick={() => setDriveOpen(true)}>
                {strings.taskChooseFromDrive}
              </Button>
            </div>
            {(deviceFiles.length > 0 || driveFiles.length > 0) && (
              <div className="flex flex-wrap gap-2">
                {deviceFiles.map((file, index) => (
                  <Chip
                    key={`${file.name}-${index}`}
                    onRemove={() => setDeviceFiles((current) => current.filter((_, i) => i !== index))}
                    removeLabel={`${strings.taskDelete} ${file.name}`}
                  >
                    <Paperclip size={14} className="shrink-0" />
                    <span className="max-w-48 truncate">{file.name}</span>
                  </Chip>
                ))}
                {driveFiles.map((file) => (
                  <Chip
                    key={file.id}
                    onRemove={() => setDriveFiles((current) => current.filter((item) => item.id !== file.id))}
                    removeLabel={`${strings.taskDelete} ${file.name}`}
                  >
                    <HardDrive size={14} className="shrink-0" />
                    <span className="max-w-48 truncate">{file.name}</span>
                  </Chip>
                ))}
              </div>
            )}
          </div>
        </form>
      </Modal>
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
    </>
  );
}
