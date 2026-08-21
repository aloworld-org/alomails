// The task detail: a panel that slides in from the right and never navigates
// away (ADR 0021). Editable title/description, the status/priority/assignee/due
// fields, the source link (jump back to the email/event it came from), a
// subtask checklist, comments, and the activity history. Field edits persist on
// change; a change bubbles up so the board/list behind stays in sync.
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  AlignLeft,
  Ban,
  CalendarDays,
  CheckCircle2,
  Circle,
  Check,
  Download,
  FolderClosed,
  Link2,
  Paperclip,
  Play,
  Plus,
  Square,
  Tag,
  Timer,
  Trash2,
  User,
  X,
} from "lucide-react";

import { strings } from "../i18n";
import {
  useJmapClient,
  type Task,
  type TaskDetailData,
  type TaskInput,
  type TaskLabelDto,
  type TaskPriority,
} from "../jmap";
import { DatePicker, Spinner } from "../ds";
import { Avatar, COLUMNS, LABEL_PALETTE, statusColor } from "./parts";
import { projectsMessage, useProjectsApi } from "../projects/api";
import { announceTimerChanged, onTimerChanged } from "../projects/timerBus";
import type { RunningTimer } from "../projects/types";

/** Human file size (kB/MB) for the attachment rows. */
function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

interface Props {
  taskId: string;
  projectName?: string | undefined;
  onClose: () => void;
  /** Called after any change so the board/list can refresh. */
  onChanged: () => void;
}

export type TaskTimerState = "idle" | "this-task" | "another-task";

/** A task owns the stop action only when the running timer names that task.
 *  A project-only timer or a sibling task produces an explicit switch action:
 *  the current time is safely logged before tracking moves to this task. */
export function taskTimerState(timer: RunningTimer | null, taskId: string): TaskTimerState {
  if (timer === null) return "idle";
  return timer.taskId === taskId ? "this-task" : "another-task";
}

interface TaskTimerApi {
  stopTimer(): Promise<unknown>;
  startTimer(input: {
    projectId: string;
    taskId: string;
    note: string;
  }): Promise<RunningTimer>;
}

/** Persist the running entry before moving time tracking to another task. */
export async function changeTaskTimer(
  api: TaskTimerApi,
  state: TaskTimerState,
  task: Pick<Task, "id" | "projectId" | "title">,
): Promise<RunningTimer | null> {
  if (state === "this-task") {
    await api.stopTimer();
    return null;
  }
  if (state === "another-task") await api.stopTimer();
  return api.startTimer({ projectId: task.projectId, taskId: task.id, note: task.title });
}

