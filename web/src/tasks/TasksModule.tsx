// The Tasks module (ADR 0021–0023): a project sidebar, a main area that renders
// the SAME task rows as a board or a list (an instant, lossless toggle), a
// slide-in detail panel, plus "what's on my plate" and the AI proposals inbox
// (propose-then-approve). All data goes through the authenticated /tasks API;
// the board's grouping and the list's flattening are the client's job.
import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import {
  ArrowLeft,
  CircleAlert,
  CalendarRange,
  ClipboardList,
  GanttChartSquare,
  FolderKanban,
  LayoutDashboard,
  LayoutGrid,
  List,
  Paperclip,
  Plus,
  Search,
  Sparkles,
  Sun,
  RefreshCw,
} from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient, type Task, type TaskDepEdgeDto, type TaskProject } from "../jmap";
import { Spinner } from "../ds";
import { useAuth } from "../auth";
import { BoardView } from "./BoardView";
import { ListView } from "./ListView";
import { TimelineView } from "./TimelineView";
import { CalendarView } from "./CalendarView";
import { TaskToolbar } from "./TaskToolbar";
import { OverviewView } from "./OverviewView";
import { ProjectOverviewView } from "./ProjectOverviewView";
import { FilesView } from "./FilesView";
import { NewTaskModal } from "./NewTaskModal";
import { TaskDetail } from "./TaskDetail";
import { Avatar, DueChip, PriorityChip } from "./parts";
import { DEFAULT_CONFIG, filterTasks, type ViewConfig } from "./viewConfig";
import { projectsMessage, useProjectsApi } from "../projects/api";
import { EditProjectDialog } from "../projects/EditProjectDialog";
import type { Project, ProjectDraft, ProjectPlan } from "../projects/types";

type View = "overview" | "list" | "board" | "timeline" | "calendar" | "files";

function isView(value: string | undefined | null): value is View {
  return value === "overview" || value === "list" || value === "board" ||
    value === "timeline" || value === "calendar" || value === "files";
}

function projectStatusLabel(status: Project["status"]): string {
  return {
    planned: strings.projectsStatusPlanned,
    active: strings.projectsStatusActive,
    on_hold: strings.projectsStatusOnHold,
    completed: strings.projectsStatusCompleted,
    cancelled: strings.projectsStatusCancelled,
  }[status];
}

type Mode = { type: "project"; id: string } | { type: "plate" } | { type: "proposals" };

