// The plan: a project's milestones on a date axis, with the board's own tasks
// placed under them.
//
// This is a **rendering of the board that already exists**, not a second model
// of the work (`docs/design/projects.md`, "Milestones and templates"). The
// tasks in every group are the same rows the Tasks module shows — read from
// `/tasks?project=`, grouped here by the placements the plan read answers with
// — so a task closed on the board is closed here, in the same instant, without
// anybody copying anything.
//
// Three things this screen deliberately does not do:
//
// - **It never decides a milestone is reached.** Every task under it can be
//   closed and it still says "not reached", because "the last task closed" is
//   not the statement "the client accepted the deliverable". The bar shows the
//   work; the button is a person's.
// - **It never judges lateness itself.** `late` is the server's flag, computed
//   against the server's date, so a browser with a wrong clock cannot clear its
//   own late list.
// - **It draws no bar for a plan of one date.** A timeline needs a span; one
//   milestone is a date, and it is shown as one.
import { useCallback, useEffect, useMemo, useState } from "react";
import { CheckCircle2, Flag, Plus, X } from "lucide-react";
import { useSearchParams } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import type { Task } from "../jmap";
import { projectsMessage, useProjectsApi } from "./api";
import { dayLabel, dayString, dayValue } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import { MilestoneDialog } from "./MilestoneDialog";
import { ProjectScopePicker } from "./ProjectScopePicker";
import { resolveProjectScope } from "./scope";
import type { Milestone, Project, ProjectPlan } from "./types";
const styles = {
  page: "flex min-h-0 flex-col gap-4 overflow-auto px-5 py-4",
  toolbar: "flex flex-wrap items-center gap-3",
  select:
    "w-full rounded-md border border-default bg-surface px-3 py-2 text-sm text-primary focus-visible:outline-2 focus-visible:outline-accent",
  toolbarSpacer: "flex-1",
  timeline: "relative mx-6 h-14",
  timelineTrack: "absolute inset-x-0 top-2.5 h-0.5 bg-subtle",
  timelineMark:
    "absolute top-0 flex -translate-x-1/2 flex-col items-center gap-1",
  timelineDot: "mt-1 h-3 w-3 rounded-full border-2 border-tertiary bg-surface",
  timelineDotDone: "!border-success !bg-success",
  timelineDotLate: "!border-danger !bg-danger",
  timelineDay: "whitespace-nowrap text-xs text-tertiary",
  plan: "m-0 grid list-none gap-4 p-0 xl:grid-cols-2",
  milestone:
    "overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm transition-colors hover:border-default hover:bg-raised/30",
  milestoneHead: "flex flex-wrap items-start gap-3 px-5 py-4",
  rowName: "text-left text-base font-semibold text-link hover:text-accent",
  muted: "text-tertiary",
  chip: "inline-flex items-center whitespace-nowrap rounded-full px-2 py-0.5 text-xs font-medium",
  chipGood: "bg-[var(--success-tint)] text-success",
  chipBad: "bg-[var(--danger-tint)] text-danger",
  milestoneTasks: "m-0 list-none border-t border-subtle bg-raised/40 p-0",
  milestoneTask:
    "flex min-h-11 items-center gap-3 border-b border-subtle px-5 py-2.5 text-sm text-primary last:border-b-0",
  taskDone: "text-tertiary line-through",
  unplaced: "rounded-md border border-dashed border-default",
  unplacedTitle: "m-0 px-4 py-2.5 text-sm font-medium text-secondary",
} as const;

const ALL_PROJECTS = "all";

/** Where a milestone sits along the timeline, as a percentage of the plan's
 *  span. `null` when the plan has no span to place it on — a single date, or
 *  several on one day — in which case the markers are simply spread evenly and
 *  the axis says nothing it cannot support. */
function offsets(milestones: Milestone[]): number[] | null {
  const days = milestones.map((m) => dayValue(m.dueOn)?.getTime() ?? null);
  if (days.some((d) => d === null)) return null;
  const times = days as number[];
  const first = Math.min(...times);
  const last = Math.max(...times);
  if (last === first) return null;
  return times.map((t) => ((t - first) / (last - first)) * 100);
}

