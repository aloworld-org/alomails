// The Tasks module (ADR 0021–0023): a project sidebar, a main area that renders
// the SAME task rows as a board or a list (an instant, lossless toggle), a
// slide-in detail panel, plus "what's on my plate" and the AI proposals inbox
// (propose-then-approve). All data goes through the authenticated /tasks API;
// the board's grouping and the list's flattening are the client's job.
import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import {
  CalendarRange,
  ClipboardList,
  GanttChartSquare,
  LayoutDashboard,
  LayoutGrid,
  List,
  Paperclip,
  Plus,
  Search,
  Sparkles,
  Sun,
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
import { FilesView } from "./FilesView";
import { NewTaskModal } from "./NewTaskModal";
import { TaskDetail } from "./TaskDetail";
import { Avatar, DueChip, PriorityChip } from "./parts";
import { DEFAULT_CONFIG, filterTasks, type ViewConfig } from "./viewConfig";
import styles from "./TasksModule.module.css";

type View = "overview" | "list" | "board" | "timeline" | "calendar" | "files";

type Mode = { type: "project"; id: string } | { type: "plate" } | { type: "proposals" };

export function TasksModule() {
  const client = useJmapClient();
  const navigate = useNavigate();
  const { identity } = useAuth();
  const [projects, setProjects] = useState<TaskProject[]>([]);
  const [edges, setEdges] = useState<TaskDepEdgeDto[]>([]);
  const [mode, setMode] = useState<Mode>({ type: "plate" });
  const [view, setView] = useState<View>("overview");
  const [config, setConfig] = useState<ViewConfig>(DEFAULT_CONFIG);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [proposals, setProposals] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<string | null>(null);

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
    const requested = searchParams.get("project");
    if (requested === null || !projects.some((project) => project.id === requested)) return;
    setMode((current) =>
      current.type === "project" && current.id === requested
        ? current
        : { type: "project", id: requested },
    );
  }, [projects, searchParams]);

  useEffect(() => {
    const requested = searchParams.get("view");
    if (
      requested === "overview" || requested === "list" || requested === "board" ||
      requested === "timeline" || requested === "calendar" || requested === "files"
    ) {
      setView(requested);
    }
  }, [searchParams]);

  function openProject(id: string) {
    const next = new URLSearchParams(searchParams);
    next.set("project", id);
    next.set("view", "overview");
    setSearchParams(next);
    setMode({ type: "project", id });
    setView("overview");
  }

  function openView(nextView: View) {
    setView(nextView);
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
      // Land on the personal project's board by default.
      setMode((m) =>
        m.type === "plate" && ps[0] ? { type: "project", id: ps[0].id } : m,
      );
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

  const activeProject = useMemo(
    () => (mode.type === "project" ? projects.find((p) => p.id === mode.id) : undefined),
    [mode, projects],
  );

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
      ? strings.taskMyPlate
      : mode.type === "proposals"
        ? strings.taskProposals
        : (activeProject?.name ?? strings.moduleTasks);

  return (
    <div className={styles.tasks}>
      <aside className={styles.sidebar}>
        <button
          type="button"
          className={`${styles.plateBtn} ${mode.type === "plate" ? styles.projActive : ""}`}
          onClick={() => setMode({ type: "plate" })}
        >
          <Sun size={16} /> {strings.taskMyPlate}
        </button>
        <button
          type="button"
          className={`${styles.plateBtn} ${mode.type === "proposals" ? styles.projActive : ""}`}
          onClick={() => {
            void loadProposals();
            setMode({ type: "proposals" });
          }}
        >
          <Sparkles size={16} /> {strings.taskProposals}
          {proposals.length > 0 && <span className={styles.badge}>{proposals.length}</span>}
        </button>

        <div className={styles.projList}>
          <div className={styles.sideHead}>
            <span>{strings.taskProjects}</span>
            <button
              type="button"
              className={styles.iconBtn}
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
              className={`${styles.projItem} ${
                mode.type === "project" && mode.id === p.id ? styles.projActive : ""
              }`}
              onClick={() => openProject(p.id)}
            >
              <span
                className={styles.projDot}
                style={{ background: p.color ?? "var(--accent)" }}
                aria-hidden="true"
              />
              <span className={styles.projName}>{p.name}</span>
            </button>
          ))}
        </div>
      </aside>

      <section className={styles.main}>
        <header className={styles.topbar}>
          <h1 className={styles.pageTitle}>{title}</h1>
          <form
            className={styles.searchWrap}
            role="search"
            onSubmit={(e) => e.preventDefault()}
          >
            <Search size={16} className={styles.searchIcon} aria-hidden />
            <input
              className={styles.searchInput}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder={strings.taskSearchPlaceholder}
              aria-label={strings.taskSearchPlaceholder}
            />
          </form>
          <div className={styles.topActions}>
            {loading && <Spinner size={16} />}
            {tasks.length > 0 && mode.type !== "proposals" && (
              <button type="button" className={styles.newTaskBtn} onClick={() => openCreate()}>
                <Plus size={16} /> {strings.taskNew}
              </button>
            )}
          </div>
        </header>

        {mode.type !== "proposals" && (
          <div className={styles.tabs} role="tablist">
            {(
              [
                { id: "overview", label: strings.taskOverview, Icon: LayoutDashboard },
                { id: "list", label: strings.taskList, Icon: List },
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
                className={view === t.id ? `${styles.tab} ${styles.tabActive}` : styles.tab}
                onClick={() => openView(t.id)}
              >
                <t.Icon size={16} /> {t.label}
              </button>
            ))}
          </div>
        )}

        {mode.type !== "proposals" && view !== "overview" && view !== "files" && tasks.length > 0 && (
          <TaskToolbar config={config} onChange={setConfig} />
        )}

        <div className={styles.viewport}>
          {mode.type === "proposals" ? (
            <ProposalsInbox
              proposals={proposals}
              onDone={async () => {
                await loadProposals();
                await reload();
              }}
            />
          ) : tasks.length === 0 ? (
            <div className={styles.emptyState}>
              <span className={styles.emptyArt}>
                <ClipboardList size={40} />
              </span>
              <h2 className={styles.emptyTitle}>{strings.taskEmptyTitle}</h2>
              <p className={styles.emptyBody}>{strings.taskEmptyBody}</p>
              <button type="button" className={styles.emptyCta} onClick={() => openCreate()}>
                <Plus size={17} /> {strings.taskCreateFirst}
              </button>
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
      <div className={`${styles.emptyState} ${styles.proposalEmpty}`}>
        <span className={styles.emptyArt}>
          <Sparkles size={38} />
        </span>
        <h2 className={styles.emptyTitle}>{strings.taskNoProposalsTitle}</h2>
        <p className={styles.emptyBody}>{strings.taskNoProposals}</p>
      </div>
    );
  }
  return (
    <div style={{ padding: "var(--space-4)", maxWidth: 640 }}>
      {proposals.map((t) => (
        <div key={t.id} className={styles.proposal}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span className={styles.aiPill}>
              <Sparkles size={11} /> {strings.taskAiSuggested}
            </span>
            <strong style={{ fontSize: "var(--text-sm)" }}>{t.title}</strong>
          </div>
          <div className={styles.proposalMeta}>
            {t.assignee && <Avatar email={t.assignee} />}
            {t.assignee && <span>{t.assignee}</span>}
            {t.dueAt && <DueChip iso={t.dueAt} done={false} />}
            <PriorityChip priority={t.priority} />
            {t.sourceKind && (
              <span>· {t.sourceKind === "event" ? strings.taskFromEvent : strings.taskFromEmail}</span>
            )}
          </div>
          <div className={styles.proposalActions}>
            <button
              type="button"
              className={styles.accept}
              onClick={async () => {
                await client.acceptTask(t.id);
                onDone();
              }}
            >
              {strings.taskAccept}
            </button>
            <button
              type="button"
              className={styles.reject}
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