export function TasksModule({
  projectId,
  projectsContext = false,
  workspaceView,
}: {
  projectId?: string;
  projectsContext?: boolean;
  workspaceView?: string;
} = {}) {
  const client = useJmapClient();
  const projectsApi = useProjectsApi();
  const navigate = useNavigate();
  const { identity } = useAuth();
  const [projects, setProjects] = useState<TaskProject[]>([]);
  const [edges, setEdges] = useState<TaskDepEdgeDto[]>([]);
  const [mode, setMode] = useState<Mode>({ type: "plate" });
  const [view, setView] = useState<View>(
    isView(workspaceView) ? workspaceView : "overview",
  );
  const [config, setConfig] = useState<ViewConfig>(DEFAULT_CONFIG);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [proposals, setProposals] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<string | null>(null);
  const [engagement, setEngagement] = useState<Project | null>(null);
  const [engagementLoading, setEngagementLoading] = useState(projectId !== undefined);
  const [engagementError, setEngagementError] = useState<string | null>(null);
  const [engagementRevision, setEngagementRevision] = useState(0);
  const [editingProject, setEditingProject] = useState(false);
  const [projectPlan, setProjectPlan] = useState<ProjectPlan>({ milestones: [], placements: [] });

  // Open a task arrived at from workspace search (?open=<taskId>).
  const [searchParams, setSearchParams] = useSearchParams();
  useEffect(() => {
    const id = searchParams.get("open");
    if (id === null) return;
    const next = new URLSearchParams(searchParams);
    next.delete("open");
    setSearchParams(next, { replace: true });
    setSelected(id);
  }, [searchParams, setSearchParams]);
  const [search, setSearch] = useState("");
  const [creating, setCreating] = useState<{ status?: string; dueDate?: string } | null>(null);

  // Projects links here with a stable project id. Keep the query in the URL so
  // refreshes and shared links reopen the same task workspace.
  useEffect(() => {
    const requested = projectId ?? searchParams.get("project");
    if (requested === null || !projects.some((project) => project.id === requested)) return;
    setMode((current) =>
      current.type === "project" && current.id === requested
        ? current
        : { type: "project", id: requested },
    );
  }, [projectId, projects, searchParams]);

  useEffect(() => {
    const requested = workspaceView ?? searchParams.get("view");
    if (isView(requested)) {
      setView(requested);
    }
  }, [searchParams, workspaceView]);

  function openProject(id: string) {
    if (projectId !== undefined || projectsContext) {
      navigate(`/projects/${encodeURIComponent(id)}/overview`);
      return;
    }
    const next = new URLSearchParams(searchParams);
    next.set("project", id);
    next.set("view", "overview");
    setSearchParams(next);
    setMode({ type: "project", id });
    setView("overview");
  }

  function openView(nextView: View) {
    setView(nextView);
    if (projectId !== undefined) {
      navigate(`/projects/${encodeURIComponent(projectId)}/${nextView}`);
      return;
    }
    const next = new URLSearchParams(searchParams);
    next.set("view", nextView);
    setSearchParams(next, { replace: true });
  }

  const projectName = useCallback(
    (id: string) => projects.find((p) => p.id === id)?.name ?? "",
    [projects],
  );

  /** The project a new task lands in: the active one, else the personal one. */
  function targetProject(): string | undefined {
    if (mode.type === "project") return mode.id;
    return (projects.find((p) => p.kind === "personal") ?? projects[0])?.id;
  }

  function openCreate(status?: string, dueDate?: string) {
    const next: { status?: string; dueDate?: string } = {};
    if (status !== undefined) next.status = status;
    if (dueDate !== undefined) next.dueDate = dueDate;
    setCreating(next);
  }

  function localDateValue(day: Date): string {
    const year = day.getFullYear();
    const month = String(day.getMonth() + 1).padStart(2, "0");
    const date = String(day.getDate()).padStart(2, "0");
    return `${year}-${month}-${date}`;
  }

  const loadProjects = useCallback(async () => {
    try {
      const ps = await client.taskProjects();
      setProjects(ps);
    } catch {
      /* keep */
    }
  }, [client]);

  const loadProposals = useCallback(async () => {
    try {
      setProposals(await client.taskProposals());
    } catch {
      /* keep */
    }
  }, [client]);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      if (mode.type === "project") {
        setTasks(await client.tasks(mode.id));
        setEdges(await client.projectDependencies(mode.id).catch(() => []));
      } else if (mode.type === "plate") {
        setTasks(await client.myPlate());
        setEdges([]);
      }
    } finally {
      setLoading(false);
    }
  }, [client, mode]);

  useEffect(() => {
    void loadProjects();
    void loadProposals();
  }, [loadProjects, loadProposals]);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    if (projectId === undefined) {
      setEngagement(null);
      setEngagementLoading(false);
      setEngagementError(null);
      setProjectPlan({ milestones: [], placements: [] });
      return;
    }
    let current = true;
    setEngagementLoading(true);
    setEngagementError(null);
    void Promise.all([
      projectsApi.project(projectId),
      projectsApi.plan(projectId),
    ]).then(([nextProject, nextPlan]) => {
      if (!current) return;
      setEngagement(nextProject);
      setProjectPlan(nextPlan);
    }).catch((error: unknown) => {
      if (!current) return;
      setEngagement(null);
      setEngagementError(projectsMessage(error, strings.projectsWorkspaceLoadFailed));
      setProjectPlan({ milestones: [], placements: [] });
    }).finally(() => {
      if (current) setEngagementLoading(false);
    });
    return () => { current = false; };
  }, [engagementRevision, projectId, projectsApi]);

  const activeProject = useMemo(
    () => (mode.type === "project" ? projects.find((p) => p.id === mode.id) : undefined),
    [mode, projects],
  );

  async function updateEngagement(draft: ProjectDraft) {
    if (engagement === null) return;
    const updated = await projectsApi.updateProject(engagement.id, draft);
    setEngagement(updated);
    setEditingProject(false);
    await loadProjects();
  }

  /** Optimistic move: update local state instantly, then persist. */
  async function move(id: string, status: string, position: number) {
    setTasks((ts) => ts.map((t) => (t.id === id ? { ...t, status, position } : t)));
    try {
      await client.moveTask(id, status, position);
    } catch {
      void reload();
    }
  }

  const title =
    mode.type === "plate"
      ? (projectsContext ? strings.projectsTabMyWork : strings.taskMyPlate)
      : mode.type === "proposals"
        ? strings.taskProposals
        : (engagement?.name ?? activeProject?.name ?? strings.moduleTasks);

  return (
    <div className="flex h-full min-h-0 bg-app">
      {projectId === undefined && (
        <aside className="flex w-60 shrink-0 flex-col gap-4 overflow-y-auto border-r border-default bg-sunken p-4 max-md:w-52 max-sm:hidden">
          <button
            type="button"
            className={`flex min-h-10 w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm text-primary !no-underline transition-colors hover:bg-raised hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${mode.type === "plate" ? "bg-selected font-medium" : ""}`}
            onClick={() => setMode({ type: "plate" })}
          >
            <Sun size={16} /> {projectsContext ? strings.projectsTabMyWork : strings.taskMyPlate}
          </button>
          <button
            type="button"
            className={`flex min-h-10 w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm text-primary !no-underline transition-colors hover:bg-raised hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${mode.type === "proposals" ? "bg-selected font-medium" : ""}`}
            onClick={() => {
              void loadProposals();
              setMode({ type: "proposals" });
            }}
          >
            <Sparkles size={16} /> {strings.taskProposals}
            {proposals.length > 0 && <span className="ml-auto inline-flex min-h-5 min-w-5 items-center justify-center rounded-full bg-accent px-1.5 text-[11px] font-semibold text-on-accent">{proposals.length}</span>}
          </button>

          <div className="flex flex-col gap-1">
            <div className="flex items-center justify-between px-1 text-xs font-semibold uppercase tracking-wide text-secondary">
              <span>{strings.taskProjects}</span>
              <button
                type="button"
                className="inline-flex size-8 items-center justify-center rounded-md text-secondary !no-underline transition-colors hover:bg-raised hover:text-accent hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                onClick={() => navigate("/projects/list?new=1")}
                aria-label={strings.taskNewProject}
                title={strings.taskNewProject}
              >
                <Plus size={15} />
              </button>
            </div>
            {projects.map((p) => (
              <button
                key={p.id}
                type="button"
                className={`flex min-h-10 w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm text-primary !no-underline transition-colors hover:bg-raised hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${
                  mode.type === "project" && mode.id === p.id ? "bg-selected font-medium" : ""
                }`}
                onClick={() => openProject(p.id)}
              >
                <span
                  className="size-2.5 shrink-0 rounded-[3px]"
                  style={{ background: p.color ?? "var(--accent)" }}
                  aria-hidden="true"
                />
                <span className="min-w-0 flex-1 truncate">{p.name}</span>
              </button>
            ))}
          </div>
        </aside>
      )}

      <section className="flex min-w-0 flex-1 flex-col">
        <header className="flex items-center gap-6 border-b border-subtle bg-surface px-6 py-5 max-lg:flex-wrap max-sm:px-4">
          <div className="flex min-w-0 flex-1 items-center gap-3">
            {projectId !== undefined && (
              <button
                type="button"
                className="shrink-0 rounded-lg text-secondary !no-underline transition-colors hover:bg-raised hover:text-primary hover:!no-underline focus:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                onClick={() => navigate("/projects/list")}
                aria-label={strings.projectsTabList}
              >
                <span className="flex min-h-10 items-center gap-2 px-3">
                  <ArrowLeft size={16} aria-hidden="true" />
                  <span className="max-sm:hidden">{strings.projectsTabList}</span>
                </span>
              </button>
            )}
            {projectId !== undefined && <span className="h-8 w-px shrink-0 bg-[var(--border-subtle)]" aria-hidden="true" />}
            <div className="flex min-w-0 items-center gap-3">
              {projectId !== undefined && (
                <span className="inline-flex size-10 shrink-0 items-center justify-center rounded-xl bg-[var(--accent-tint)] text-accent" aria-hidden="true">
                  <FolderKanban size={20} />
                </span>
              )}
              <div className="min-w-0">
                <h1 className="m-0 truncate text-2xl font-bold text-primary">{title}</h1>
                {projectId !== undefined && engagement !== null && (
                  <p className="mt-1 inline-flex items-center rounded-full bg-raised px-2.5 py-0.5 text-xs font-medium text-secondary">
                    {projectStatusLabel(engagement.status)}
                  </p>
                )}
              </div>
            </div>
          </div>
          <form
            className="flex h-10 w-full max-w-[28.75rem] items-center gap-2 rounded-full border border-default bg-app px-3 transition-shadow focus-within:border-accent focus-within:ring-2 focus-within:ring-accent/15 max-lg:order-3 max-lg:max-w-none max-lg:basis-full"
            role="search"
            onSubmit={(e) => e.preventDefault()}
          >
            <Search size={16} className="shrink-0 text-tertiary" aria-hidden />
            <input
              className="min-w-0 flex-1 border-0 bg-transparent text-base text-primary outline-none placeholder:text-tertiary"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder={strings.taskSearchPlaceholder}
              aria-label={strings.taskSearchPlaceholder}
            />
          </form>
          <div className="flex shrink-0 items-center gap-3">
            {loading && <Spinner size={16} />}
            {tasks.length > 0 && mode.type !== "proposals" && (
              <button type="button" className="inline-flex min-h-10 items-center justify-center gap-2 rounded-lg bg-accent px-4 py-2 text-sm font-semibold text-on-accent !no-underline shadow-sm transition-colors hover:bg-accent-hover hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2" onClick={() => openCreate()}>
                <Plus size={16} /> {strings.taskNew}
              </button>
            )}
          </div>
        </header>

        {mode.type !== "proposals" && (
          <div className="flex gap-3 overflow-x-auto border-b border-subtle bg-surface px-6 py-2 max-sm:gap-2 max-sm:px-4" role="tablist" aria-label={title}>
            {(
              [
                { id: "overview", label: strings.taskOverview, Icon: LayoutDashboard },
                { id: "list", label: projectId !== undefined ? strings.projectsWorkspaceTasks : strings.taskList, Icon: List },
                { id: "board", label: strings.taskBoard, Icon: LayoutGrid },
                { id: "timeline", label: strings.taskTimeline, Icon: GanttChartSquare },
                { id: "calendar", label: strings.taskCalendar, Icon: CalendarRange },
                { id: "files", label: strings.taskFiles, Icon: Paperclip },
              ] as const
            ).map((t) => (
              <button
                key={t.id}
                type="button"
                role="tab"
                aria-selected={view === t.id}
                className={`inline-flex min-h-11 shrink-0 items-center gap-2.5 rounded-lg border-b-2 px-4 py-2.5 text-sm !no-underline transition-colors hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent ${view === t.id ? "border-accent bg-[var(--accent-soft)] font-semibold !text-accent" : "border-transparent bg-transparent font-medium !text-secondary hover:bg-raised hover:!text-primary"}`}
                onClick={() => openView(t.id)}
              >
                <t.Icon size={16} aria-hidden="true" />
                <span>{t.label}</span>
              </button>
            ))}
          </div>
        )}

        {mode.type !== "proposals" && view !== "overview" && view !== "files" && tasks.length > 0 && (
          <TaskToolbar config={config} onChange={setConfig} />
        )}

        <div className="min-h-0 flex-1 overflow-auto">
          {mode.type === "proposals" ? (
            <ProposalsInbox
              proposals={proposals}
              onDone={async () => {
                await loadProposals();
                await reload();
              }}
            />
          ) : projectId !== undefined && engagementLoading ? (
            <div className="flex min-h-[28rem] items-center justify-center" role="status">
              <Spinner size={24} />
            </div>
          ) : projectId !== undefined && engagementError !== null ? (
            <section className="mx-auto flex min-h-[28rem] w-full max-w-2xl flex-col items-center justify-center px-6 py-12 text-center">
              <span className="flex size-14 items-center justify-center rounded-2xl bg-[var(--danger-tint)] text-danger" aria-hidden="true">
                <CircleAlert size={26} />
              </span>
              <h2 className="mt-4 text-lg font-semibold text-primary">{strings.projectsWorkspaceUnavailable}</h2>
              <p className="mt-2 max-w-[46ch] text-sm leading-6 text-secondary" role="alert">{engagementError}</p>
              <button
                type="button"
                className="mt-5 inline-flex min-h-11 items-center justify-center gap-2 rounded-lg bg-accent px-5 py-2.5 text-sm font-semibold text-on-accent !no-underline transition-colors hover:bg-accent-hover hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2"
                onClick={() => setEngagementRevision((current) => current + 1)}
              >
                <RefreshCw size={16} aria-hidden="true" />
                {strings.projectsRetry}
              </button>
            </section>
          ) : view === "overview" && projectId !== undefined && engagement !== null ? (
            <ProjectOverviewView
              project={engagement}
              plan={projectPlan}
              tasks={tasks}
              edges={edges}
              onAddTask={() => openCreate()}
              onOpenTask={setSelected}
              onOpenTasks={() => openView("list")}
              onOpenTimesheet={() => navigate(`/projects/week?project=${encodeURIComponent(projectId)}`)}
              onOpenTimeline={() => navigate(`/projects/timeline?project=${encodeURIComponent(projectId)}`)}
              onOpenReport={() => navigate(`/projects/reports?project=${encodeURIComponent(projectId)}`)}
              onEditProject={() => setEditingProject(true)}
              onOpenInvoice={(id) => navigate(`/billing/invoices/${encodeURIComponent(id)}`)}
            />
          ) : tasks.length === 0 ? (
            <div className="p-6 max-sm:p-4">
              <section className="flex min-h-[28rem] flex-col items-center justify-center gap-3 rounded-2xl border border-default bg-surface px-6 py-12 text-center shadow-sm">
                <span className="mb-2 inline-flex size-20 items-center justify-center rounded-full bg-[var(--accent-tint)] text-accent-hover" aria-hidden="true">
                  <ClipboardList size={40} />
                </span>
                <h2 className="m-0 text-xl font-bold text-primary">{strings.taskEmptyTitle}</h2>
                <p className="m-0 max-w-sm text-base leading-relaxed text-secondary">{strings.taskEmptyBody}</p>
                <button type="button" className="mt-3 inline-flex min-h-11 items-center justify-center gap-2 rounded-lg bg-accent px-6 py-3 text-sm font-semibold text-on-accent !no-underline shadow-sm transition-colors hover:bg-accent-hover hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2" onClick={() => openCreate()}>
                  <Plus size={17} /> {strings.taskCreateFirst}
                </button>
              </section>
            </div>
          ) : view === "overview" ? (
            <OverviewView
              tasks={tasks}
              me={identity?.email}
              onOpen={setSelected}
              onAdd={() => openCreate()}
              onViewAll={() => openView("list")}
            />
          ) : view === "board" ? (
            <BoardView
              tasks={filterTasks(tasks, config, identity?.email)}
              onOpen={setSelected}
              onMove={move}
              onAdd={(status) => openCreate(status)}
            />
          ) : view === "timeline" ? (
            <TimelineView tasks={filterTasks(tasks, config, identity?.email)} edges={edges} onOpen={setSelected} />
          ) : view === "calendar" ? (
            <CalendarView
              tasks={filterTasks(tasks, config, identity?.email)}
              onOpen={setSelected}
              onAdd={(day) => openCreate(undefined, localDateValue(day))}
            />
          ) : view === "files" ? (
            <FilesView
              projectId={targetProject() ?? ""}
              onOpen={setSelected}
              onCreate={() => openCreate()}
            />
          ) : (
            <ListView
              tasks={tasks}
              config={config}
              projectName={projectName}
              me={identity?.email}
              search={search}
              onOpen={setSelected}
              onMove={move}
              onAdd={(status) => openCreate(status)}
            />
          )}
        </div>
        {editingProject && engagement?.kind === "team" && (
          <EditProjectDialog
            project={engagement}
            onClose={() => setEditingProject(false)}
            onSave={updateEngagement}
          />
        )}
      </section>

      {selected !== null && (
        <TaskDetail
          taskId={selected}
          projectName={(() => {
            const st = tasks.find((t) => t.id === selected);
            return st !== undefined ? projectName(st.projectId) : undefined;
          })()}
          onClose={() => setSelected(null)}
          onChanged={() => {
            void reload();
            void loadProposals();
          }}
        />
      )}

      {creating !== null && (
        <NewTaskModal
          projects={projects}
          defaultProjectId={targetProject()}
          defaultStatus={creating.status}
          defaultDueDate={creating.dueDate}
          onClose={() => setCreating(null)}
          onCreated={() => {
            void reload();
          }}
        />
      )}
    </div>
  );
}

