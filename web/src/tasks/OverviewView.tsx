// The project Overview: a dashboard computed entirely from the loaded tasks —
// counts by status, a progress donut, tasks per assignee, and what's coming up.
// No invented data; every number is a real count. (A project-wide activity feed
// needs a backend endpoint and is not shown here yet.)
import { useMemo } from "react";
import { CheckCircle2, Circle, Clock, ListTodo, Plus } from "lucide-react";

import { strings } from "../i18n";
import type { Task } from "../jmap";
import { Avatar, COLUMNS, dueLabel, statusColor } from "./parts";
import styles from "./TasksModule.module.css";

interface Props {
  tasks: Task[];
  me?: string | undefined;
  onOpen: (id: string) => void;
  onAdd: () => void;
  onViewAll: () => void;
}

const ASSIGNEE_COLORS = ["#e76f51", "#4b83c4", "#9b6dd6", "#2e8b57", "#e0a63b", "#d1568f", "#3aa8a0"];

function pct(n: number, total: number): number {
  return total === 0 ? 0 : Math.round((n / total) * 100);
}

/** An SVG donut of the status breakdown, with the completion % in the centre. */
function Donut({ segments, total, done }: { segments: { count: number; color: string }[]; total: number; done: number }) {
  const r = 54;
  const c = 2 * Math.PI * r;
  let offset = 0;
  return (
    <svg className={styles.donut} viewBox="0 0 140 140" role="img" aria-label={`${pct(done, total)}% completed`}>
      <circle cx="70" cy="70" r={r} fill="none" stroke="var(--bg-raised)" strokeWidth="16" />
      {segments.map((s, i) => {
        const len = total === 0 ? 0 : (s.count / total) * c;
        const dash = (
          <circle
            key={i}
            cx="70"
            cy="70"
            r={r}
            fill="none"
            stroke={s.color}
            strokeWidth="16"
            strokeDasharray={`${len} ${c - len}`}
            strokeDashoffset={-offset}
            transform="rotate(-90 70 70)"
            strokeLinecap="butt"
          />
        );
        offset += len;
        return dash;
      })}
      <text x="70" y="66" className={styles.donutPct}>
        {pct(done, total)}%
      </text>
      <text x="70" y="86" className={styles.donutSub}>
        {strings.taskOvCompletedLabel}
      </text>
    </svg>
  );
}

