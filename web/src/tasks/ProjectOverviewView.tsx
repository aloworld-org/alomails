import { ArrowRight, BarChart3, CalendarDays, CheckCircle2, CircleAlert, Clock3, Flag, ListTodo, MessageSquareText, PencilLine, ReceiptText, Send } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";

import { strings } from "../i18n";
import type { Task, TaskDepEdgeDto } from "../jmap";
import { InvoiceHandoffDialog } from "../projects/InvoiceHandoffDialog";
import { durationLabel } from "../projects/format";
import { projectsMessage, useProjectsApi } from "../projects/api";
import type { Project, ProjectPlan, ProjectUpdate, ProjectUpdateState } from "../projects/types";

interface Props {
  project: Project;
  plan: ProjectPlan;
  tasks: Task[];
  edges: TaskDepEdgeDto[];
  onAddTask: () => void;
  onOpenTask: (id: string) => void;
  onOpenTasks: () => void;
  onOpenTimesheet: () => void;
  onOpenTimeline: () => void;
  onOpenReport: () => void;
  onEditProject: () => void;
  onOpenInvoice: (id: string) => void;
}

/** A project may offer the billing handoff only when the server has confirmed
 * that approved, billable, uninvoiced time exists. */
export function canCreateProjectInvoice(project: Project): boolean {
  return project.client !== null && project.hours.approvedUnbilledMinutes > 0;
}

export type ProjectNextStep = "tasks" | "time" | "approval" | "invoice" | "continue";

/** One honest next action for the engagement. The overview is the hand-off
 * between project management and billing, so it must not make people infer
 * whether their hours still need recording, approval, or invoicing. */
export function projectNextStep(project: Project, taskCount: number): ProjectNextStep {
  if (taskCount === 0) return "tasks";
  if (project.hours.minutes === 0) return "time";
  if (canCreateProjectInvoice(project)) return "invoice";
  const awaitingApproval = Math.max(
    0,
    project.hours.billableMinutes
      - project.hours.approvedUnbilledMinutes
      - project.hours.billedMinutes,
  );
  if (project.client !== null && awaitingApproval > 0) return "approval";
  return "continue";
}

