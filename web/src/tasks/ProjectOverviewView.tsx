import { CalendarDays, CheckCircle2, CircleAlert, Clock3, Flag, ListTodo, ReceiptText } from "lucide-react";
import type { ReactNode } from "react";

import { strings } from "../i18n";
import type { Task } from "../jmap";
import { durationLabel } from "../projects/format";
import type { Project, ProjectPlan } from "../projects/types";

interface Props {
  project: Project;
  plan: ProjectPlan;
  tasks: Task[];
  onAddTask: () => void;
  onOpenTask: (id: string) => void;
  onOpenTasks: () => void;
  onOpenTimeline: () => void;
}

export function ProjectOverviewView({
  project,
  plan,
  tasks,
  onAddTask,
  onOpenTask,
  onOpenTasks,
  onOpenTimeline,
}: Props) {
  const done = tasks.filter((task) => task.status === "done").length;
  const open = tasks.length - done;
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const overdue = tasks.filter((task) =>
    task.status !== "done" && task.dueAt !== null && new Date(task.dueAt) < today
  ).length;
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
    </div>
  );
}

function friendlyDate(value: string): string {
  const date = new Date(`${value.slice(0, 10)}T12:00:00`);
  return new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", year: "numeric" }).format(date);
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