export function OverviewView({ tasks, me, onOpen, onAdd, onViewAll }: Props) {
  const stats = useMemo(() => {
    const by = (s: string) => tasks.filter((t) => t.status === s).length;
    return {
      total: tasks.length,
      done: by("done"),
      inProgress: by("in_progress"),
      todo: by("todo"),
      review: by("review"),
    };
  }, [tasks]);

  const byAssignee = useMemo(() => {
    const map = new Map<string, number>();
    for (const t of tasks) {
      const key = t.assignee ?? "";
      map.set(key, (map.get(key) ?? 0) + 1);
    }
    return [...map.entries()]
      .map(([email, count]) => ({ email, count }))
      .sort((a, b) => b.count - a.count)
      .slice(0, 6);
  }, [tasks]);
  const maxAssignee = Math.max(1, ...byAssignee.map((a) => a.count));

  const upcoming = useMemo(() => {
    const start = new Date();
    start.setHours(0, 0, 0, 0);
    return tasks
      .filter((t) => t.status !== "done" && t.dueAt !== null && new Date(t.dueAt) >= start)
      .sort((a, b) => (a.dueAt as string).localeCompare(b.dueAt as string))
      .slice(0, 4);
  }, [tasks]);

  const segments = [
    { count: stats.done, color: statusColor("done") },
    { count: stats.inProgress, color: statusColor("in_progress") },
    { count: stats.review, color: statusColor("review") },
    { count: stats.todo, color: statusColor("todo") },
  ];

  return (
    <div className={styles.overview}>
      <section className={styles.ovStats}>
        <StatTile Icon={ListTodo} tone="accent" label={strings.taskOvTotal} value={stats.total} sub={strings.moduleTasks} />
        <StatTile Icon={CheckCircle2} tone="done" label={strings.taskOvCompleted} value={stats.done} sub={`${pct(stats.done, stats.total)}%`} />
        <StatTile Icon={Clock} tone="prog" label={strings.taskColInProgress} value={stats.inProgress} sub={`${pct(stats.inProgress, stats.total)}%`} />
        <StatTile Icon={Circle} tone="todo" label={strings.taskColTodo} value={stats.todo} sub={`${pct(stats.todo, stats.total)}%`} />
      </section>

      <div className={styles.ovGrid}>
        <section className={styles.ovCard}>
          <h2 className={styles.ovCardTitle}>{strings.taskOvProgress}</h2>
          <div className={styles.ovProgress}>
            <Donut segments={segments} total={stats.total} done={stats.done} />
            <div className={styles.ovLegend}>
              {COLUMNS.map((c) => {
                const v =
                  c.key === "in_progress"
                    ? stats.inProgress
                    : c.key === "done"
                      ? stats.done
                      : c.key === "review"
                        ? stats.review
                        : stats.todo;
                if (c.key === "review" && v === 0) return null;
                return (
                  <div key={c.key} className={styles.ovLegendRow}>
                    <span className={styles.ovLegendDot} style={{ background: statusColor(c.key) }} aria-hidden />
                    <span className={styles.ovLegendLabel}>{c.label()}</span>
                    <span className={styles.ovLegendVal}>{v}</span>
                  </div>
                );
              })}
              <div className={styles.ovLegendTotal}>{strings.taskOvTasksTotal(stats.total)}</div>
            </div>
          </div>
        </section>

        <section className={styles.ovCard}>
          <h2 className={styles.ovCardTitle}>{strings.taskOvByAssignee}</h2>
          <div className={styles.ovAssignees}>
            {byAssignee.length === 0 ? (
              <p className={styles.ovEmpty}>{strings.taskEmpty}</p>
            ) : (
              byAssignee.map((a, i) => (
                <div key={a.email || "none"} className={styles.ovAssignee}>
                  {a.email !== "" ? <Avatar email={a.email} /> : <span className={styles.ovNoAvatar} />}
                  <span className={styles.ovAssigneeName}>
                    {a.email === "" ? strings.taskOvNobody : nameOf(a.email, me)}
                  </span>
                  <span className={styles.ovBarTrack}>
                    <span
                      className={styles.ovBar}
                      style={{
                        width: `${(a.count / maxAssignee) * 100}%`,
                        background: ASSIGNEE_COLORS[i % ASSIGNEE_COLORS.length],
                      }}
                    />
                  </span>
                  <span className={styles.ovAssigneeCount}>{a.count}</span>
                </div>
              ))
            )}
          </div>
        </section>

        <section className={styles.ovCard}>
          <div className={styles.ovCardHead}>
            <h2 className={styles.ovCardTitle}>{strings.taskOvUpcoming}</h2>
            <button type="button" className={styles.ovViewAll} onClick={onViewAll}>
              {strings.taskOvViewAll}
            </button>
          </div>
          {upcoming.length === 0 ? (
            <p className={styles.ovEmpty}>{strings.taskPlateEmpty}</p>
          ) : (
            <ul className={styles.ovUpcoming}>
              {upcoming.map((t) => (
                <li key={t.id}>
                  <button type="button" className={styles.ovUpItem} onClick={() => onOpen(t.id)}>
                    <Circle size={16} className={styles.ovUpCheck} />
                    <span className={styles.ovUpTitle}>{t.title}</span>
                    <span className={styles.ovUpDue}>{dueLabel(t.dueAt as string)}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <button type="button" className={styles.ovAddTask} onClick={onAdd}>
            <Plus size={15} /> {strings.taskAdd}
          </button>
        </section>
      </div>
    </div>
  );
}

function nameOf(email: string, me?: string): string {
  if (me !== undefined && email.toLowerCase() === me.toLowerCase()) return strings.taskAssigneeYou;
  const local = email.split("@")[0] ?? email;
  const first = local.split(/[.\-_]+/)[0] ?? local;
  return first.charAt(0).toUpperCase() + first.slice(1);
}

interface TileProps {
  Icon: typeof ListTodo;
  tone: "accent" | "done" | "prog" | "todo";
  label: string;
  value: number;
  sub: string;
}

function StatTile({ Icon, tone, label, value, sub }: TileProps) {
  const iconCls =
    tone === "done"
      ? styles.ovTileIconDone
      : tone === "prog"
        ? styles.ovTileIconProg
        : tone === "todo"
          ? styles.ovTileIconTodo
          : styles.ovTileIconAccent;
  return (
    <div className={styles.ovTile}>
      <span className={`${styles.ovTileIcon} ${iconCls}`}>
        <Icon size={18} />
      </span>
      <span className={styles.ovTileLabel}>{label}</span>
      <span className={styles.ovTileValue}>{value}</span>
      <span className={styles.ovTileSub}>{sub}</span>
    </div>
  );
}
