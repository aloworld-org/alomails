// The engagement list: every project this workspace can see, as client work —
// who it is for, at what rate, what it has cost in hours, and how much of its
// budget that is.
//
// It is the same list Tasks shows, in the same order, seen through a second
// lens: a project here IS the board over there (`docs/design/projects.md`, "One
// project list, extended"). Internal projects are listed too, greyed rather than
// hidden — "make this client work" is the action this screen exists to offer,
// and a project you cannot see is a project you cannot offer it for.
//
// Every figure is the server's. The hours are the project's aggregate — nobody
// is named, here or in the API — and the budget bar is drawn from basis points
// the server computed, so two people looking at one engagement see one bar.
import { Briefcase, CopyPlus, FolderKanban, PencilLine, Play, Plus, Square, Star } from "lucide-react";
import { useEffect, useState } from "react";

import { Button, IconButton, Spinner } from "../ds";
import { strings } from "../i18n";
import { amountLabel, dayLabel, durationLabel, elapsedMinutes, percentLabel, rateLabel } from "./format";
import { EmptyState } from "./parts";
import type { Project, RunningTimer } from "./types";

const TIMER_TICK_MS = 15_000;

function ProjectBudget({ consumptionBp }: { consumptionBp: number | null }) {
  if (consumptionBp === null) return <span className="text-tertiary">—</span>;

  const percentage = Math.round(consumptionBp / 100);
  const width = Math.min(100, Math.max(0, percentage));
  const overBudget = consumptionBp > 10_000;

  return (
    <div className="flex items-center gap-2.5">
      <span
        className="h-1.5 min-w-20 flex-1 overflow-hidden rounded-full bg-raised"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percentage}
        aria-label={strings.projectsBudgetUsed}
      >
        <span
          className={`block h-full rounded-full ${overBudget ? "bg-danger" : "bg-accent"}`}
          style={{ width: `${width}%` }}
        />
      </span>
      <span
        className={`shrink-0 text-xs font-semibold tabular-nums ${overBudget ? "text-danger" : "text-secondary"}`}
      >
        {percentLabel(consumptionBp)}
      </span>
    </div>
  );
}