export function TaskDetail({ taskId, projectName, onClose, onChanged }: Props) {
  const client = useJmapClient();
  const projectsApi = useProjectsApi();
  const navigate = useNavigate();
  const [data, setData] = useState<TaskDetailData | null>(null);
  const [newSub, setNewSub] = useState("");
  const [newComment, setNewComment] = useState("");
  const [uploading, setUploading] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  const [labelMenu, setLabelMenu] = useState(false);
  const [allLabels, setAllLabels] = useState<TaskLabelDto[]>([]);
  const [newLabel, setNewLabel] = useState("");
  const labelWrapRef = useRef<HTMLDivElement>(null);
  const [blockMenu, setBlockMenu] = useState(false);
  const [siblings, setSiblings] = useState<Task[]>([]);
  const blockWrapRef = useRef<HTMLDivElement>(null);
  const [runningTimer, setRunningTimer] = useState<RunningTimer | null>(null);
  const [timerBusy, setTimerBusy] = useState(false);
  const [timerError, setTimerError] = useState<string | null>(null);

  const loadTimer = useCallback(async () => {
    try {
      setRunningTimer(await projectsApi.timer());
    } catch {
      setRunningTimer(null);
    }
  }, [projectsApi]);

  useEffect(() => {
    void loadTimer();
    return onTimerChanged(() => void loadTimer());
  }, [loadTimer]);

  useEffect(() => {
    if (!labelMenu) return undefined;
    void client.taskLabels().then(setAllLabels).catch(() => setAllLabels([]));
    function down(e: PointerEvent) {
      if (labelWrapRef.current !== null && !labelWrapRef.current.contains(e.target as Node)) {
        setLabelMenu(false);
      }
    }
    document.addEventListener("pointerdown", down);
    return () => document.removeEventListener("pointerdown", down);
  }, [labelMenu, client]);

  useEffect(() => {
    if (!blockMenu) return undefined;
    if (data?.task.projectId !== undefined) {
      void client.tasks(data.task.projectId).then(setSiblings).catch(() => setSiblings([]));
    }
    function down(e: PointerEvent) {
      if (blockWrapRef.current !== null && !blockWrapRef.current.contains(e.target as Node)) {
        setBlockMenu(false);
      }
    }
    document.addEventListener("pointerdown", down);
    return () => document.removeEventListener("pointerdown", down);
  }, [blockMenu, client, data?.task.projectId]);

  const load = useCallback(async () => {
    try {
      setData(await client.taskDetail(taskId));
    } catch {
      /* keep what we have */
    }
  }, [client, taskId]);

  useEffect(() => {
    void load();
  }, [load]);

  if (data === null) {
    return (
      <div className="fixed inset-0 z-modal flex justify-end bg-overlay" onMouseDown={onClose}>
        <div
          className="flex h-full w-full max-w-xl flex-col bg-surface shadow-xl"
          onMouseDown={(e) => e.stopPropagation()}
        >
          <div className="flex flex-1 items-center justify-center">
            <Spinner size={20} />
          </div>
        </div>
      </div>
    );
  }

  const t = data.task;

  /** Persist the editable fields (everything except status, which is a move).
   *  In `next`, a field is unchanged when omitted, cleared when `null`. */
  async function save(next: {
    title?: string;
    description?: string | null;
    assignee?: string | null;
    dueAt?: string | null;
    priority?: TaskPriority;
  }) {
    const cur = data!.task;
    const input: TaskInput = { title: next.title ?? cur.title, priority: next.priority ?? cur.priority };
    const description = next.description === undefined ? cur.description : next.description;
    if (description) input.description = description;
    const assignee = next.assignee === undefined ? cur.assignee : next.assignee;
    if (assignee) input.assignee = assignee;
    const dueAt = next.dueAt === undefined ? cur.dueAt : next.dueAt;
    if (dueAt) input.dueAt = dueAt;
    try {
      await client.updateTask(cur.id, input);
      await load();
      onChanged();
    } catch {
      /* ignore */
    }
  }

  async function changeStatus(status: string) {
    try {
      await client.moveTask(t.id, status, t.position);
      await load();
      onChanged();
    } catch {
      /* ignore */
    }
  }

  async function uploadAttachment(file: File) {
    setUploading(true);
    try {
      const { blobId, size } = await client.uploadFile(file);
      await client.addTaskAttachment(t.id, blobId, file.name, size);
      await load();
    } catch {
      /* leave state as-is on failure */
    } finally {
      setUploading(false);
    }
  }

  async function downloadAttachment(attachmentId: string, filename: string) {
    try {
      const blob = await client.downloadTaskAttachment(t.id, attachmentId);
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = filename;
      a.click();
      URL.revokeObjectURL(url);
    } catch {
      /* ignore */
    }
  }

  async function toggleLabel(labelId: string, on: boolean) {
    try {
      if (on) await client.removeTaskLabel(t.id, labelId);
      else await client.addTaskLabel(t.id, labelId);
      await load();
      onChanged();
    } catch {
      /* ignore */
    }
  }

  async function toggleFollow() {
    try {
      await client.followTask(t.id, !data!.following);
      await load();
    } catch {
      /* ignore */
    }
  }

  async function addBlocker(dependsOn: string) {
    try {
      await client.addTaskDependency(t.id, dependsOn);
      setBlockMenu(false);
      await load();
      onChanged();
    } catch {
      /* ignore */
    }
  }

  async function removeBlocker(dependsOn: string) {
    try {
      await client.removeTaskDependency(t.id, dependsOn);
      await load();
      onChanged();
    } catch {
      /* ignore */
    }
  }

  async function createAndAddLabel() {
    const name = newLabel.trim();
    if (name === "") return;
    try {
      const color = LABEL_PALETTE[allLabels.length % LABEL_PALETTE.length];
      const created = await client.createTaskLabel(name, color);
      await client.addTaskLabel(t.id, created.id);
      setNewLabel("");
      setAllLabels((cur) => [...cur, created]);
      await load();
      onChanged();
    } catch {
      /* ignore */
    }
  }

  async function toggleTimer() {
    const state = taskTimerState(runningTimer, t.id);
    if (timerBusy) return;
    setTimerBusy(true);
    setTimerError(null);
    try {
      setRunningTimer(await changeTaskTimer(projectsApi, state, t));
      announceTimerChanged();
      onChanged();
    } catch (error) {
      await loadTimer();
      setTimerError(projectsMessage(
        error,
        state === "this-task" ? strings.projectsStopFailed : strings.projectsStartFailed,
      ));
    } finally {
      setTimerBusy(false);
    }
  }

  const dueDate = t.dueAt ? t.dueAt.slice(0, 10) : "";
  const done = t.status === "done";
  const labelIds = new Set(data.labels.map((l) => l.id));
  const blockerIds = new Set(data.blockedBy.map((b) => b.id));
  const blockerCandidates = siblings.filter(
    (s) => s.id !== t.id && !blockerIds.has(s.id) && s.state !== "proposed",
  );
  const subDone = data.subtasks.filter((s) => s.done).length;
  const subTotal = data.subtasks.length;
  const timerState = taskTimerState(runningTimer, t.id);
  const prioClass =
    t.priority === "high"
      ? "bg-danger"
      : t.priority === "medium"
        ? "bg-warning"
        : t.priority === "low"
          ? "bg-success"
          : "bg-tertiary";

  return (
    <div className="fixed inset-0 z-modal flex justify-end bg-overlay" onMouseDown={onClose}>
      <div
        className="flex h-full w-full max-w-xl flex-col bg-surface shadow-xl"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex shrink-0 items-center gap-2 border-b border-subtle px-5 py-4">
          <select
            className="h-10 rounded-lg border border-default bg-surface px-3 text-sm font-medium text-primary outline-none focus:border-accent focus:ring-2 focus:ring-accent/15"
            value={t.status}
            onChange={(e) => void changeStatus(e.target.value)}
          >
            {COLUMNS.map((c) => (
              <option key={c.key} value={c.key}>
                {c.label()}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="ml-auto rounded-lg p-2 text-tertiary hover:bg-raised hover:text-danger"
            onClick={async () => {
              await client.deleteTask(t.id);
              onChanged();
              onClose();
            }}
            aria-label={strings.taskDelete}
          >
            <Trash2 size={16} />
          </button>
          <button
            type="button"
            className="rounded-lg p-2 text-tertiary hover:bg-raised hover:text-primary"
            onClick={onClose}
            aria-label={strings.taskClose}
          >
            <X size={18} />
          </button>
        </div>

        <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-6 py-5">
          <div className="flex items-center gap-3">
            <button
              type="button"
              className="inline-flex shrink-0 text-tertiary transition-colors hover:text-success"
              onClick={() => void changeStatus(done ? "todo" : "done")}
              aria-label={done ? strings.taskMarkNotDone : strings.taskMarkDone}
            >
              {done ? <CheckCircle2 size={22} /> : <Circle size={22} />}
            </button>
            <input
              className={`min-w-0 flex-1 border-0 border-b-2 border-transparent bg-transparent px-0.5 py-1 text-xl font-bold text-primary outline-none focus:border-accent ${done ? "text-tertiary line-through" : ""}`}
              defaultValue={t.title}
              onBlur={(e) => {
                if (e.target.value.trim() && e.target.value !== t.title) void save({ title: e.target.value.trim() });
              }}
            />
          </div>

          {t.sourceKind === "email" && t.sourceId && (
            <button
              type="button"
              className="inline-flex w-fit items-center gap-1.5 rounded-lg bg-raised px-3 py-2 text-sm font-medium text-primary hover:bg-accent-tint hover:text-accent"
              onClick={() => {
                onClose();
                navigate(`/mail?open=${encodeURIComponent(t.sourceId!)}`);
              }}
            >
              <Link2 size={14} /> {strings.taskOpenEmail}
            </button>
          )}
          {t.sourceKind === "event" && (
            <span className="inline-flex w-fit items-center gap-1.5 rounded-lg bg-raised px-3 py-2 text-sm font-medium text-secondary">
              <Link2 size={14} /> {strings.taskFromEvent}
            </span>
          )}

          <section className="flex items-center justify-between gap-4 rounded-xl border border-subtle bg-surface px-4 py-3">
            <div className="flex min-w-0 items-center gap-3">
              <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-accent-soft text-accent">
                <Timer size={17} aria-hidden="true" />
              </span>
              <span className="min-w-0">
                <span className="block text-sm font-semibold text-primary">{strings.taskTimeTracking}</span>
                <span className="mt-0.5 block text-xs text-secondary">
                  {timerState === "this-task"
                    ? strings.taskTimerRunningOnTask
                    : timerState === "another-task"
                      ? strings.taskTimerRunningElsewhere
                      : strings.taskTimeTrackingHint}
                </span>
              </span>
            </div>
            <button
              type="button"
              className={`inline-flex min-h-10 shrink-0 items-center justify-center gap-2 rounded-lg px-4 py-2 text-sm font-semibold !no-underline transition-colors hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:cursor-not-allowed disabled:opacity-55 ${
                timerState === "idle"
                  ? "bg-accent text-on-accent hover:bg-accent-hover"
                  : "bg-raised text-primary hover:bg-strong"
              }`}
              onClick={() => void toggleTimer()}
              disabled={timerBusy}
            >
              {timerState === "this-task" ? <Square size={14} aria-hidden="true" /> : <Play size={14} aria-hidden="true" />}
              {timerState === "this-task"
                ? strings.projectsStopTimer
                : timerState === "another-task"
                  ? strings.taskSwitchTimer
                  : strings.projectsStartTimer}
            </button>
          </section>
          {timerError !== null && <p className="text-sm text-danger" role="alert">{timerError}</p>}

          <div className="flex flex-col gap-1 rounded-xl border border-subtle bg-surface p-3">
            <label className="grid min-h-10 grid-cols-[8.5rem_minmax(0,1fr)] items-center gap-3">
              <span className="inline-flex items-center gap-2 text-sm text-secondary [&>svg]:text-tertiary">
                <User size={15} /> {strings.taskAssignee}
              </span>
              <input
                className="rounded-md border border-transparent bg-transparent px-2 py-1.5 text-sm text-primary outline-none hover:bg-raised focus:border-accent focus:bg-surface"
                defaultValue={t.assignee ?? ""}
                placeholder={strings.taskAssigneePlaceholder}
                inputMode="email"
                autoCapitalize="none"
                onBlur={(e) => {
                  if ((e.target.value || "") !== (t.assignee ?? "")) void save({ assignee: e.target.value || null });
                }}
              />
            </label>
            {projectName !== undefined && projectName !== "" && (
              <div className="grid min-h-10 grid-cols-[8.5rem_minmax(0,1fr)] items-center gap-3">
                <span className="inline-flex items-center gap-2 text-sm text-secondary [&>svg]:text-tertiary">
                  <FolderClosed size={15} /> {strings.taskColProject}
                </span>
                <span className="px-2 py-1.5 text-sm text-primary">{projectName}</span>
              </div>
            )}
            <label className="grid min-h-10 grid-cols-[8.5rem_minmax(0,1fr)] items-center gap-3">
              <span className="inline-flex items-center gap-2 text-sm text-secondary [&>svg]:text-tertiary">
                <CalendarDays size={15} /> {strings.taskDue}
              </span>
              <DatePicker
                value={dueDate}
                onChange={(v) => void save({ dueAt: v !== "" ? `${v}T12:00:00Z` : null })}
                placeholder={strings.taskDue}
              />
            </label>
            <label className="grid min-h-10 grid-cols-[8.5rem_minmax(0,1fr)] items-center gap-3">
              <span className="inline-flex items-center gap-2 text-sm text-secondary">
                <span className={`size-2 rounded-full ${prioClass}`} aria-hidden /> {strings.taskPriority}
              </span>
              <select
                className="rounded-md border border-transparent bg-transparent px-2 py-1.5 text-sm text-primary outline-none hover:bg-raised focus:border-accent focus:bg-surface"
                value={t.priority}
                onChange={(e) => void save({ priority: e.target.value as TaskPriority })}
              >
                <option value="none">{strings.taskPrioNone}</option>
                <option value="low">{strings.taskPrioLow}</option>
                <option value="medium">{strings.taskPrioMedium}</option>
                <option value="high">{strings.taskPrioHigh}</option>
              </select>
            </label>
            <div className="grid min-h-10 grid-cols-[8.5rem_minmax(0,1fr)] items-start gap-3 py-1">
              <span className="inline-flex items-center gap-2 pt-1.5 text-sm text-secondary [&>svg]:text-tertiary">
                <Tag size={15} /> {strings.taskLabelsTitle}
              </span>
              <div className="relative flex min-w-0 flex-wrap items-center gap-2" ref={labelWrapRef}>
                {data.labels.map((l) => (
                  <span
                    key={l.id}
                    className="inline-flex min-h-8 items-center gap-1.5 rounded-full bg-raised px-2.5 py-1 text-sm font-medium text-primary"
                  >
                    <span
                      className="size-2 shrink-0 rounded-full"
                      style={{ backgroundColor: l.color ?? "var(--accent)" }}
                      aria-hidden
                    />
                    {l.name}
                    <button
                      type="button"
                      className="-mr-1 inline-flex rounded-full p-1 text-tertiary hover:bg-surface hover:text-danger"
                      onClick={() => void toggleLabel(l.id, true)}
                      aria-label={strings.taskDelete}
                    >
                      <X size={12} />
                    </button>
                  </span>
                ))}
                <span className="relative inline-flex">
                  <button
                    type="button"
                    className="inline-flex min-h-8 items-center gap-1.5 rounded-lg px-2.5 py-1 text-sm font-medium text-secondary hover:bg-accent-tint hover:text-accent"
                    onClick={() => setLabelMenu((v) => !v)}
                  >
                    <Plus size={12} /> {strings.taskAddLabel}
                  </button>
                  {labelMenu && (
                    <div className="absolute right-0 top-[calc(100%+0.375rem)] z-dropdown flex max-h-72 min-w-64 flex-col gap-1 overflow-y-auto rounded-xl border border-default bg-surface p-2 shadow-lg">
                      {allLabels.map((l) => {
                        const on = labelIds.has(l.id);
                        return (
                          <button
                            key={l.id}
                            type="button"
                            className="flex min-h-9 w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-sm text-primary hover:bg-accent-tint hover:text-accent"
                            onClick={() => void toggleLabel(l.id, on)}
                          >
                            <span
                              className="size-2.5 shrink-0 rounded-full"
                              style={{ background: l.color ?? "var(--accent)" }}
                              aria-hidden
                            />
                            {l.name}
                            {on && (
                              <span className="ml-auto inline-flex text-accent">
                                <Check size={14} />
                              </span>
                            )}
                          </button>
                        );
                      })}
                      <div className="mt-1 flex gap-2 border-t border-subtle pt-2">
                        <input
                          className="min-w-0 flex-1 rounded-lg border border-default bg-surface px-3 py-2 text-sm text-primary outline-none placeholder:text-tertiary focus:border-accent focus:ring-2 focus:ring-accent/15"
                          value={newLabel}
                          onChange={(e) => setNewLabel(e.target.value)}
                          placeholder={strings.taskNewLabelPlaceholder}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") void createAndAddLabel();
                          }}
                        />
                        <button
                          type="button"
                          className="shrink-0 rounded-lg bg-accent px-3 py-2 text-sm font-semibold text-on-accent hover:bg-accent-hover disabled:opacity-50"
                          onClick={() => void createAndAddLabel()}
                          disabled={!newLabel.trim()}
                        >
                          {strings.taskCreateLabel}
                        </button>
                      </div>
                    </div>
                  )}
                </span>
              </div>
            </div>
          </div>

          <section className="flex flex-col gap-2 border-t border-subtle pt-4">
            <span className="inline-flex items-center gap-2 text-sm font-semibold text-primary [&>svg]:text-tertiary">
              <Ban size={15} /> {strings.taskBlockedBy}
            </span>
            <div className="relative flex flex-wrap items-center gap-2" ref={blockWrapRef}>
              {data.blockedBy.map((b) => (
                <span
                  key={b.id}
                  className="inline-flex min-h-8 items-center gap-1.5 rounded-full bg-raised px-2.5 py-1 text-sm font-medium text-primary"
                >
                  <span
                    className="size-2 shrink-0 rounded-full"
                    style={{ backgroundColor: statusColor(b.status) }}
                    aria-hidden
                  />
                  {b.title}
                  <button
                    type="button"
                    className="-mr-1 inline-flex rounded-full p-1 text-tertiary hover:bg-surface hover:text-danger"
                    onClick={() => void removeBlocker(b.id)}
                    aria-label={strings.taskDelete}
                  >
                    <X size={12} />
                  </button>
                </span>
              ))}
              <span className="relative inline-flex">
                <button
                  type="button"
                  className="inline-flex min-h-8 items-center gap-1.5 rounded-lg px-2.5 py-1 text-sm font-medium text-secondary hover:bg-accent-tint hover:text-accent"
                  onClick={() => setBlockMenu((v) => !v)}
                >
                  <Plus size={12} /> {strings.taskAddBlocker}
                </button>
                {blockMenu && (
                  <div className="absolute left-0 top-[calc(100%+0.375rem)] z-dropdown flex max-h-72 min-w-64 flex-col gap-1 overflow-y-auto rounded-xl border border-default bg-surface p-2 shadow-lg">
                    {blockerCandidates.length === 0 ? (
                      <div className="px-3 py-4 text-center text-sm text-tertiary">
                        {strings.taskNoBlockerCandidates}
                      </div>
                    ) : (
                      blockerCandidates.map((s) => (
                        <button
                          key={s.id}
                          type="button"
                          className="flex min-h-9 w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-sm text-primary hover:bg-accent-tint hover:text-accent"
                          onClick={() => void addBlocker(s.id)}
                        >
                          <span
                            className="size-2.5 shrink-0 rounded-full"
                            style={{ background: statusColor(s.status) }}
                            aria-hidden
                          />
                          {s.title}
                        </button>
                      ))
                    )}
                  </div>
                )}
              </span>
            </div>
          </section>

          <section className="flex flex-col gap-2 border-t border-subtle pt-4">
            <span className="inline-flex items-center gap-2 text-sm font-semibold text-primary [&>svg]:text-tertiary">
              <AlignLeft size={15} /> {strings.taskDescription}
            </span>
            <textarea
              className="min-h-24 w-full resize-y rounded-xl border border-default bg-surface px-3 py-2.5 text-sm text-primary outline-none placeholder:text-tertiary focus:border-accent focus:ring-2 focus:ring-accent/15"
              rows={3}
              defaultValue={t.description ?? ""}
              placeholder={strings.taskDescriptionPlaceholder}
              onBlur={(e) => {
                if ((e.target.value || "") !== (t.description ?? "")) void save({ description: e.target.value || null });
              }}
            />
          </section>

          <section className="flex flex-col gap-2 border-t border-subtle pt-4">
            <div className="flex items-center justify-between gap-3">
              <span className="text-sm font-semibold text-primary">{strings.taskSubtasks}</span>
              {subTotal > 0 && (
                <span className="inline-flex items-center gap-2">
                  <span className="text-xs tabular-nums text-tertiary">
                    {subDone}/{subTotal}
                  </span>
                  <span className="h-1.5 w-20 overflow-hidden rounded-full bg-raised">
                    <span
                      className="block h-full rounded-full bg-success transition-[width] duration-200"
                      style={{ width: `${subTotal === 0 ? 0 : (subDone / subTotal) * 100}%` }}
                    />
                  </span>
                </span>
              )}
            </div>
            <div className="flex flex-col gap-1">
              {data.subtasks.map((s) => (
                <div key={s.id} className="group flex min-h-9 items-center gap-2 rounded-lg px-2 py-1.5 hover:bg-raised">
                  <input
                    type="checkbox"
                    className="size-4 accent-accent"
                    checked={s.done}
                    onChange={async (e) => {
                      await client.setSubtask(t.id, s.id, e.target.checked);
                      await load();
                    }}
                  />
                  <span className={`min-w-0 flex-1 text-sm text-primary ${s.done ? "text-tertiary line-through" : ""}`}>
                    {s.title}
                  </span>
                  <button
                    type="button"
                    className="inline-flex shrink-0 rounded-md p-1.5 text-tertiary opacity-0 hover:bg-surface hover:text-danger group-hover:opacity-100 focus:opacity-100"
                    aria-label={strings.taskDelete}
                    onClick={async () => {
                      await client.deleteSubtask(t.id, s.id);
                      await load();
                    }}
                  >
                    <Trash2 size={13} />
                  </button>
                </div>
              ))}
            </div>
            <input
              className="w-full rounded-xl border border-default bg-surface px-3 py-2.5 text-sm text-primary outline-none placeholder:text-tertiary focus:border-accent focus:ring-2 focus:ring-accent/15"
              value={newSub}
              placeholder={strings.taskAddSubtask}
              onChange={(e) => setNewSub(e.target.value)}
              onKeyDown={async (e) => {
                if (e.key === "Enter" && newSub.trim()) {
                  await client.addSubtask(t.id, newSub.trim());
                  setNewSub("");
                  await load();
                  onChanged();
                }
              }}
            />
          </section>

          <section className="flex flex-col gap-2 border-t border-subtle pt-4">
            <span className="inline-flex items-center gap-2 text-sm font-semibold text-primary [&>svg]:text-tertiary">
              <Paperclip size={15} /> {strings.taskAttachments}
            </span>
            {data.attachments.map((f) => (
              <div key={f.id} className="group flex items-center gap-3 rounded-xl border border-subtle p-2.5">
                <span className="inline-flex size-9 shrink-0 items-center justify-center rounded-lg bg-accent-tint text-accent">
                  <Paperclip size={14} />
                </span>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-sm font-medium text-primary">{f.filename}</span>
                  <span className="text-xs text-tertiary">{fileSize(f.size)}</span>
                </span>
                <button
                  type="button"
                  className="inline-flex shrink-0 rounded-lg p-2 text-tertiary hover:bg-raised hover:text-accent"
                  onClick={() => void downloadAttachment(f.id, f.filename)}
                  aria-label={strings.taskDownload}
                >
                  <Download size={15} />
                </button>
                <button
                  type="button"
                  className="inline-flex shrink-0 rounded-lg p-2 text-tertiary hover:bg-raised hover:text-danger"
                  onClick={async () => {
                    await client.deleteTaskAttachment(t.id, f.id);
                    await load();
                  }}
                  aria-label={strings.taskDelete}
                >
                  <Trash2 size={15} />
                </button>
              </div>
            ))}
            <input
              ref={fileRef}
              type="file"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (file !== undefined) void uploadAttachment(file);
                e.target.value = "";
              }}
            />
            <button
              type="button"
              className="inline-flex min-h-10 w-fit items-center gap-2 rounded-lg bg-raised px-3 py-2 text-sm font-medium text-primary hover:bg-accent-tint hover:text-accent disabled:cursor-wait disabled:opacity-60"
              onClick={() => fileRef.current?.click()}
              disabled={uploading}
            >
              <Paperclip size={15} /> {uploading ? strings.taskUploading : strings.taskAddAttachment}
            </button>
          </section>

          <section className="flex flex-col gap-2 border-t border-subtle pt-4">
            <span className="text-sm font-semibold text-primary">{strings.taskComments}</span>
            {data.comments.map((c) => (
              <div key={c.id} className="flex flex-col gap-1 border-b border-subtle py-2.5 last:border-b-0">
                <div className="flex items-center gap-2 text-xs text-secondary">
                  <strong>{c.author}</strong>
                  <span>{new Date(c.createdAt).toLocaleString()}</span>
                </div>
                <div className="whitespace-pre-wrap text-sm text-primary">{c.body}</div>
              </div>
            ))}
            <textarea
              className="min-h-20 w-full resize-y rounded-xl border border-default bg-surface px-3 py-2.5 text-sm text-primary outline-none placeholder:text-tertiary focus:border-accent focus:ring-2 focus:ring-accent/15"
              rows={2}
              value={newComment}
              placeholder={strings.taskAddComment}
              onChange={(e) => setNewComment(e.target.value)}
              onKeyDown={async (e) => {
                if (e.key === "Enter" && !e.shiftKey && newComment.trim()) {
                  e.preventDefault();
                  await client.addTaskComment(t.id, newComment.trim());
                  setNewComment("");
                  await load();
                }
              }}
            />
          </section>

          <section className="border-t border-subtle pt-4">
            <div className="flex items-center gap-3">
              <span className="text-sm font-semibold text-primary">{strings.taskFollowers}</span>
              <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1">
                {data.followers.map((email, i) => (
                  <Avatar key={i} email={email} />
                ))}
              </div>
              <button
                type="button"
                className="shrink-0 rounded-lg bg-raised px-3 py-2 text-sm font-medium text-primary hover:bg-accent-tint hover:text-accent"
                onClick={() => void toggleFollow()}
              >
                {data.following ? strings.taskLeave : strings.taskFollow}
              </button>
            </div>
          </section>

          {data.activity.length > 0 && (
            <section className="flex flex-col gap-1.5 border-t border-subtle pt-4">
              <span className="mb-1 text-sm font-semibold text-primary">{strings.taskActivity}</span>
              {data.activity.slice(0, 12).map((a, i) => (
                <div key={i} className="text-xs leading-relaxed text-secondary">
                  {a.actor} · {strings.taskActivityKind(a.kind)} · {new Date(a.createdAt).toLocaleString()}
                </div>
              ))}
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
