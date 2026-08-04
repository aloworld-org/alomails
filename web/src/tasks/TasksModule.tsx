// The Tasks module (ADR 0021–0023): a project sidebar, a main area that renders
// the SAME task rows as a board or a list (an instant, lossless toggle), a
// slide-in detail panel, plus "what's on my plate" and the AI proposals inbox
// (propose-then-approve). All data goes through the authenticated /tasks API;
// the board's grouping and the list's flattening are the client's job.
import { useCallback, useEffect, useMemo, useState } from "react";
import { LayoutGrid, List, Plus, Sparkles, Sun } from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient, type Task, type TaskProject } from "../jmap";
import { Spinner } from "../ds";
import { BoardView } from "./BoardView";
import { ListView } from "./ListView";
import { TaskDetail } from "./TaskDetail";
import { Avatar, DueChip, PriorityChip } from "./parts";
import styles from "./TasksModule.module.css";

type Mode = { type: "project"; id: string } | { type: "plate" } | { type: "proposals" };

export function TasksModule() {
  const client = useJmapClient();
  const [projects, setProjects] = useState<TaskProject[]>([]);
  const [mode, setMode] = useState<Mode>({ type: "plate" });
  const [view, setView] = useState<"board" | "list">("board");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [proposals, setProposals] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<string | null>(null);
  const [quick, setQuick] = useState("");

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
      if (mode.type === "project") setTasks(await client.tasks(mode.id));
      else if (mode.type === "plate") setTasks(await client.myPlate());
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

  async function addQuick() {
    const title = quick.trim();
    if (title === "" || mode.type !== "project") return;
    setQuick("");
    await client.createTask({ projectId: mode.id, title });
    await reload();
  }

  async function newProject() {
    const name = window.prompt(strings.taskNewProjectPrompt)?.trim();
    if (!name) return;
    const p = await client.createTaskProject(name);
    await loadProjects();
    setMode({ type: "project", id: p.id });
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
              onClick={() => void newProject()}
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
              onClick={() => setMode({ type: "project", id: p.id })}
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
        <header className={styles.toolbar}>
          <h1 className={styles.title}>{title}</h1>
          {loading && <Spinner size={16} />}
          {mode.type !== "proposals" && (
            <div className={styles.viewSwitch}>
              <button
                className={view === "board" ? styles.viewActive : ""}
                onClick={() => setView("board")}
              >
                <LayoutGrid size={15} /> {strings.taskBoard}
              </button>
              <button
                className={view === "list" ? styles.viewActive : ""}
                onClick={() => setView("list")}
              >
                <List size={15} /> {strings.taskList}
              </button>
            </div>
          )}
        </header>

        {mode.type === "project" && (
          <div className={styles.quickAdd}>
            <input
              value={quick}
              placeholder={strings.taskQuickAdd}
              onChange={(e) => setQuick(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void addQuick();
              }}
            />
            <button type="button" className={styles.addBtn} onClick={() => void addQuick()}>
              <Plus size={15} /> {strings.taskAdd}
            </button>
          </div>
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
            <div className={styles.empty}>
              {mode.type === "plate" ? strings.taskPlateEmpty : strings.taskEmpty}
            </div>
          ) : view === "board" && mode.type === "project" ? (
            <BoardView tasks={tasks} onOpen={setSelected} onMove={move} />
          ) : (
            <ListView tasks={tasks} onOpen={setSelected} onMove={move} />
          )}
        </div>
      </section>

      {selected !== null && (
        <TaskDetail
          taskId={selected}
          onClose={() => setSelected(null)}
          onChanged={() => {
            void reload();
            void loadProposals();
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
    return <div className={styles.empty}>{strings.taskNoProposals}</div>;
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
