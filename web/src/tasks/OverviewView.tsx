// The project Overview: a dashboard computed entirely from the loaded tasks —
// counts by status, a progress donut, tasks per assignee, and what's coming up.
// No invented data; every number is a real count. (A project-wide activity feed
// needs a backend endpoint and is not shown here yet.)
import { useMemo } from "react";
import { CheckCircle2, Circle, Clock, ListTodo, Plus } from "lucide-react";

import { strings } from "../i18n";
import type { Task } from "../jmap";
import { Avatar, COLUMNS, dueLabel, statusColor } from "./parts";

interface Props {
  tasks: Task[];
  me?: string | undefined;
  onOpen: (id: string) => void;
  onAdd: () => void;
  onViewAll: () => void;
}

const ASSIGNEE_COLORS = ["#e76f51", "#c95c43", "#2e8b57", "#e0a63b", "#d97757", "#7b6f66"];

function pct(n: number, total: number): number {
  return total === 0 ? 0 : Math.round((n / total) * 100);
}

/** An SVG donut of the status breakdown, with the completion % in the centre. */
function Donut({ segments, total, done }: { segments: { count: number; color: string }[]; total: number; done: number }) {
  const r = 54;
  const c = 2 * Math.PI * r;
  let offset = 0;
  return (
    <svg className="size-[8.75rem] shrink-0" viewBox="0 0 140 140" role="img" aria-label={`${pct(done, total)}% completed`}>
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
      <text x="70" y="66" className="fill-primary text-[26px] font-bold [text-anchor:middle]">
        {pct(done, total)}%
      </text>
      <text x="70" y="86" className="fill-tertiary text-[11px] [text-anchor:middle]">
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
    <div className="flex flex-col gap-5 overflow-y-auto px-6 py-5 max-sm:px-4">
      <section className="grid grid-cols-[repeat(auto-fit,minmax(12rem,1fr))] gap-4">
        <StatTile Icon={ListTodo} tone="accent" label={strings.taskOvTotal} value={stats.total} sub={strings.moduleTasks} />
        <StatTile Icon={CheckCircle2} tone="done" label={strings.taskOvCompleted} value={stats.done} sub={`${pct(stats.done, stats.total)}%`} />
        <StatTile Icon={Clock} tone="prog" label={strings.taskColInProgress} value={stats.inProgress} sub={`${pct(stats.inProgress, stats.total)}%`} />
        <StatTile Icon={Circle} tone="todo" label={strings.taskColTodo} value={stats.todo} sub={`${pct(stats.todo, stats.total)}%`} />
      </section>

      <div className="grid grid-cols-2 items-start gap-5 max-[860px]:grid-cols-1">
        <section className="rounded-xl border border-subtle bg-surface p-5 shadow-sm">
          <h2 className="mb-4 text-base font-semibold text-primary">{strings.taskOvProgress}</h2>
          <div className="flex items-center gap-6 max-sm:flex-col max-sm:items-stretch">
            <Donut segments={segments} total={stats.total} done={stats.done} />
            <div className="flex flex-1 flex-col gap-2">
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
                  <div key={c.key} className="flex items-center gap-2">
                    <span className="size-2.5 shrink-0 rounded-full" style={{ background: statusColor(c.key) }} aria-hidden />
                    <span className="flex-1 text-sm text-secondary">{c.label()}</span>
                    <span className="text-sm font-semibold tabular-nums text-primary">{v}</span>
                  </div>
                );
              })}
              <div className="mt-2 text-xs text-tertiary">{strings.taskOvTasksTotal(stats.total)}</div>
            </div>
          </div>
        </section>

        <section className="rounded-xl border border-subtle bg-surface p-5 shadow-sm">
          <h2 className="mb-4 text-base font-semibold text-primary">{strings.taskOvByAssignee}</h2>
          <div className="flex flex-col gap-3">
            {byAssignee.length === 0 ? (
              <p className="text-sm text-tertiary">{strings.taskEmpty}</p>
            ) : (
              byAssignee.map((a, i) => (
                <div key={a.email || "none"} className="flex items-center gap-3">
                  {a.email !== "" ? <Avatar email={a.email} /> : <span className="size-[26px] shrink-0 rounded-full bg-raised" />}
                  <span className="w-[5.625rem] shrink-0 truncate text-sm text-primary">
                    {a.email === "" ? strings.taskOvNobody : nameOf(a.email, me)}
                  </span>
                  <span className="h-2 flex-1 overflow-hidden rounded-full bg-raised">
                    <span
                      className="block h-full rounded-full"
                      style={{
                        width: `${(a.count / maxAssignee) * 100}%`,
                        background: ASSIGNEE_COLORS[i % ASSIGNEE_COLORS.length],
                      }}
                    />
                  </span>
                  <span className="w-5 text-right text-sm font-semibold tabular-nums text-primary">{a.count}</span>
                </div>
              ))
            )}
          </div>
        </section>

        <section className="rounded-xl border border-subtle bg-surface p-5 shadow-sm">
          <div className="mb-3 flex items-center justify-between gap-4">
            <h2 className="text-base font-semibold text-primary">{strings.taskOvUpcoming}</h2>
            <button type="button" className="rounded-md px-2 py-1 text-sm font-medium text-link !no-underline hover:bg-raised hover:text-primary hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent" onClick={onViewAll}>
              {strings.taskOvViewAll}
            </button>
          </div>
          {upcoming.length === 0 ? (
            <p className="text-sm text-tertiary">{strings.taskPlateEmpty}</p>
          ) : (
            <ul className="flex flex-col">
              {upcoming.map((t) => (
                <li key={t.id}>
                  <button type="button" className="flex min-h-11 w-full items-center gap-3 border-t border-subtle px-1 py-3 text-left !no-underline first:border-t-0 hover:bg-raised hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent" onClick={() => onOpen(t.id)}>
                    <Circle size={16} className="shrink-0 text-tertiary" />
                    <span className="min-w-0 flex-1 truncate text-sm text-primary">{t.title}</span>
                    <span className="shrink-0 text-xs tabular-nums text-tertiary">{dueLabel(t.dueAt as string)}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <button type="button" className="mt-3 inline-flex min-h-9 items-center gap-2 rounded-lg bg-raised px-3 py-2 text-sm font-medium text-primary !no-underline hover:bg-selected hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent" onClick={onAdd}>
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
      ? "bg-success/10 text-success"
      : tone === "prog"
        ? "bg-accent-tint text-accent-hover"
        : tone === "todo"
          ? "bg-raised text-secondary"
          : "bg-accent-tint text-accent-hover";
  return (
    <div className="flex flex-col items-start gap-1 rounded-xl border border-subtle bg-surface px-5 py-4 shadow-sm">
      <span className={`mb-1 inline-flex size-[38px] items-center justify-center rounded-full ${iconCls}`}>
        <Icon size={18} />
      </span>
      <span className="text-sm text-secondary">{label}</span>
      <span className="text-3xl font-bold leading-none tabular-nums text-primary">{value}</span>
      <span className="text-xs text-tertiary">{sub}</span>
    </div>
  );
}
