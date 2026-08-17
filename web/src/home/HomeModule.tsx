// The Home dashboard — the "here's your day" landing surface of the mail
// product. A greeting + global search, three live stat tiles (unread mail,
// this week's events, tasks due today), recent mail, today's calendar, the
// tasks on your plate, and the Ask-alo prompt. Every number and list is backed
// by a real call (mailboxes, calendar, tasks) — no placeholders for things the
// product doesn't have. Search and compose route into Mail, which owns them.
import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  ArrowRight,
  Bell,
  Calendar,
  CheckCircle2,
  Circle,
  Hand,
  Mail,
  PenLine,
  Search,
  Sparkles,
  Star,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { getLocale, strings } from "../i18n";
import { Spinner } from "../ds";
import { KEYWORD_FLAGGED, KEYWORD_SEEN, useJmapClient } from "../jmap";
import type { CalendarEvent, EmailHeaders, Task } from "../jmap";
import { useAuth } from "../auth";
import { formatDate, senderName, subjectOr } from "../mail/format";
import { surface } from "../product";
import { isModuleAllowed, useDeniedModules } from "../shell";
import { mostUsedApps } from "../shell/appUsage";
import styles from "./HomeModule.module.css";

type Tab = "recent" | "starred" | "unread";

function greeting(): string {
  const h = new Date().getHours();
  if (h < 12) return strings.homeGreetingMorning;
  if (h < 18) return strings.homeGreetingAfternoon;
  return strings.homeGreetingEvening;
}

function hm(iso: string): string {
  return new Date(iso).toLocaleTimeString(getLocale(), { hour: "2-digit", minute: "2-digit" });
}

function sameDay(iso: string, d: Date): boolean {
  const x = new Date(iso);
  return (
    x.getFullYear() === d.getFullYear() &&
    x.getMonth() === d.getMonth() &&
    x.getDate() === d.getDate()
  );
}