export function ProjectsView({
  projects,
  loading,
  runningTimer,
  customerName,
  isTemplate,
  onEditClient,
  onEditProject,
  onStartTimer,
  onStopTimer,
  onToggleTemplate,
  onOpenTasks,
  onNewProject,
  onNewFromTemplate,
}: {
  projects: Project[];
  loading: boolean;
  runningTimer: RunningTimer | null;
  /** The customer's own name for an id, or `null` while the list is loading or
   *  when the customer is one this reader cannot see. Resolved by the caller,
   *  which owns the billing read. */
  customerName: (customerId: string) => string | null;
  /** Whether this board is already marked reusable. */
  isTemplate: (projectId: string) => boolean;
  onEditClient: (project: Project) => void;
  onEditProject: (project: Project) => void;
  onStartTimer: (project: Project) => void;
  onStopTimer: () => void;
  /** Marks the board reusable, or takes the mark off — the same control, because
   *  a board either is a template or is not. */
  onToggleTemplate: (project: Project) => void;
  /** Opens the task workspace already scoped to this project. */
  onOpenTasks: (project: Project) => void;
  onNewProject: () => void;
  onNewFromTemplate: () => void;
}) {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (runningTimer === null) return;
    setNow(Date.now());
    const tick = window.setInterval(() => setNow(Date.now()), TIMER_TICK_MS);
    return () => window.clearInterval(tick);
  }, [runningTimer]);

  const runningMinutes = runningTimer === null ? null : elapsedMinutes(runningTimer.startedAt, now);
  const runningElapsed = runningMinutes === null
    ? null
    : runningMinutes === 0
      ? strings.projectsMinutesShort(0)
      : durationLabel(runningMinutes);

  if (projects.length === 0) {
    return loading ? (
      <div className="flex min-h-0 flex-1 items-center justify-center p-8">
        <Spinner size={20} />
      </div>
    ) : (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-6">
        <EmptyState
          Icon={Briefcase}
          title={strings.projectsEmptyTitle}
          body={strings.projectsEmptyBody}
          cta={strings.projectsNew}
          onCta={onNewProject}
        />
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-5 pb-8 pt-4 max-sm:px-3">
      <section className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm">
        <div className="flex items-center justify-between gap-4 border-b border-subtle px-5 py-4 max-sm:items-start">
          <div className="flex min-w-0 items-center gap-3">
            <span className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-[var(--accent-soft)] text-accent">
              <FolderKanban className="size-5" aria-hidden="true" />
            </span>
            <div className="min-w-0">
              <h2 className="m-0 text-base font-semibold text-primary">
                {strings.projectsTabList}
              </h2>
              <p className="m-0 mt-0.5 text-sm text-secondary">
                {projects.length}{" "}
                {projects.length === 1
                  ? strings.projectsProject.toLowerCase()
                  : strings.projectsTabList.toLowerCase()}
              </p>
            </div>
          </div>
          {/* `min-w-0`, not `shrink-0`: the group already wraps, but a
              flex item that refuses to shrink keeps its one-line width and
              overflows a phone instead of wrapping. */}
          <div className="flex min-w-0 flex-wrap items-center justify-end gap-2">
            <Button
              variant="ghost"
              icon={<CopyPlus size={16} />}
              className="!border-transparent !bg-raised enabled:hover:!bg-default enabled:hover:!text-primary"
              onClick={onNewFromTemplate}
            >
              {strings.projectsTemplateNew}
            </Button>
            <Button icon={<Plus size={16} />} onClick={onNewProject}>{strings.projectsNew}</Button>
          </div>
        </div>
        {/* Scrolls horizontally on purpose — the table keeps its readable
            column widths and the strip pans; the responsive e2e sweep exempts
            marked containers from its element-width invariant. */}
        <div className="overflow-x-auto" data-allow-overflow="">
          <table className="w-full min-w-[76rem] border-collapse text-sm">
          <thead>
            <tr className="bg-raised/60 text-left text-xs font-semibold uppercase tracking-wide text-tertiary">
              <th scope="col" className="px-5 py-3">{strings.projectsProject}</th>
              <th scope="col" className="px-4 py-3">{strings.projectsCustomer}</th>
              <th scope="col" className="px-4 py-3 text-right">
                {strings.projectsRate}
              </th>
              <th scope="col" className="px-4 py-3 text-right">
                {strings.projectsHoursLogged}
              </th>
              <th scope="col" className="px-4 py-3">{strings.projectsBudget}</th>
              <th scope="col" className="px-4 py-3">{strings.projectsLastWorked}</th>
              <th scope="col" className="px-4 py-3">{strings.projectsWorkspaceTasks}</th>
              <th scope="col" className="px-5 py-3" aria-label={strings.projectsActions} />
            </tr>
          </thead>
          <tbody>
            {projects.map((project) => {
              const client = project.client;
              const customer = client === null ? null : customerName(client.customerId);
              const isRunning = runningTimer?.projectId === project.id;
              return (
                <tr
                  key={project.id}
                  className={`group border-t border-subtle transition-colors ${
                    isRunning ? "bg-[var(--accent-soft)]" : "hover:bg-raised/50"
                  }`}
                >
                  <td className="px-5 py-4">
                    <button
                      type="button"
                      className="flex items-center gap-3 text-left text-sm font-semibold text-primary no-underline outline-none hover:text-accent focus-visible:rounded-md focus-visible:ring-2 focus-visible:ring-accent"
                      onClick={() => onOpenTasks(project)}
                    >
                      <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-[var(--accent-soft)] text-accent">
                        <Briefcase className="size-4" aria-hidden="true" />
                      </span>
                      <span className="min-w-0">
                        <span className="block truncate">{project.name}</span>
                        <span className="mt-1 flex flex-wrap items-center gap-2 text-xs font-medium text-secondary">
                          <span className="inline-flex rounded-full bg-raised px-2 py-0.5">
                            {{
                              planned: strings.projectsStatusPlanned,
                              active: strings.projectsStatusActive,
                              on_hold: strings.projectsStatusOnHold,
                              completed: strings.projectsStatusCompleted,
                              cancelled: strings.projectsStatusCancelled,
                            }[project.status]}
                          </span>
                          {project.targetOn !== null && (
                            <span>{strings.projectsTargetOn}: {dayLabel(project.targetOn)}</span>
                          )}
                        </span>
                        {isRunning && (
                          <span className="mt-1 flex items-center gap-1.5 text-xs font-medium text-secondary">
                            <span className="size-1.5 shrink-0 rounded-full bg-success" aria-hidden="true" />
                            {strings.projectsTimerRunning}
                            <span aria-hidden="true">·</span>
                            <span className="tabular-nums">{runningElapsed}</span>
                          </span>
                        )}
                      </span>
                    </button>
                  </td>
                  <td className="px-4 py-4">
                    <span
                      className={
                        client === null
                          ? "inline-flex rounded-full bg-raised px-2.5 py-1 text-xs font-medium text-secondary"
                          : "font-medium text-primary"
                      }
                    >
                      {client === null
                        ? strings.projectsInternal
                        : (customer ?? strings.projectsCustomerUnknown)}
                    </span>
                  </td>
                  <td className="whitespace-nowrap px-4 py-4 text-right font-medium tabular-nums text-primary">
                    {client === null ? "\u2014" : rateLabel(client.rateCents, client.currency)}
                  </td>
                  <td className="whitespace-nowrap px-4 py-4 text-right font-medium tabular-nums text-primary">
                    {durationLabel(project.hours.minutes)}
                    {project.hours.billableMinutes !== project.hours.minutes && (
                      <div className="mt-1 text-xs font-normal text-tertiary">
                        {strings.projectsOfWhichBillable(
                          durationLabel(project.hours.billableMinutes),
                        )}
                      </div>
                    )}
                  </td>
                  <td className="min-w-40 px-4 py-4">
                    <ProjectBudget consumptionBp={project.hours.budgetConsumptionBp} />
                    {client !== null && client.budgetCents !== null && (
                      <span className="mt-1 block text-xs text-tertiary">
                        {amountLabel(client.budgetCents, client.currency)}
                      </span>
                    )}
                  </td>
                  <td className="whitespace-nowrap px-4 py-4 text-secondary">
                    {project.hours.lastWorkedOn === null
                      ? strings.projectsNeverWorked
                      : dayLabel(project.hours.lastWorkedOn)}
                  </td>
                  <td className="min-w-48 px-4 py-4">
                    <div className="flex flex-wrap items-center gap-1.5">
                      {(project.work?.overdueTasks ?? 0) > 0 && (
                        <span className="inline-flex rounded-full bg-danger/10 px-2.5 py-1 text-xs font-semibold text-danger">
                          {strings.projectsOverdueTasks(project.work?.overdueTasks ?? 0)}
                        </span>
                      )}
                      {(project.work?.blockedTasks ?? 0) > 0 && (
                        <span className="inline-flex rounded-full bg-raised px-2.5 py-1 text-xs font-semibold text-primary">
                          {strings.projectsBlockedTasks(project.work?.blockedTasks ?? 0)}
                        </span>
                      )}
                      {(project.work?.overdueTasks ?? 0) === 0 && (project.work?.blockedTasks ?? 0) === 0 && (
                        <span className="text-sm text-secondary">
                          {strings.projectsOpenTasks(project.work?.openTasks ?? 0)}
                        </span>
                      )}
                    </div>
                    {project.work?.nextDueAt != null && (
                      <span className="mt-1 block text-xs text-tertiary">
                        {strings.projectsNextWeek}: {dayLabel(project.work.nextDueAt)}
                      </span>
                    )}
                  </td>
                  <td className="px-5 py-4">
                    <div className="flex items-center justify-end gap-2">
                      {project.kind === "team" && (
                        <IconButton
                          label={`${strings.projectsEdit} ${project.name}`}
                          icon={<PencilLine size={16} />}
                          size="sm"
                          onClick={() => onEditProject(project)}
                        />
                      )}
                      {isRunning ? (
                        <Button icon={<Square size={15} />} onClick={onStopTimer}>
                          {strings.projectsStopTimer}
                        </Button>
                      ) : (
                        <Button
                          variant="ghost"
                          icon={<Play size={16} />}
                          className="shrink-0 !border-transparent !bg-raised enabled:hover:!bg-default enabled:hover:!text-primary"
                          aria-label={strings.projectsStartTimerOn(project.name)}
                          onClick={() => onStartTimer(project)}
                        >
                          {strings.projectsStartTimer}
                        </Button>
                      )}
                      {/* A personal board cannot be a template — the list of
                          templates is the whole workspace's — so the control is
                          absent there rather than offered and refused. */}
                      {project.kind === "team" && (
                        <IconButton
                          label={
                            isTemplate(project.id)
                              ? strings.projectsTemplateUnmarkOn(project.name)
                              : strings.projectsTemplateMarkOn(project.name)
                          }
                          icon={
                            <Star
                              size={16}
                              fill={isTemplate(project.id) ? "currentColor" : "none"}
                            />
                          }
                          size="sm"
                          active={isTemplate(project.id)}
                          onClick={() => onToggleTemplate(project)}
                        />
                      )}
                      <Button
                        variant={client === null ? "primary" : "ghost"}
                        className="shrink-0"
                        onClick={() => onEditClient(project)}
                      >
                        {client === null ? strings.projectsMakeClientWork : strings.projectsEdit}
                      </Button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        </div>
      </section>
    </div>
  );
}