export function PlanView({
  projects,
  projectsLoading,
  revision,
  onChanged,
}: {
  projects: Project[];
  projectsLoading: boolean;
  /** Bumped by the module when anything it loaded changed. */
  revision: number;
  onChanged: () => void;
}) {
  const api = useProjectsApi();
  const client = useJmapClient();
  const [searchParams, setSearchParams] = useSearchParams();

  const requestedProjectId = searchParams.get("project");
  const resolvedProjectId = resolveProjectScope(
    requestedProjectId,
    projectsLoading,
    projects,
  );
  const projectId = resolvedProjectId ?? ALL_PROJECTS;
  const [plan, setPlan] = useState<ProjectPlan>({
    milestones: [],
    placements: [],
  });
  const [tasks, setTasks] = useState<Task[]>([]);
  const [editing, setEditing] = useState<Milestone | "new" | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [localRevision, setLocalRevision] = useState(0);

  const bump = useCallback(() => setLocalRevision((r) => r + 1), []);

  function selectProject(next: string) {
    const params = new URLSearchParams(searchParams);
    if (next === ALL_PROJECTS) params.delete("project");
    else params.set("project", next);
    setSearchParams(params);
  }

  useEffect(() => {
    if (projectsLoading) return;
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const scopedProjects =
          projectId === ALL_PROJECTS
            ? projects
            : projects.filter((project) => project.id === projectId);
        // Each project is fetched in parallel. The portfolio timeline is the
        // same plans combined, never a separate source of truth.
        const loaded = await Promise.all(
          scopedProjects.map(async (project) => {
            const [loadedPlan, loadedTasks] = await Promise.all([
              api.plan(project.id),
              client.tasks(project.id),
            ]);
            return { plan: loadedPlan, tasks: loadedTasks };
          }),
        );
        if (live) {
          setPlan({
            milestones: loaded.flatMap((result) => result.plan.milestones),
            placements: loaded.flatMap((result) => result.plan.placements),
          });
          setTasks(loaded.flatMap((result) => result.tasks));
          setError(null);
        }
      } catch (err) {
        if (live)
          setError(projectsMessage(err, strings.projectsPlanLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [
    api,
    client,
    projectId,
    projects,
    projectsLoading,
    revision,
    localRevision,
  ]);

  const project = projects.find((p) => p.id === projectId) ?? null;
  const projectById = useMemo(
    () => new Map(projects.map((item) => [item.id, item])),
    [projects],
  );
  const placedIn = useMemo(() => {
    const byTask = new Map<string, string>();
    for (const placement of plan.placements)
      byTask.set(placement.taskId, placement.milestoneId);
    return byTask;
  }, [plan.placements]);
  const unplaced = tasks.filter((task) => !placedIn.has(task.id));
  const positions = offsets(plan.milestones);

  /** Puts a task under a milestone, or takes it out when `milestoneId` is
   *  empty. A failure is shown and nothing is guessed: the plan is re-read
   *  either way, so what is on screen is what the server holds. */
  async function place(taskId: string, milestoneId: string) {
    try {
      if (milestoneId === "") await api.unplaceTask(taskId);
      else await api.placeTask(taskId, milestoneId);
      setError(null);
    } catch (err) {
      setError(projectsMessage(err, strings.projectsSaveFailed));
    } finally {
      bump();
    }
  }

  /** Marks a milestone reached, or puts it back ahead of us. */
  async function setDone(milestone: Milestone, done: boolean) {
    try {
      await api.setMilestoneDone(milestone.id, done);
      setError(null);
    } catch (err) {
      setError(projectsMessage(err, strings.projectsSaveFailed));
    } finally {
      bump();
    }
  }

  return (
    <div className={styles.page}>
      <section className="rounded-2xl border border-default bg-surface shadow-sm">
        <div className="flex flex-wrap items-center justify-between gap-4 px-5 py-4">
          <div className="flex min-w-0 items-center gap-3">
            <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
              <Flag size={20} aria-hidden="true" />
            </span>
            <div>
              <h2 className="m-0 text-lg font-semibold text-primary">
                {strings.projectsTabPlan}
              </h2>
              <p className="m-0 mt-0.5 text-sm text-secondary">
                {projectId === ALL_PROJECTS
                  ? strings.projectsAllProjects
                  : project?.name}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            {loading && <Spinner size={16} />}
            {project !== null && plan.milestones.length > 0 && (
              <Button
                icon={<Plus size={15} />}
                onClick={() => setEditing("new")}
              >
                {strings.projectsMilestoneAdd}
              </Button>
            )}
          </div>
        </div>
        <div className="border-t border-subtle px-5 py-4">
          <ProjectScopePicker
            compact
            projects={projects}
            value={projectId === ALL_PROJECTS ? null : projectId}
            disabled={projectsLoading}
            onChange={(nextProjectId) =>
              selectProject(nextProjectId ?? ALL_PROJECTS)
            }
          />
        </div>
        {positions !== null && (
          <div className="border-t border-subtle py-4">
            <div className={styles.timeline} aria-hidden="true">
              <span className={styles.timelineTrack} />
              {plan.milestones.map((milestone, index) => (
                <span
                  key={milestone.id}
                  className={styles.timelineMark}
                  style={{ left: `${positions[index] ?? 0}%` }}
                >
                  <span
                    className={`${styles.timelineDot} ${milestone.done ? styles.timelineDotDone : milestone.late ? styles.timelineDotLate : ""}`}
                  />
                  <span className={styles.timelineDay}>
                    {dayLabel(milestone.dueOn, {
                      day: "numeric",
                      month: "short",
                    })}
                  </span>
                </span>
              ))}
            </div>
          </div>
        )}
      </section>

      {error !== null && <ErrorBanner message={error} />}

      {loading && plan.milestones.length === 0 ? (
        <div className="flex min-h-80 items-center justify-center">
          <Spinner size={20} />
        </div>
      ) : plan.milestones.length === 0 ? (
        <EmptyState
          Icon={Flag}
          title={
            project === null
              ? strings.projectsTimelineAllEmptyTitle
              : strings.projectsPlanEmptyTitle
          }
          body={
            project === null
              ? strings.projectsTimelineAllEmptyBody
              : strings.projectsPlanEmptyBody
          }
          {...(project === null
            ? {}
            : {
                cta: strings.projectsMilestoneAdd,
                onCta: () => setEditing("new"),
              })}
        />
      ) : (
        <>
          <ul className={styles.plan}>
            {plan.milestones.map((milestone) => {
              const under = tasks.filter(
                (task) => placedIn.get(task.id) === milestone.id,
              );
              return (
                <li key={milestone.id} className={styles.milestone}>
                  <div className={styles.milestoneHead}>
                    <button
                      type="button"
                      className={styles.rowName}
                      onClick={() => {
                        if (projectId === ALL_PROJECTS)
                          selectProject(milestone.projectId);
                        setEditing(milestone);
                      }}
                    >
                      {milestone.name}
                    </button>
                    {projectId === ALL_PROJECTS && (
                      <span
                        className={`${styles.chip} bg-subtle text-secondary`}
                      >
                        {projectById.get(milestone.projectId)?.name ??
                          strings.projectsProject}
                      </span>
                    )}
                    <span className={styles.muted}>
                      {dayLabel(milestone.dueOn)}
                    </span>
                    {milestone.done ? (
                      <span className={`${styles.chip} ${styles.chipGood}`}>
                        {strings.projectsMilestoneReached}
                      </span>
                    ) : milestone.late ? (
                      <span className={`${styles.chip} ${styles.chipBad}`}>
                        {strings.projectsMilestoneLate}
                      </span>
                    ) : null}
                    <div className="ml-auto flex min-w-36 flex-col gap-1.5">
                      <span className="text-right text-xs text-secondary">
                        {milestone.taskCount === 0
                          ? strings.projectsMilestoneNoTasks
                          : strings.projectsMilestoneTasksClosed(
                              milestone.taskDoneCount,
                              milestone.taskCount,
                            )}
                      </span>
                      {milestone.taskCount > 0 && (
                        <span
                          className="h-1.5 overflow-hidden rounded-full bg-raised"
                          role="progressbar"
                          aria-valuemin={0}
                          aria-valuemax={milestone.taskCount}
                          aria-valuenow={milestone.taskDoneCount}
                        >
                          <span
                            className="block h-full rounded-full bg-accent"
                            style={{
                              width: `${(milestone.taskDoneCount / milestone.taskCount) * 100}%`,
                            }}
                          />
                        </span>
                      )}
                    </div>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => void setDone(milestone, !milestone.done)}
                    >
                      {milestone.done
                        ? strings.projectsMilestoneReopen
                        : strings.projectsMilestoneReach}
                    </Button>
                  </div>
                  {under.length > 0 && (
                    <ul className={styles.milestoneTasks}>
                      {under.map((task) => (
                        <li key={task.id} className={styles.milestoneTask}>
                          <span
                            className="grid size-5 shrink-0 place-items-center rounded-full border border-default text-success"
                            aria-hidden="true"
                          >
                            {task.completedAt !== null && (
                              <CheckCircle2 size={14} />
                            )}
                          </span>
                          <span
                            className={
                              task.completedAt === null
                                ? undefined
                                : styles.taskDone
                            }
                          >
                            {task.title}
                          </span>
                          <span className={styles.toolbarSpacer} />
                          <Button
                            variant="ghost"
                            size="sm"
                            icon={<X size={15} />}
                            aria-label={strings.projectsPlanRemove}
                            onClick={() => void place(task.id, "")}
                          />
                        </li>
                      ))}
                    </ul>
                  )}
                </li>
              );
            })}
          </ul>

          {unplaced.length > 0 && (
            <section className={styles.unplaced}>
              <h2 className={styles.unplacedTitle}>
                {strings.projectsPlanUnplaced}
              </h2>
              <ul className={styles.milestoneTasks}>
                {unplaced.map((task) => (
                  <li key={task.id} className={styles.milestoneTask}>
                    <span
                      className={
                        task.completedAt === null ? undefined : styles.taskDone
                      }
                    >
                      {task.title}
                    </span>
                    <span className={styles.toolbarSpacer} />
                    {/* The board's own task, put into the plan without leaving
                        the plan — and a task under one milestone is under no
                        other, which is why this is a single choice. */}
                    <select
                      className={styles.select}
                      value=""
                      aria-label={strings.projectsPlanPlaceTask(task.title)}
                      onChange={(e) => void place(task.id, e.target.value)}
                    >
                      <option value="">{strings.projectsPlanPlace}</option>
                      {plan.milestones.map((milestone) =>
                        milestone.projectId === task.projectId ? (
                          <option key={milestone.id} value={milestone.id}>
                            {milestone.name}
                          </option>
                        ) : null,
                      )}
                    </select>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </>
      )}

      {editing !== null && project !== null && (
        <MilestoneDialog
          milestone={editing === "new" ? null : editing}
          projectId={project.id}
          projectName={project.name}
          defaultDay={dayString(new Date())}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            bump();
            onChanged();
          }}
        />
      )}
    </div>
  );
}