export function HomeModule() {
  const client = useJmapClient();
  const { identity } = useAuth();
  const navigate = useNavigate();
  const deniedModules = useDeniedModules();

  const [loading, setLoading] = useState(true);
  const [searchText, setSearchText] = useState("");
  const [unreadEmails, setUnreadEmails] = useState<number | null>(null);
  const [upcomingEvents, setUpcomingEvents] = useState<number | null>(null);
  const [dueToday, setDueToday] = useState<number | null>(null);
  const [recent, setRecent] = useState<EmailHeaders[]>([]);
  const [starred, setStarred] = useState<EmailHeaders[]>([]);
  const [todayEvents, setTodayEvents] = useState<CalendarEvent[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [tab, setTab] = useState<Tab>("recent");

  const load = useCallback(async () => {
    try {
      const now = new Date();
      const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
      const endOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 23, 59, 59);
      const weekAhead = new Date(now.getTime() + 7 * 24 * 60 * 60 * 1000);

      const boxes = await client.mailboxes();
      const inbox = boxes.find((b) => b.role === "inbox") ?? boxes[0];
      setUnreadEmails(inbox?.unreadEmails ?? 0);

      const [headers, flagged, plate, events] = await Promise.all([
        inbox ? client.emailHeaders(inbox.id, 8) : Promise.resolve([]),
        client.flaggedHeaders(8).catch(() => []),
        client.myPlate().catch(() => []),
        client.calendarEvents(startOfToday.toISOString(), weekAhead.toISOString()).catch(() => []),
      ]);

      setRecent(headers);
      setStarred(flagged);
      setTasks(plate);
      setDueToday(
        plate.filter((t) => t.dueAt !== null && new Date(t.dueAt) <= endOfToday).length,
      );
      setTodayEvents(
        events
          .filter((e) => sameDay(e.startsAt, now))
          .sort((a, b) => a.startsAt.localeCompare(b.startsAt)),
      );
      setUpcomingEvents(events.filter((e) => new Date(e.startsAt) >= now).length);
    } catch {
      // Best-effort: the dashboard degrades to empty states, never an error.
    } finally {
      setLoading(false);
    }
  }, [client]);

  useEffect(() => {
    void load();
  }, [load]);

  const firstName = useMemo(() => {
    const name = identity?.name?.trim();
    if (name !== undefined && name.length > 0) return name.split(/\s+/)[0] ?? name;
    return identity?.email?.split("@")[0] ?? "";
  }, [identity]);

  const unreadList = useMemo(
    () => recent.filter((e) => e.keywords[KEYWORD_SEEN] !== true),
    [recent],
  );
  const rows = tab === "recent" ? recent : tab === "starred" ? starred : unreadList;

  const tools = useMemo(() => {
    const available = surface.modules.filter(
      (module) =>
        module.id !== "home" &&
        module.enabled &&
        isModuleAllowed(deniedModules, module.id),
    );
    const byId = new Map(available.map((module) => [module.id, module]));
    const preferredDefaults = ["mail", "agenda", "tasks", "chat", "meet", "drive"];
    const orderedIds = [
      ...mostUsedApps(6),
      ...preferredDefaults,
      ...available.map((module) => module.id),
    ];

    return [...new Set(orderedIds)]
      .map((id) => byId.get(id))
      .filter((module): module is NonNullable<typeof module> => module !== undefined)
      .slice(0, 6);
  }, [deniedModules]);

  function runSearch(e: React.FormEvent) {
    e.preventDefault();
    const q = searchText.trim();
    if (q.length > 0) navigate(`/mail?q=${encodeURIComponent(q)}`);
  }

  async function completeTask(task: Task) {
    // Optimistically drop it, then persist; a failure reloads the true state.
    setTasks((prev) => prev.filter((t) => t.id !== task.id));
    try {
      await client.moveTask(task.id, "done", task.position);
      await load();
    } catch {
      await load();
    }
  }

  return (
    <div className={styles.home}>
      <header className={styles.head}>
        <div className={styles.headLeft}>
          <h1 className={styles.greeting}>
            <Hand className={styles.wave} size={26} aria-hidden />
            {greeting()}
            {firstName.length > 0 ? `, ${firstName}` : ""}
          </h1>
          <p className={styles.welcome}>{strings.homeSubtitle}</p>
        </div>

        <form className={styles.search} onSubmit={runSearch} role="search">
          <Search size={16} className={styles.searchIcon} aria-hidden />
          <input
            className={styles.searchInput}
            value={searchText}
            onChange={(e) => setSearchText(e.target.value)}
            placeholder={strings.homeSearchPlaceholder}
            aria-label={strings.homeSearchPlaceholder}
          />
        </form>

        <div className={styles.headRight}>
          <button
            type="button"
            className={styles.bell}
            onClick={() => navigate("/mail")}
            aria-label={strings.homeNotifications}
          >
            <Bell size={18} />
            {unreadEmails !== null && unreadEmails > 0 && (
              <span className={styles.bellBadge}>{unreadEmails > 9 ? "9+" : unreadEmails}</span>
            )}
          </button>
          <button
            type="button"
            className={styles.compose}
            onClick={() => navigate("/mail?compose=1")}
          >
            <PenLine size={17} />
            <span>{strings.homeCompose}</span>
          </button>
        </div>
      </header>

      <section className={styles.stats}>
        <StatCard
          Icon={Mail}
          tone="accent"
          value={unreadEmails}
          loading={loading}
          label={strings.homeStatUnreadEmails}
          cta={strings.homeGoToMail}
          onClick={() => navigate("/mail")}
        />
        <StatCard
          Icon={Calendar}
          tone="neutral"
          value={upcomingEvents}
          loading={loading}
          label={strings.homeStatEvents}
          cta={strings.homeViewCalendar}
          onClick={() => navigate("/agenda")}
        />
        <StatCard
          Icon={CheckCircle2}
          tone="success"
          value={dueToday}
          loading={loading}
          label={strings.homeStatTasks}
          cta={strings.homeViewTasks}
          onClick={() => navigate("/tasks")}
        />
      </section>

      {tools.length > 0 && (
        <section className={styles.tools} aria-labelledby="home-tools-title">
          <div className={styles.toolsHead}>
            <div>
              <h2 id="home-tools-title" className={styles.toolsTitle}>
                {strings.homeToolsTitle}
              </h2>
              <p className={styles.toolsSubtitle}>{strings.homeToolsSubtitle}</p>
            </div>
          </div>
          <div className={styles.toolsGrid}>
            {tools.map((tool) => (
              <button
                key={tool.id}
                type="button"
                className={styles.tool}
                onClick={() => navigate(tool.path)}
              >
                <span className={styles.toolIcon} aria-hidden>
                  <tool.Icon size={19} strokeWidth={1.8} />
                </span>
                <span className={styles.toolLabel}>{tool.label}</span>
                <ArrowRight className={styles.toolArrow} size={16} aria-hidden />
              </button>
            ))}
          </div>
        </section>
      )}

      <div className={styles.grid}>
        <section className={`${styles.card} ${styles.mailCard}`}>
          <div className={styles.cardHead}>
            <div className={styles.tabs} role="tablist">
              <Tabs tab={tab} onChange={setTab} />
            </div>
            <button type="button" className={styles.link} onClick={() => navigate("/mail")}>
              {strings.homeViewAll}
              <ArrowRight size={14} />
            </button>
          </div>
          {loading ? (
            <div className={styles.state}>
              <Spinner size={20} />
            </div>
          ) : rows.length === 0 ? (
            <EmptyState Icon={Mail} message={strings.homeNoRecent} action={strings.homeGoToMail} onAction={() => navigate("/mail")} />
          ) : (
            <ul className={styles.list}>
              {rows.slice(0, 6).map((e) => {
                const unread = e.keywords[KEYWORD_SEEN] !== true;
                const flagged = e.keywords[KEYWORD_FLAGGED] === true;
                return (
                  <li key={e.id}>
                    <button type="button" className={styles.row} onClick={() => navigate("/mail")}>
                      <span className={styles.rowIcon}>
                        {flagged ? <Star className={styles.star} size={16} /> : <Mail size={16} />}
                      </span>
                      <span className={styles.rowText}>
                        <span className={unread ? styles.subjectUnread : styles.subject}>
                          {subjectOr(e)}
                        </span>
                        <span className={styles.sender}>{senderName(e)}</span>
                      </span>
                      <span className={styles.rowTime}>
                        {formatDate(e.receivedAt)}
                        {unread && <span className={styles.dot} aria-hidden />}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </section>

        <aside className={styles.side}>
          <section className={`${styles.card} ${styles.calendarCard}`}>
            <div className={styles.cardHead}>
              <h2 className={styles.cardHeadTitle}>{strings.homeTodaysCalendar}</h2>
              <button type="button" className={styles.link} onClick={() => navigate("/agenda")}>
                {strings.homeViewFullCalendar}
                <ArrowRight size={14} />
              </button>
            </div>
            {loading ? (
              <div className={styles.state}>
                <Spinner size={18} />
              </div>
            ) : todayEvents.length === 0 ? (
              <EmptyState Icon={Calendar} message={strings.homeNoEventsToday} action={strings.homeViewCalendar} onAction={() => navigate("/agenda")} compact />
            ) : (
              <ul className={styles.agenda}>
                {todayEvents.slice(0, 4).map((e, i) => (
                  <li key={`${e.id}-${i}`} className={styles.agendaRow}>
                    <span className={styles.agendaTime}>
                      <span className={styles.agendaStart}>{e.allDay ? "—" : hm(e.startsAt)}</span>
                      {!e.allDay && <span className={styles.agendaEnd}>{hm(e.endsAt)}</span>}
                    </span>
                    <span className={styles.agendaBar} aria-hidden />
                    <span className={styles.agendaBody}>
                      <span className={styles.agendaSummary}>{subjectFor(e)}</span>
                      {e.location !== null && e.location.length > 0 && (
                        <span className={styles.agendaLocation}>{e.location}</span>
                      )}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section className={`${styles.card} ${styles.tasksCard}`}>
            <div className={styles.cardHead}>
              <h2 className={styles.cardHeadTitle}>{strings.homeMyTasks}</h2>
              <button type="button" className={styles.link} onClick={() => navigate("/tasks")}>
                {strings.homeViewAllTasks}
                <ArrowRight size={14} />
              </button>
            </div>
            {loading ? (
              <div className={styles.state}>
                <Spinner size={18} />
              </div>
            ) : tasks.length === 0 ? (
              <EmptyState Icon={CheckCircle2} message={strings.homeNoTasks} action={strings.homeViewTasks} onAction={() => navigate("/tasks")} compact />
            ) : (
              <ul className={styles.taskList}>
                {tasks.slice(0, 5).map((t) => (
                  <li key={t.id} className={styles.taskRow}>
                    <button
                      type="button"
                      className={styles.taskCheck}
                      onClick={() => void completeTask(t)}
                      aria-label={t.title}
                    >
                      <Circle className={styles.taskCircle} size={18} />
                      <CheckCircle2 className={styles.taskCircleDone} size={18} />
                    </button>
                    <button
                      type="button"
                      className={styles.taskTitle}
                      onClick={() => navigate("/tasks")}
                    >
                      {t.title}
                    </button>
                    <DueLabel dueAt={t.dueAt} />
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section className={`${styles.card} ${styles.ask}`}>
            <div className={styles.cardHead}>
              <div className={styles.askHeading}>
                <span className={styles.askMark}>
                  <Hand size={18} />
                </span>
                <h2 className={styles.cardHeadTitle}>{strings.homeAskTitle}</h2>
              </div>
              <button type="button" className={styles.link} onClick={() => navigate("/mail")}>
                <Sparkles size={14} />
                {strings.homeAskCta}
                <ArrowRight size={14} />
              </button>
            </div>
            <p className={styles.askBody}>{strings.homeAskBody}</p>
          </section>
        </aside>
      </div>
    </div>
  );
}

function subjectFor(e: CalendarEvent): string {
  return e.summary.trim().length > 0 ? e.summary : strings.agendaUntitledEvent;
}

function DueLabel({ dueAt }: { dueAt: string | null }) {
  if (dueAt === null) return null;
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const due = new Date(dueAt);
  if (due < startOfToday) {
    return <span className={`${styles.taskDue} ${styles.taskDueOverdue}`}>{strings.homeTaskOverdue}</span>;
  }
  if (sameDay(dueAt, now)) {
    return <span className={`${styles.taskDue} ${styles.taskDueToday}`}>{strings.homeTaskToday}</span>;
  }
  return <span className={styles.taskDue}>{formatDate(dueAt)}</span>;
}

interface StatCardProps {
  Icon: LucideIcon;
  label: string;
  cta: string;
  onClick: () => void;
  value?: number | null;
  loading?: boolean;
  tone: "accent" | "neutral" | "success";
}

function StatCard({ Icon, label, cta, onClick, value, loading, tone }: StatCardProps) {
  const iconClass =
    tone === "accent"
      ? `${styles.statIcon} ${styles.statIconAccent}`
      : tone === "success"
        ? `${styles.statIcon} ${styles.statIconSuccess}`
        : styles.statIcon;
  return (
    <button type="button" className={styles.stat} onClick={onClick}>
      <span className={iconClass}>
        <Icon size={20} />
      </span>
      <span className={styles.statValue}>{loading === true ? "—" : (value ?? 0)}</span>
      <span className={styles.statLabel}>{label}</span>
      <span className={styles.statCta}>
        {cta}
        <ArrowRight size={13} />
      </span>
    </button>
  );
}

function EmptyState({ Icon, message, action, onAction, compact = false }: { Icon: LucideIcon; message: string; action: string; onAction: () => void; compact?: boolean }) {
  return (
    <div className={compact ? `${styles.emptyState} ${styles.emptyStateCompact}` : styles.emptyState}>
      <span className={styles.emptyIcon}><Icon size={19} aria-hidden="true" /></span>
      <p>{message}</p>
      <button type="button" onClick={onAction}>{action}<ArrowRight size={13} aria-hidden="true" /></button>
    </div>
  );
}

function Tabs({ tab, onChange }: { tab: Tab; onChange: (t: Tab) => void }) {
  const items: { id: Tab; label: string }[] = [
    { id: "recent", label: strings.homeRecent },
    { id: "starred", label: strings.homeStarred },
    { id: "unread", label: strings.homeUnread },
  ];
  return (
    <>
      {items.map((it) => (
        <button
          key={it.id}
          type="button"
          role="tab"
          aria-selected={tab === it.id}
          className={tab === it.id ? `${styles.tab} ${styles.tabActive}` : styles.tab}
          onClick={() => onChange(it.id)}
        >
          {it.label}
        </button>
      ))}
    </>
  );
}
