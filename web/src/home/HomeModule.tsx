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
  MessageCircle,
  PenLine,
  Send,
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
import { useChatApi } from "../chat/api";

type Tab = "recent" | "starred" | "unread";

const cardClass = "rounded-xl border border-subtle bg-surface p-5 shadow-sm";
const cardHeadClass = "mb-3 flex items-center justify-between gap-3";
const cardTitleClass = "m-0 text-md font-semibold text-primary";
const linkClass = "inline-flex min-h-8 shrink-0 items-center gap-1 rounded-md px-2 text-sm font-medium text-secondary transition-colors hover:bg-raised hover:text-accent focus-visible:outline-2 focus-visible:outline-accent";

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
  const chatApi = useChatApi();

  const [loading, setLoading] = useState(true);
  const [searchText, setSearchText] = useState("");
  const [unreadEmails, setUnreadEmails] = useState<number | null>(null);
  const [upcomingEvents, setUpcomingEvents] = useState<number | null>(null);
  const [dueToday, setDueToday] = useState<number | null>(null);
  const [unreadMessages, setUnreadMessages] = useState<number | null>(null);
  const [recent, setRecent] = useState<EmailHeaders[]>([]);
  const [starred, setStarred] = useState<EmailHeaders[]>([]);
  const [todayEvents, setTodayEvents] = useState<CalendarEvent[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [tab, setTab] = useState<Tab>("recent");
  const [askText, setAskText] = useState("");
  const [askReply, setAskReply] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);

  const load = useCallback(async () => {
    try {
      const now = new Date();
      const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
      const endOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 23, 59, 59);
      const weekAhead = new Date(now.getTime() + 7 * 24 * 60 * 60 * 1000);

      const boxes = await client.mailboxes();
      const inbox = boxes.find((b) => b.role === "inbox") ?? boxes[0];
      setUnreadEmails(inbox?.unreadEmails ?? 0);

      const [headers, flagged, plate, events, rooms] = await Promise.all([
        inbox ? client.emailHeaders(inbox.id, 8) : Promise.resolve([]),
        client.flaggedHeaders(8).catch(() => []),
        client.myPlate().catch(() => []),
        client.calendarEvents(startOfToday.toISOString(), weekAhead.toISOString()).catch(() => []),
        chatApi.channels().catch(() => []),
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
      setUnreadMessages(rooms.reduce((total, room) => total + room.unread, 0));
    } catch {
      // Best-effort: the dashboard degrades to empty states, never an error.
    } finally {
      setLoading(false);
    }
  }, [chatApi, client]);

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
      ...mostUsedApps(8),
      ...preferredDefaults,
      ...available.map((module) => module.id),
    ];

    return [...new Set(orderedIds)]
      .map((id) => byId.get(id))
      .filter((module): module is NonNullable<typeof module> => module !== undefined)
      .slice(0, 8);
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

  async function askAlo(e: React.FormEvent) {
    e.preventDefault();
    const query = askText.trim();
    if (query.length === 0 || asking) return;
    setAsking(true);
    setAskReply(null);
    try {
      const reply = await client.askAgent(query);
      setAskReply(
        reply.answer ?? reply.action?.say ?? strings.homeAskUnavailable,
      );
    } catch {
      setAskReply(strings.homeAskUnavailable);
    } finally {
      setAsking(false);
    }
  }

  return (
    <div className="mx-auto flex h-full max-w-[1400px] flex-col gap-5 overflow-y-auto px-8 pb-8 pt-7 max-sm:p-4">
      <header className="grid grid-cols-[minmax(260px,auto)_minmax(280px,520px)_auto] items-center gap-5 max-lg:grid-cols-[1fr_auto] max-sm:grid-cols-1">
        <div className="shrink-0">
          <h1 className="m-0 flex items-center gap-3 text-3xl font-bold tracking-[-0.01em] text-primary max-sm:text-2xl">
            <Hand className="text-accent" size={26} aria-hidden />
            {greeting()}
            {firstName.length > 0 ? `, ${firstName}` : ""}
          </h1>
          <p className="mb-0 mt-1 text-md text-secondary">{strings.homeSubtitle}</p>
        </div>

        <form className="m-0 flex h-[42px] w-full max-w-[520px] flex-1 items-center gap-2 rounded-full border border-default bg-surface px-3 transition focus-within:border-accent focus-within:ring-3 focus-within:ring-[color-mix(in_srgb,var(--accent)_13%,transparent)] max-lg:order-3 max-lg:col-span-full max-lg:max-w-none max-sm:order-3" onSubmit={runSearch} role="search">
          <Search size={16} className="shrink-0 text-tertiary" aria-hidden />
          <input
            className="min-w-0 flex-1 border-0 bg-transparent text-base text-primary outline-none"
            value={searchText}
            onChange={(e) => setSearchText(e.target.value)}
            placeholder={strings.homeSearchPlaceholder}
            aria-label={strings.homeSearchPlaceholder}
          />
          <kbd className="hidden rounded-md bg-raised px-2 py-1 font-ui text-[11px] font-medium text-tertiary sm:inline-flex">Ctrl K</kbd>
        </form>

        <div className="flex shrink-0 items-center gap-3 max-sm:order-2">
          <button
            type="button"
            className="relative inline-flex size-[42px] items-center justify-center rounded-full border border-subtle bg-surface text-secondary transition-colors hover:bg-raised hover:text-primary"
            onClick={() => navigate("/mail")}
            aria-label={strings.homeNotifications}
          >
            <Bell size={18} />
            {unreadEmails !== null && unreadEmails > 0 && (
              <span className="absolute -right-[3px] -top-[3px] inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-full border-2 border-app bg-accent px-1 text-[11px] font-bold tabular-nums text-on-accent">{unreadEmails > 9 ? "9+" : unreadEmails}</span>
            )}
          </button>
          <button
            type="button"
            className="inline-flex h-[42px] shrink-0 items-center gap-2 rounded-md !bg-accent px-4 text-base font-semibold !text-on-accent shadow-sm transition-colors hover:!bg--hover"
            onClick={() => navigate("/mail?compose=1")}
          >
            <PenLine size={17} />
            <span>{strings.homeCompose}</span>
          </button>
        </div>
      </header>

      <section className="grid grid-cols-[repeat(auto-fit,minmax(200px,1fr))] gap-3 max-lg:grid-cols-2 max-sm:grid-cols-1">
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
        <StatCard
          Icon={MessageCircle}
          tone="message"
          value={unreadMessages}
          loading={loading}
          label={strings.homeStatMessages}
          cta={strings.moduleChat}
          onClick={() => navigate("/chat")}
        />
      </section>

      {tools.length > 0 && (
        <section className="rounded-xl border border-subtle bg-surface p-5 shadow-sm" aria-labelledby="home-tools-title">
          <div className="mb-4 flex items-start justify-between">
            <div>
              <h2 id="home-tools-title" className="m-0 text-md font-semibold text-primary">
                {strings.homeToolsTitle}
              </h2>
              <p className="mb-0 mt-0.5 text-xs text-tertiary">{strings.homeToolsSubtitle}</p>
            </div>
          </div>
          <div className="grid grid-cols-8 gap-3 max-xl:grid-cols-4 max-sm:grid-cols-2">
            {tools.map((tool, index) => (
              <button
                key={tool.id}
                type="button"
                className="group grid min-h-[54px] min-w-0 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 rounded-lg border border-transparent bg-transparent px-2 py-2 text-left text-primary transition hover:bg-raised focus-visible:outline-2 focus-visible:outline-accent"
                onClick={() => navigate(tool.path)}
              >
                <span className={`inline-flex size-9 items-center justify-center rounded-lg shadow-sm ${index % 3 === 1 ? "bg-[var(--success-bg)] text-[var(--success-text)]" : index % 3 === 2 ? "bg-[var(--accent-secondary-tint)] text-[var(--accent-secondary)]" : "bg-[var(--accent-soft)] text-accent"}`} aria-hidden>
                  <tool.Icon size={19} strokeWidth={1.8} />
                </span>
                <span className="min-w-0 truncate whitespace-nowrap text-sm font-semibold">{tool.label}</span>
                <ArrowRight className="-translate-x-[3px] text-tertiary opacity-0 transition group-hover:translate-x-0 group-hover:opacity-100 group-focus-visible:translate-x-0 group-focus-visible:opacity-100" size={16} aria-hidden />
              </button>
            ))}
          </div>
        </section>
      )}

      <div className="grid grid-cols-[minmax(0,2fr)_minmax(320px,1fr)] [grid-template-areas:'mail_calendar'_'ask_tasks'] items-stretch gap-4 max-lg:grid-cols-1 max-lg:[grid-template-areas:'mail'_'calendar'_'tasks'_'ask']">
        <section className={`${cardClass} [grid-area:mail]`}>
          <div className={cardHeadClass}>
            <div className="flex items-center gap-2" role="tablist">
              <Tabs tab={tab} onChange={setTab} />
            </div>
            <button type="button" className={linkClass} onClick={() => navigate("/mail")}>
              {strings.homeViewAll}
              <ArrowRight size={14} />
            </button>
          </div>
          {loading ? (
            <div className="p-8 text-center text-sm text-tertiary">
              <Spinner size={20} />
            </div>
          ) : rows.length === 0 ? (
            <EmptyState Icon={Mail} title={strings.homeMailClearTitle} message={strings.homeNoRecent} action={strings.homeGoToMail} onAction={() => navigate("/mail")} />
          ) : (
            <ul className="m-0 list-none p-0">
              {rows.slice(0, 6).map((e) => {
                const unread = e.keywords[KEYWORD_SEEN] !== true;
                const flagged = e.keywords[KEYWORD_FLAGGED] === true;
                return (
                  <li key={e.id}>
                    <button type="button" className="flex min-h-14 w-full items-center gap-3 border-b border-subtle px-2 py-2 text-left transition-colors last:border-b-0 hover:bg-raised" onClick={() => navigate("/mail")}>
                      <span className="inline-flex size-8 shrink-0 items-center justify-center rounded-md bg-app text-secondary">
                        {flagged ? <Star className="fill-[var(--warning)] text-[var(--warning)]" size={16} /> : <Mail size={16} />}
                      </span>
                      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                        <span className={unread ? "truncate text-sm font-semibold text-primary" : "truncate text-sm font-medium text-primary"}>
                          {subjectOr(e)}
                        </span>
                        <span className="truncate text-xs text-tertiary">{senderName(e)}</span>
                      </span>
                      <span className="inline-flex shrink-0 items-center gap-2 text-xs text-tertiary">
                        {formatDate(e.receivedAt)}
                        {unread && <span className="size-[7px] rounded-full bg-[var(--unread)]" aria-hidden />}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </section>

        <aside className="contents">
          <section className={`${cardClass} [grid-area:calendar]`}>
            <div className={cardHeadClass}>
              <h2 className={cardTitleClass}>{strings.homeTodaysCalendar}</h2>
              <button type="button" className={linkClass} onClick={() => navigate("/agenda")}>
                {strings.homeViewFullCalendar}
                <ArrowRight size={14} />
              </button>
            </div>
            {loading ? (
              <div className="p-8 text-center text-sm text-tertiary">
                <Spinner size={18} />
              </div>
            ) : todayEvents.length === 0 ? (
              <EmptyState Icon={Calendar} title={strings.homeCalendarClearTitle} message={strings.homeNoEventsToday} action={strings.homeViewCalendar} onAction={() => navigate("/agenda")} compact />
            ) : (
              <ul className="m-0 flex list-none flex-col gap-2 p-0">
                {todayEvents.slice(0, 4).map((e, i) => (
                  <li key={`${e.id}-${i}`} className="grid min-h-12 grid-cols-[66px_3px_minmax(0,1fr)] items-center gap-3 rounded-md px-2 py-1 transition-colors hover:bg-raised">
                    <span className="flex flex-col text-xs tabular-nums text-tertiary">
                      <span className="font-medium text-secondary">{e.allDay ? "—" : hm(e.startsAt)}</span>
                      {!e.allDay && <span>{hm(e.endsAt)}</span>}
                    </span>
                    <span className="h-8 w-[3px] rounded-full bg-accent" aria-hidden />
                    <span className="flex min-w-0 flex-col gap-0.5">
                      <span className="truncate text-sm font-medium text-primary">{subjectFor(e)}</span>
                      {e.location !== null && e.location.length > 0 && (
                        <span className="truncate text-xs text-tertiary">{e.location}</span>
                      )}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section className={`${cardClass} [grid-area:tasks]`}>
            <div className={cardHeadClass}>
              <h2 className={cardTitleClass}>{strings.homeMyTasks}</h2>
              <button type="button" className={linkClass} onClick={() => navigate("/tasks")}>
                {strings.homeViewAllTasks}
                <ArrowRight size={14} />
              </button>
            </div>
            {loading ? (
              <div className="p-8 text-center text-sm text-tertiary">
                <Spinner size={18} />
              </div>
            ) : tasks.length === 0 ? (
              <EmptyState Icon={CheckCircle2} title={strings.homeTasksClearTitle} message={strings.homeNoTasks} action={strings.homeViewTasks} onAction={() => navigate("/tasks")} compact />
            ) : (
              <ul className="m-0 flex list-none flex-col gap-1 p-0">
                {tasks.slice(0, 5).map((t) => (
                  <li key={t.id} className="group flex min-h-11 items-center gap-2 rounded-md px-2 transition-colors hover:bg-raised">
                    <button
                      type="button"
                      className="relative inline-flex size-8 shrink-0 items-center justify-center rounded-full text-tertiary hover:text-accent"
                      onClick={() => void completeTask(t)}
                      aria-label={t.title}
                    >
                      <Circle className="block group-hover:hidden" size={18} />
                      <CheckCircle2 className="hidden text-accent group-hover:block" size={18} />
                    </button>
                    <button
                      type="button"
                      className="min-w-0 flex-1 truncate text-left text-sm font-medium text-primary"
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

          <section className={`${cardClass} flex min-h-[168px] flex-col [grid-area:ask]`}>
            <div className={cardHeadClass}>
              <div className="flex min-w-0 items-center gap-2">
                <span className="inline-flex size-9 shrink-0 items-center justify-center rounded-md bg--soft text-accent">
                  <Hand size={18} />
                </span>
                <h2 className={cardTitleClass}>{strings.homeAskTitle}</h2>
              </div>
              <Sparkles size={16} className="text-accent" aria-hidden />
            </div>
            <div className="flex flex-1 flex-col justify-center gap-3">
              <p className="m-0 text-sm leading-relaxed text-secondary">{strings.homeAskBody}</p>
              <form className="flex min-h-12 items-center gap-2 rounded-lg border border-subtle bg-app p-1.5 pl-4 transition focus-within:border-accent focus-within:ring-2 focus-within:ring-[var(--accent-tint)]" onSubmit={(e) => void askAlo(e)}>
                <input
                  className="min-w-0 flex-1 border-0 bg-transparent text-sm text-primary outline-none placeholder:text-tertiary"
                  value={askText}
                  onChange={(e) => setAskText(e.target.value)}
                  placeholder={strings.homeAskPlaceholder}
                  aria-label={strings.homeAskPlaceholder}
                />
                <button
                  type="submit"
                  className="inline-flex size-10 shrink-0 items-center justify-center rounded-md !bg-accent !text-on-accent transition-colors hover:!bg--hover disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={askText.trim().length === 0 || asking}
                  aria-label={strings.homeAskCta}
                >
                  {asking ? <Spinner size={16} /> : <Send size={17} />}
                </button>
              </form>
              {askReply !== null && <p className="m-0 line-clamp-2 text-sm leading-relaxed text-primary" aria-live="polite">{askReply}</p>}
            </div>
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
    return <span className="shrink-0 rounded-full bg-[var(--danger-bg)] px-2 py-0.5 text-xs font-medium text-[var(--danger-text)]">{strings.homeTaskOverdue}</span>;
  }
  if (sameDay(dueAt, now)) {
    return <span className="shrink-0 rounded-full bg--soft px-2 py-0.5 text-xs font-medium text-accent">{strings.homeTaskToday}</span>;
  }
  return <span className="shrink-0 rounded-full bg-raised px-2 py-0.5 text-xs font-medium text-tertiary">{formatDate(dueAt)}</span>;
}

interface StatCardProps {
  Icon: LucideIcon;
  label: string;
  cta: string;
  onClick: () => void;
  value?: number | null;
  loading?: boolean;
  tone: "accent" | "neutral" | "success" | "message";
}

function StatCard({ Icon, label, cta, onClick, value, loading, tone }: StatCardProps) {
  const iconClass =
    tone === "accent"
      ? "bg--soft text-accent"
      : tone === "success"
        ? "bg-[var(--success-bg)] text-[var(--success-text)]"
        : tone === "message"
          ? "bg-[var(--accent-secondary-tint)] text-[var(--accent-secondary)]"
        : "bg-raised text-secondary";
  return (
    <button type="button" className="group flex min-h-[112px] items-center gap-4 rounded-xl border border-subtle bg-surface p-5 text-left shadow-sm transition hover:-translate-y-px hover:border-default hover:shadow-md focus-visible:outline-2 focus-visible:outline-accent max-sm:min-h-[96px]" onClick={onClick}>
      <span className={`inline-flex size-11 shrink-0 items-center justify-center rounded-lg ${iconClass}`}>
        <Icon size={20} />
      </span>
      <span className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="text-2xl font-bold leading-none tabular-nums text-primary">{loading === true ? "—" : (value ?? 0)}</span>
        <span className="truncate text-sm text-secondary">{label}</span>
      </span>
      <span className="inline-flex size-8 shrink-0 items-center justify-center rounded-full text-secondary transition group-hover:translate-x-0.5 group-hover:bg-raised group-hover:text-accent" aria-label={cta}>
        <span className="sr-only">{cta}</span>
        <ArrowRight size={13} />
      </span>
    </button>
  );
}

function EmptyState({ Icon, title, message, action, onAction, compact = false }: { Icon: LucideIcon; title: string; message: string; action: string; onAction: () => void; compact?: boolean }) {
  return (
    <div className={`flex flex-col items-center justify-center gap-2 p-5 text-center ${compact ? "min-h-[132px] py-3" : "min-h-[210px]"}`}>
      <span className="inline-flex size-12 items-center justify-center rounded-full bg--soft text-accent"><Icon size={20} aria-hidden="true" /></span>
      <strong className="text-sm font-semibold text-primary">{title}</strong>
      <p className="m-0 max-w-[34ch] text-sm leading-normal text-tertiary">{message}</p>
      <button className="inline-flex min-h-[34px] items-center gap-1 rounded-md border border-subtle bg-surface px-3 text-sm font-medium text-primary transition-colors hover:border-accent hover:bg--soft" type="button" onClick={onAction}>{action}<ArrowRight size={13} aria-hidden="true" /></button>
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
          className={tab === it.id ? "min-h-9 border-b-2 border-accent px-2 text-sm font-semibold text-primary" : "min-h-9 border-b-2 border-transparent px-2 text-sm font-medium text-secondary transition-colors hover:text-primary"}
          onClick={() => onChange(it.id)}
        >
          {it.label}
        </button>
      ))}
    </>
  );
}