/** The AI proposals inbox: accept the real ones, reject the noise (ADR 0023). */
function ProposalsInbox({ proposals, onDone }: { proposals: Task[]; onDone: () => void }) {
  const client = useJmapClient();
  if (proposals.length === 0) {
    return (
      <div className="flex min-h-[min(35rem,calc(100vh-11.875rem))] flex-col items-center justify-center gap-3 px-6 py-10 text-center">
        <span className="mb-2 inline-flex size-[88px] items-center justify-center rounded-full bg-accent-tint text-accent-hover ring-1 ring-inset ring-accent/20">
          <Sparkles size={38} />
        </span>
        <h2 className="m-0 text-xl font-bold text-primary">{strings.taskNoProposalsTitle}</h2>
        <p className="m-0 max-w-[360px] text-base leading-relaxed text-secondary">{strings.taskNoProposals}</p>
      </div>
    );
  }
  return (
    <div className="w-full max-w-2xl p-4">
      {proposals.map((t) => (
        <div key={t.id} className="mb-3 flex flex-col gap-2 rounded-lg border border-default bg-surface p-3">
          <div className="flex items-center gap-2">
            <span className="inline-flex items-center gap-1 rounded-full bg-accent-tint px-2 py-0.5 text-[11px] font-semibold text-accent">
              <Sparkles size={11} /> {strings.taskAiSuggested}
            </span>
            <strong className="text-sm text-primary">{t.title}</strong>
          </div>
          <div className="flex flex-wrap items-center gap-2 text-xs text-secondary">
            {t.assignee && <Avatar email={t.assignee} />}
            {t.assignee && <span>{t.assignee}</span>}
            {t.dueAt && <DueChip iso={t.dueAt} done={false} />}
            <PriorityChip priority={t.priority} />
            {t.sourceKind && (
              <span>· {t.sourceKind === "event" ? strings.taskFromEvent : strings.taskFromEmail}</span>
            )}
          </div>
          <div className="flex gap-2">
            <button
              type="button"
              className="inline-flex min-h-10 items-center justify-center rounded-lg bg-accent px-4 py-2 text-sm font-semibold text-on-accent transition-colors hover:bg-accent-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2"
              onClick={async () => {
                await client.acceptTask(t.id);
                onDone();
              }}
            >
              {strings.taskAccept}
            </button>
            <button
              type="button"
              className="inline-flex min-h-10 items-center justify-center rounded-lg bg-raised px-4 py-2 text-sm font-semibold text-primary transition-colors hover:bg-accent-tint hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2"
              onClick={async () => {
                await client.rejectTask(t.id);
                onDone();
              }}
            >
              {strings.taskReject}
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