export function ProjectOverviewView({
  project,
  plan,
  tasks,
  edges,
  onAddTask,
  onOpenTask,
  onOpenTasks,
  onOpenTimesheet,
  onOpenTimeline,
  onOpenReport,
  onEditProject,
  onOpenInvoice,
}: Props) {
  const api = useProjectsApi();
  const [updates, setUpdates] = useState<ProjectUpdate[]>([]);
  const [updateState, setUpdateState] = useState<ProjectUpdateState>("on_track");
  const [updateBody, setUpdateBody] = useState("");
  const [updatesLoading, setUpdatesLoading] = useState(true);
  const [updateSaving, setUpdateSaving] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [invoiceOpen, setInvoiceOpen] = useState(false);
  const nextStep = projectNextStep(project, tasks.length);
  const workflowLabels = project.client === null
    ? [
        strings.projectsWorkflowTasks,
        strings.projectsWorkflowTime,
        strings.projectsWorkflowApproval,
      ]
    : [
        strings.projectsWorkflowTasks,
        strings.projectsWorkflowTime,
        strings.projectsWorkflowApproval,
        strings.projectsWorkflowInvoice,
      ];

  useEffect(() => {
    let active = true;
    setUpdatesLoading(true);
    setUpdateError(null);
    void api.projectUpdates(project.id).then((loaded) => {
      if (active) setUpdates(loaded);
    }).catch((error: unknown) => {
      if (active) setUpdateError(projectsMessage(error, strings.projectsUpdatesLoadFailed));
    }).finally(() => {
      if (active) setUpdatesLoading(false);
    });
    return () => { active = false; };
  }, [api, project.id]);

  async function publishUpdate() {
    const body = updateBody.trim();
    if (body === "" || updateSaving) return;
    setUpdateSaving(true);
    setUpdateError(null);
    try {
      const created = await api.createProjectUpdate(project.id, updateState, body);
      setUpdates((current) => [created, ...current]);
      setUpdateBody("");
    } catch (error) {
      setUpdateError(projectsMessage(error, strings.projectsUpdateSaveFailed));
    } finally {
      setUpdateSaving(false);
    }
  }
  const done = tasks.filter((task) => task.status === "done").length;
  const open = tasks.length - done;
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const overdue = tasks.filter((task) =>
    task.status !== "done" && task.dueAt !== null && new Date(task.dueAt) < today
  ).length;
  const taskById = new Map(tasks.map((task) => [task.id, task]));
  const blockedIds = new Set(
    edges
      .filter((edge) => taskById.get(edge.blockedBy)?.status !== "done")
      .map((edge) => edge.blocked),
  );
  const blocked = tasks.filter((task) => task.status !== "done" && blockedIds.has(task.id)).length;
  const targetMissed = project.targetOn !== null && new Date(`${project.targetOn}T23:59:59`) < today && open > 0;
  const atRisk = overdue > 0 || blocked > 0 || targetMissed;
  const workload = [...tasks
    .filter((task) => task.status !== "done")
    .reduce((counts, task) => {
      const key = task.assignee ?? strings.taskUnassigned;
      counts.set(key, (counts.get(key) ?? 0) + 1);
      return counts;
    }, new Map<string, number>())]
    .sort((a, b) => b[1] - a[1]);
  const maxWorkload = Math.max(1, ...workload.map(([, count]) => count));
  const completion = tasks.length === 0 ? 0 : Math.round((done / tasks.length) * 100);
  const nextMilestone = [...plan.milestones]
    .filter((milestone) => !milestone.done)
    .sort((a, b) => a.dueOn.localeCompare(b.dueOn))[0];
  const billedShare = project.hours.billableMinutes === 0
    ? 0
    : Math.min(100, Math.round((project.hours.billedMinutes / project.hours.billableMinutes) * 100));
  const upcoming = tasks
    .filter((task) => task.status !== "done" && task.dueAt !== null)
    .sort((a, b) => (a.dueAt ?? "").localeCompare(b.dueAt ?? ""))
    .slice(0, 4);

  return (
    <div className="mx-auto flex w-full max-w-[88rem] flex-col gap-5 px-6 py-6 lg:px-8">
      <section className="flex flex-wrap items-start justify-between gap-5 rounded-2xl border border-subtle bg-surface p-5 shadow-sm">
        <div className="min-w-0 max-w-3xl">
          <div className="flex flex-wrap items-center gap-2">
            <span className="inline-flex rounded-full bg-accent-soft px-2.5 py-1 text-xs font-semibold text-accent">
              {{
                planned: strings.projectsStatusPlanned,
                active: strings.projectsStatusActive,
                on_hold: strings.projectsStatusOnHold,
                completed: strings.projectsStatusCompleted,
                cancelled: strings.projectsStatusCancelled,
              }[project.status]}
            </span>
            {project.targetOn !== null && (
              <span className="inline-flex items-center gap-1.5 text-sm text-secondary">
                <CalendarDays size={15} aria-hidden="true" />
                {strings.projectsTargetOn}: {friendlyDate(project.targetOn)}
              </span>
            )}
          </div>
          <p className="mt-3 text-sm leading-6 text-secondary">
            {project.description ?? strings.projectsDetailsSubtitle}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            className="inline-flex min-h-10 shrink-0 items-center gap-2 rounded-lg bg-raised px-4 py-2 text-sm font-medium text-primary !no-underline transition-colors hover:bg-default hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            onClick={onOpenTimesheet}
          >
            <Clock3 size={16} aria-hidden="true" /> {strings.projectsTabWeek}
          </button>
          <button
            type="button"
            className="inline-flex min-h-10 shrink-0 items-center gap-2 rounded-lg bg-raised px-4 py-2 text-sm font-medium text-primary !no-underline transition-colors hover:bg-default hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            onClick={onOpenReport}
          >
            <BarChart3 size={16} aria-hidden="true" /> {strings.projectsTabReports}
          </button>
          {project.kind === "team" && (
            <button
              type="button"
              className="inline-flex min-h-10 shrink-0 items-center gap-2 rounded-lg bg-raised px-4 py-2 text-sm font-medium text-primary !no-underline transition-colors hover:bg-default hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
              onClick={onEditProject}
            >
              <PencilLine size={16} aria-hidden="true" /> {strings.projectsEdit}
            </button>
          )}
        </div>
      </section>
      <section className="rounded-2xl border border-subtle bg-surface p-5 shadow-sm" aria-labelledby="project-next-step-title">
        <div className="flex flex-wrap items-center justify-between gap-5">
          <div className="min-w-0">
            <p className="text-xs font-semibold uppercase tracking-wide text-accent">{strings.projectsWorkflowEyebrow}</p>
            <h2 id="project-next-step-title" className="mt-1 text-base font-semibold text-primary">
              {{
                tasks: strings.projectsWorkflowTasksTitle,
                time: strings.projectsWorkflowTimeTitle,
                approval: strings.projectsWorkflowApprovalTitle,
                invoice: strings.projectsWorkflowInvoiceTitle,
                continue: strings.projectsWorkflowContinueTitle,
              }[nextStep]}
            </h2>
            <p className="mt-1 text-sm leading-6 text-secondary">
              {{
                tasks: strings.projectsWorkflowTasksBody,
                time: strings.projectsWorkflowTimeBody,
                approval: strings.projectsWorkflowApprovalBody,
                invoice: strings.projectsReadyToInvoiceBody(durationLabel(project.hours.approvedUnbilledMinutes)),
                continue: strings.projectsWorkflowContinueBody,
              }[nextStep]}
            </p>
          </div>
          <button
            type="button"
            className="inline-flex min-h-11 shrink-0 items-center justify-center gap-2 rounded-lg bg-accent px-5 py-2.5 text-sm font-semibold text-on-accent !no-underline transition-colors hover:bg-accent-hover hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2"
            onClick={() => {
              if (nextStep === "tasks") onAddTask();
              else if (nextStep === "invoice") setInvoiceOpen(true);
              else onOpenTimesheet();
            }}
          >
            {{
              tasks: strings.taskCreateFirst,
              time: strings.projectsAddTime,
              approval: strings.projectsReviewTimesheet,
              invoice: strings.projectsCreateInvoice,
              continue: strings.projectsAddTime,
            }[nextStep]}
            <ArrowRight size={16} aria-hidden="true" />
          </button>
        </div>
        <ol
          className={`mt-5 grid gap-2 border-t border-subtle pt-4 ${project.client === null ? "sm:grid-cols-3" : "sm:grid-cols-4"}`}
          aria-label={strings.projectsWorkflowLabel}
        >
          {workflowLabels.map((label, index) => {
            const activeIndex = { tasks: 0, time: 1, approval: 2, invoice: 3, continue: project.client === null ? 2 : 3 }[nextStep];
            const reached = index <= activeIndex;
            return (
              <li key={label} className={`flex min-h-10 items-center gap-2 rounded-lg px-3 py-2 text-sm ${reached ? "bg-accent-soft font-medium text-accent" : "bg-raised text-secondary"}`}>
                <span className={`inline-flex size-5 shrink-0 items-center justify-center rounded-full text-xs font-semibold ${reached ? "bg-accent text-on-accent" : "bg-surface text-secondary"}`}>
                  {index + 1}
                </span>
                <span className="truncate">{label}</span>
              </li>
            );
          })}
        </ol>
      </section>

      <section className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4" aria-label={strings.taskOverview}>
        <Metric
          icon={<ListTodo size={18} />}
          label={strings.taskOvProgress}
          value={`${completion}%`}
          detail={strings.taskOvTasksTotal(tasks.length)}
        />
        <Metric
          icon={<CircleAlert size={18} />}
          label={strings.taskColInProgress}
          value={String(open)}
          detail={overdue > 0 ? `${overdue} ${strings.homeTaskOverdue.toLowerCase()}` : strings.taskUnscheduled}
          warning={overdue > 0}
        />
        <Metric
          icon={<Clock3 size={18} />}
          label={strings.projectsHoursLogged}
          value={durationLabel(project.hours.minutes)}
          detail={strings.projectsOfWhichBillable(durationLabel(project.hours.billableMinutes))}
        />
        <Metric
          icon={<ReceiptText size={18} />}
          label={strings.projectsBillableHours}
          value={`${billedShare}%`}
          detail={`${durationLabel(project.hours.billedMinutes)} ${strings.projectsBilledEntry}`}
        />
      </section>

      <section className="grid gap-5 xl:grid-cols-[minmax(0,1.35fr)_minmax(20rem,.65fr)]">
        <div className="rounded-2xl border border-subtle bg-surface p-5 shadow-sm">
          <div className="flex flex-wrap items-start justify-between gap-3 border-b border-subtle pb-4">
            <div>
              <h2 className="text-base font-semibold text-primary">{strings.taskOvUpcoming}</h2>
              <p className="mt-1 text-sm text-secondary">{strings.projectsWorkspaceTasks}</p>
            </div>
            <button
              type="button"
              className="inline-flex min-h-10 items-center rounded-lg bg-raised px-4 py-2 text-sm font-medium text-primary !no-underline transition-colors hover:bg-[var(--border-subtle)] hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
              onClick={onOpenTasks}
            >
              {strings.taskOvViewAll}
            </button>
          </div>

          {upcoming.length === 0 ? (
            <div className="flex min-h-52 flex-col items-center justify-center px-6 py-10 text-center">
              <span className="mb-3 inline-flex size-12 items-center justify-center rounded-xl bg-[var(--accent-soft)] text-accent">
                <CheckCircle2 size={22} />
              </span>
              <p className="font-semibold text-primary">{strings.taskEmptyTitle}</p>
              <p className="mt-1 max-w-md text-sm text-secondary">{strings.taskEmptyBody}</p>
              <button
                type="button"
                className="mt-5 inline-flex min-h-11 items-center rounded-lg bg-accent px-5 py-2.5 text-sm font-semibold text-on-accent !no-underline transition-colors hover:bg-accent-hover hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2"
                onClick={onAddTask}
              >
                {strings.taskCreateFirst}
              </button>
            </div>
          ) : (
            <ul className="divide-y divide-subtle">
              {upcoming.map((task) => (
                <li key={task.id}>
                  <button
                    type="button"
                    className="flex min-h-16 w-full items-center gap-3 rounded-lg px-2 py-3 text-left !no-underline transition-colors hover:bg-raised hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                    onClick={() => onOpenTask(task.id)}
                  >
                    <span className="size-2 shrink-0 rounded-full bg-accent" />
                    <span className="min-w-0 flex-1 truncate font-medium text-primary">{task.title}</span>
                    <span className="shrink-0 text-sm text-secondary">{task.dueAt === null ? "" : friendlyDate(task.dueAt)}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="flex flex-col gap-5">
          <div className="rounded-2xl border border-subtle bg-surface p-5 shadow-sm">
            <div className="flex items-center justify-between gap-3">
              <h2 className="text-base font-semibold text-primary">{strings.projectsHealth}</h2>
              <span className={`inline-flex rounded-full px-2.5 py-1 text-xs font-semibold ${atRisk ? "bg-danger-tint text-danger" : "bg-success-tint text-success"}`}>
                {atRisk ? strings.projectsHealthAtRisk : strings.projectsHealthOnTrack}
              </span>
            </div>
            <div className="mt-4 grid grid-cols-2 gap-3">
              <HealthDatum label={strings.projectsBlockedTasks(blocked)} value={blocked} warning={blocked > 0} />
              <HealthDatum label={strings.projectsOverdueTasks(overdue)} value={overdue} warning={overdue > 0} />
            </div>
            {project.targetOn === null && (
              <p className="mt-3 text-sm text-secondary">{strings.projectsHealthNeedsTarget}</p>
            )}
          </div>

          <div className="rounded-2xl border border-subtle bg-surface p-5 shadow-sm">
            <h2 className="text-base font-semibold text-primary">{strings.projectsWorkload}</h2>
            {workload.length === 0 ? (
              <p className="mt-4 text-sm text-secondary">{strings.projectsWorkloadEmpty}</p>
            ) : (
              <ul className="mt-4 space-y-4">
                {workload.slice(0, 5).map(([assignee, count]) => (
                  <li key={assignee}>
                    <div className="flex items-center justify-between gap-4 text-sm">
                      <span className="min-w-0 truncate font-medium text-primary">{assignee}</span>
                      <span className="shrink-0 text-secondary">{strings.projectsOpenTasks(count)}</span>
                    </div>
                    <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-raised">
                      <div className="h-full rounded-full bg-accent" style={{ width: `${Math.round((count / maxWorkload) * 100)}%` }} />
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </div>

          <div className="rounded-2xl border border-subtle bg-surface p-5 shadow-sm">
            <div className="flex items-center justify-between gap-3">
              <h2 className="text-base font-semibold text-primary">{strings.agentStatusMilestones}</h2>
              <button
                type="button"
                className="inline-flex min-h-9 items-center rounded-lg px-3 py-1.5 text-sm font-medium text-secondary !no-underline hover:bg-raised hover:text-primary hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                onClick={onOpenTimeline}
              >
                {strings.projectsTabPlan}
              </button>
            </div>
            {nextMilestone === undefined ? (
              <div className="mt-4 flex items-center gap-3 rounded-xl bg-raised p-4 text-sm text-secondary">
                <Flag size={18} /> {strings.agentStatusNoMilestones}
              </div>
            ) : (
              <div className="mt-4 rounded-xl bg-raised p-4">
                <div className="flex items-start gap-3">
                  <span className="mt-0.5 inline-flex size-9 shrink-0 items-center justify-center rounded-lg bg-[var(--accent-soft)] text-accent"><Flag size={17} /></span>
                  <div className="min-w-0">
                    <p className="truncate font-semibold text-primary">{nextMilestone.name}</p>
                    <p className="mt-1 flex items-center gap-1.5 text-sm text-secondary"><CalendarDays size={14} /> {friendlyDate(nextMilestone.dueOn)}</p>
                  </div>
                </div>
                <div className="mt-4 h-1.5 overflow-hidden rounded-full bg-surface">
                  <div className="h-full rounded-full bg-accent" style={{ width: `${nextMilestone.taskCount === 0 ? 0 : Math.round((nextMilestone.taskDoneCount / nextMilestone.taskCount) * 100)}%` }} />
                </div>
                <p className="mt-2 text-xs text-secondary">{strings.projectsMilestoneTasksClosed(nextMilestone.taskDoneCount, nextMilestone.taskCount)}</p>
              </div>
            )}
          </div>

          <div className="rounded-2xl border border-subtle bg-surface p-5 shadow-sm">
            <h2 className="text-base font-semibold text-primary">{strings.projectsBudget}</h2>
            <div className="mt-4 flex items-end justify-between gap-3">
              <div>
                <p className="text-2xl font-semibold tabular-nums text-primary">
                  {project.hours.budgetConsumptionBp === null ? "—" : `${Math.round(project.hours.budgetConsumptionBp / 100)}%`}
                </p>
                <p className="mt-1 text-sm text-secondary">{project.client === null ? strings.projectsInternal : strings.projectsBudgetUsed}</p>
              </div>
              <p className="text-right text-sm text-secondary">{project.hours.lastWorkedOn === null ? strings.projectsNeverWorked : friendlyDate(project.hours.lastWorkedOn)}</p>
            </div>
          </div>

        </div>
      </section>

      <section className="rounded-2xl border border-subtle bg-surface p-5 shadow-sm">
        <div className="flex items-start gap-3 border-b border-subtle pb-4">
          <span className="inline-flex size-10 shrink-0 items-center justify-center rounded-xl bg-accent-soft text-accent">
            <MessageSquareText size={19} aria-hidden="true" />
          </span>
          <div>
            <h2 className="text-base font-semibold text-primary">{strings.projectsUpdates}</h2>
            <p className="mt-1 text-sm text-secondary">{strings.projectsUpdatesSubtitle}</p>
          </div>
        </div>
        <div className="mt-4 rounded-xl bg-raised p-4">
          <div className="flex flex-wrap gap-2" role="group" aria-label={strings.projectsUpdateHealth}>
            {(["on_track", "at_risk", "off_track", "complete"] as const).map((state) => (
              <button key={state} type="button"
                className={`min-h-9 rounded-full px-3 py-1.5 text-xs font-semibold !no-underline transition-colors hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${updateState === state ? "bg-accent text-on-accent" : "bg-surface text-secondary hover:text-primary"}`}
                aria-pressed={updateState === state} onClick={() => setUpdateState(state)}>
                {updateStateLabel(state)}
              </button>
            ))}
          </div>
          <textarea className="mt-3 min-h-24 w-full resize-y rounded-xl border border-subtle bg-surface px-4 py-3 text-sm leading-6 text-primary placeholder:text-secondary focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/20"
            maxLength={4000} placeholder={strings.projectsUpdatePlaceholder} value={updateBody}
            onChange={(event) => setUpdateBody(event.target.value)} />
          <div className="mt-3 flex items-center justify-between gap-4">
            <p className="text-xs text-secondary">{strings.projectsUpdateHint}</p>
            <button type="button"
              className="inline-flex min-h-10 shrink-0 items-center gap-2 rounded-lg bg-accent px-4 py-2 text-sm font-semibold text-on-accent !no-underline transition-colors hover:bg-accent-hover hover:!no-underline disabled:cursor-not-allowed disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2"
              disabled={updateBody.trim() === "" || updateSaving} onClick={() => void publishUpdate()}>
              <Send size={15} aria-hidden="true" /> {updateSaving ? strings.billingSaving : strings.projectsPublishUpdate}
            </button>
          </div>
          {updateError !== null && <p className="mt-3 text-sm text-danger" role="alert">{updateError}</p>}
        </div>
        {updatesLoading ? (
          <p className="py-6 text-sm text-secondary">{strings.chatLoading}</p>
        ) : updates.length === 0 ? (
          <div className="py-8 text-center">
            <p className="font-medium text-primary">{strings.projectsUpdatesEmpty}</p>
            <p className="mt-1 text-sm text-secondary">{strings.projectsUpdatesEmptyBody}</p>
          </div>
        ) : (
          <ol className="mt-2 divide-y divide-subtle">
            {updates.slice(0, 8).map((update) => (
              <li key={update.id} className="py-4 first:pt-3">
                <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                  <span className="font-semibold text-primary">{update.authorEmail || strings.projectsSomeone}</span>
                  <span className="rounded-full bg-raised px-2 py-0.5 text-xs font-medium text-secondary">{updateStateLabel(update.state)}</span>
                  <time className="text-xs text-secondary" dateTime={update.createdAt}>{friendlyDateTime(update.createdAt)}</time>
                </div>
                <p className="mt-2 whitespace-pre-wrap text-sm leading-6 text-primary">{update.body}</p>
              </li>
            ))}
          </ol>
        )}
      </section>
      {invoiceOpen && <InvoiceHandoffDialog project={project} onClose={() => setInvoiceOpen(false)} onCreated={onOpenInvoice} />}
    </div>
  );
}

function HealthDatum({ label, value, warning }: { label: string; value: number; warning: boolean }) {
  return (
    <div className="rounded-xl bg-raised p-3">
      <p className={`text-xl font-semibold tabular-nums ${warning ? "text-danger" : "text-primary"}`}>{value}</p>
      <p className="mt-1 text-xs text-secondary">{label}</p>
    </div>
  );
}

function friendlyDate(value: string): string {
  const date = new Date(`${value.slice(0, 10)}T12:00:00`);
  return new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", year: "numeric" }).format(date);
}

function friendlyDateTime(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    month: "short", day: "numeric", hour: "numeric", minute: "2-digit",
  }).format(new Date(value));
}

function updateStateLabel(state: ProjectUpdateState): string {
  return {
    on_track: strings.projectsHealthOnTrack,
    at_risk: strings.projectsHealthAtRisk,
    off_track: strings.projectsUpdateOffTrack,
    complete: strings.projectsStatusCompleted,
  }[state];
}

function Metric({ icon, label, value, detail, warning = false }: { icon: ReactNode; label: string; value: string; detail: string; warning?: boolean }) {
  return (
    <div className="rounded-2xl border border-subtle bg-surface p-5 shadow-sm">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <p className="text-sm font-medium text-secondary">{label}</p>
          <p className="mt-2 text-2xl font-semibold tabular-nums text-primary">{value}</p>
          <p className={`mt-1 truncate text-xs ${warning ? "text-accent" : "text-secondary"}`}>{detail}</p>
        </div>
        <span className="inline-flex size-10 shrink-0 items-center justify-center rounded-xl bg-[var(--accent-soft)] text-accent">{icon}</span>
      </div>
    </div>
  );
}
