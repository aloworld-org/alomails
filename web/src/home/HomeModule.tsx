// The Home dashboard — the landing surface of alo workplace. It greets the user
// and gathers what's actually available today: real mail (unread count, recent
// and flagged messages) and document counts. Modules that aren't built yet
// (Agenda, Chat) are shown honestly as "coming soon" rather than with invented
// numbers, so the dashboard never implies data the product can't back.
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  ArrowRight,
  Calendar,
  FileText,
  Hand,
  Mail,
  MessagesSquare,
  PenLine,
  Sparkles,
  Star,
  Upload,
  Video,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { strings } from "../i18n";
import { Spinner } from "../ds";
import { KEYWORD_FLAGGED, KEYWORD_SEEN, useJmapClient } from "../jmap";
import type { EmailHeaders } from "../jmap";
import { useAuth } from "../auth";
import { formatDate, senderName, subjectOr } from "../mail/format";
import styles from "./HomeModule.module.css";

type Tab = "recent" | "starred" | "unread";

function greeting(): string {
  const h = new Date().getHours();
  if (h < 12) return strings.homeGreetingMorning;
  if (h < 18) return strings.homeGreetingAfternoon;
  return strings.homeGreetingEvening;
}

export function HomeModule() {
  const client = useJmapClient();
  const { identity } = useAuth();
  const navigate = useNavigate();

  const [loading, setLoading] = useState(true);
  const [unreadEmails, setUnreadEmails] = useState<number | null>(null);
  const [docCount, setDocCount] = useState<number | null>(null);
  const [recent, setRecent] = useState<EmailHeaders[]>([]);
  const [starred, setStarred] = useState<EmailHeaders[]>([]);
  const [tab, setTab] = useState<Tab>("recent");

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const boxes = await client.mailboxes();
        const inbox = boxes.find((b) => b.role === "inbox") ?? boxes[0];
        if (live) setUnreadEmails(inbox?.unreadEmails ?? 0);
        const [headers, flagged, docs] = await Promise.all([
          inbox ? client.emailHeaders(inbox.id, 8) : Promise.resolve([]),
          client.flaggedHeaders(8).catch(() => []),
          client.listDocs().catch(() => []),
        ]);
        if (!live) return;
        setRecent(headers);
        setStarred(flagged);
        setDocCount(docs.length);
      } catch {
        // Best-effort: the dashboard degrades to empty states, never an error.
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [client]);

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

  return (
    <div className={styles.home}>
      <header className={styles.head}>
        <div>
          <h1 className={styles.greeting}>
            {greeting()}
            {firstName.length > 0 ? `, ${firstName}` : ""}
            <Hand className={styles.wave} size={26} aria-hidden />
          </h1>
          <p className={styles.welcome}>{strings.homeWelcome}</p>
        </div>
        <button type="button" className={styles.compose} onClick={() => navigate("/mail")}>
          <PenLine size={17} />
          <span>{strings.homeCompose}</span>
        </button>
      </header>

      <section className={styles.stats}>
        <StatCard
          Icon={Mail}
          value={unreadEmails}
          loading={loading}
          label={strings.homeStatUnreadEmails}
          cta={strings.homeGoToMail}
          accent
          onClick={() => navigate("/mail")}
        />
        <StatCard
          Icon={Calendar}
          soon
          label={strings.homeStatEvents}
          cta={strings.homeViewAgenda}
          onClick={() => navigate("/agenda")}
        />
        <StatCard
          Icon={MessagesSquare}
          soon
          label={strings.homeStatMessages}
          cta={strings.homeOpenChat}
          onClick={() => navigate("/chat")}
        />
        <StatCard
          Icon={FileText}
          value={docCount}
          loading={loading}
          label={strings.homeStatFiles}
          cta={strings.homeOpenDrive}
          onClick={() => navigate("/drive")}
        />
      </section>

      <div className={styles.grid}>
        <section className={styles.card}>
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
            <p className={styles.empty}>{strings.homeNoRecent}</p>
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
          <section className={styles.card}>
            <h2 className={styles.cardTitle}>{strings.homeQuickActions}</h2>
            <div className={styles.actions}>
              <Action Icon={PenLine} label={strings.homeCompose} onClick={() => navigate("/mail")} />
              <Action Icon={Calendar} label={strings.homeCreateEvent} onClick={() => navigate("/agenda")} />
              <Action Icon={MessagesSquare} label={strings.homeStartChat} onClick={() => navigate("/chat")} />
              <Action Icon={Upload} label={strings.homeUploadFile} onClick={() => navigate("/drive")} />
              <Action Icon={FileText} label={strings.homeCreateDoc} onClick={() => navigate("/drive")} />
            </div>
          </section>

          <section className={styles.card}>
            <h2 className={styles.cardTitle}>{strings.homeToday}</h2>
            <div className={styles.todayEmpty}>
              <Video className={styles.todayIcon} size={22} aria-hidden />
              <p className={styles.muted}>{strings.homeAgendaComingSoon}</p>
            </div>
          </section>
        </aside>
      </div>

      <section className={styles.ask}>
        <span className={styles.askMark}>
          <Hand size={22} />
        </span>
        <div className={styles.askText}>
          <p className={styles.askTitle}>{strings.homeAskTitle}</p>
          <p className={styles.askBody}>{strings.homeAskBody}</p>
        </div>
        <button type="button" className={styles.askCta} onClick={() => navigate("/mail")}>
          <Sparkles size={16} />
          {strings.homeAskCta}
        </button>
      </section>
    </div>
  );
}

interface StatCardProps {
  Icon: LucideIcon;
  label: string;
  cta: string;
  onClick: () => void;
  value?: number | null;
  loading?: boolean;
  soon?: boolean;
  accent?: boolean;
}

function StatCard({ Icon, label, cta, onClick, value, loading, soon, accent }: StatCardProps) {
  return (
    <button type="button" className={styles.stat} onClick={onClick}>
      <span className={accent === true ? `${styles.statIcon} ${styles.statIconAccent}` : styles.statIcon}>
        <Icon size={20} />
      </span>
      {soon === true ? (
        <span className={styles.soon}>{strings.homeComingSoonShort}</span>
      ) : (
        <span className={styles.statValue}>{loading === true ? "—" : (value ?? 0)}</span>
      )}
      <span className={styles.statLabel}>{label}</span>
      <span className={styles.statCta}>
        {cta}
        <ArrowRight size={13} />
      </span>
    </button>
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

function Action({ Icon, label, onClick }: { Icon: LucideIcon; label: string; onClick: () => void }) {
  return (
    <button type="button" className={styles.action} onClick={onClick}>
      <Icon size={17} className={styles.actionIcon} />
      <span>{label}</span>
    </button>
  );
}
