// The message list for the selected folder — one row per CONVERSATION (thread),
// Gmail-style: a checkbox + star, then a two-line block (sender · time / subject
// — snippet). Unread threads read bold; on row hover the time swaps for archive
// / delete / read-toggle. Selecting rows turns the folder header into a bulk
// action bar (select-all · archive · delete · read/unread).
import { useEffect, useMemo, useState } from "react";
import {
  AlignJustify,
  Archive,
  CalendarClock,
  Check,
  Mail,
  MailPlus,
  MailOpen,
  MessagesSquare,
  PanelLeftClose,
  PanelLeftOpen,
  Paperclip,
  Search,
  Star,
  Trash2,
  X,
} from "lucide-react";

import { strings } from "../../i18n";
import { IconButton, cx } from "../../ds";
import { useJmapClient } from "../../jmap";
import type { Category, EmailHeaders } from "../../jmap";
import type { Async } from "../state/useAsync";
import { formatDate, recipientName, senderName, subjectOr } from "../format";
import { groupThreads, flatRows, type ThreadRow } from "../threads";
import { rowCategories } from "../categories";
import { DRAG_EMAIL_MIME } from "../dnd";
import { CategoryChips } from "./CategoryChips";
import { SnoozeMenu } from "./SnoozeMenu";
import styles from "./MessageList.module.css";

interface MessageListProps {
  folderName: string;
  /**
   * Show the recipient rather than the sender in each row.
   *
   * True for Sent, Drafts and Scheduled, where every message is from the
   * account owner and a sender column is a list of your own name. Passed
   * as a flag rather than derived from `folderName`, which is translated
   * and would break the moment somebody reads their mail in Dutch.
   */
  showsRecipient?: boolean;
  emails: Async<EmailHeaders[]>;
  /** A search term seeded from outside (Home search bar); pre-fills the box. */
  initialQuery?: string;
  selectedThreadId: string | null;
  readIds: ReadonlySet<string>;
  flagOverrides: ReadonlyMap<string, boolean>;
  foldersCollapsed: boolean;
  onToggleFolders: () => void;
  /** Flat (per-message) list vs grouped conversations. */
  flat: boolean;
  onToggleView: () => void;
  /** The account's category catalog, for the colored dots on each row. */
  categories: Category[];
  /** Whether snooze applies (hidden in the cross-folder Flagged view). */
  canSnooze: boolean;
  onSelect: (thread: ThreadRow) => void;
  /** Batch conversation actions (a single row passes `[thread]`). */
  onArchive: (threads: ThreadRow[]) => void;
  onDelete: (threads: ThreadRow[]) => void;
  onMarkRead: (threads: ThreadRow[], read: boolean) => void;
  onSnooze: (threads: ThreadRow[], until: number) => void;
  onToggleFlag: (thread: ThreadRow) => void;
  onCompose: () => void;
}

/** A compact flag due-date badge for a list row: a clock + short date, red when
 * overdue. Nothing when the message has no due-date. */
function DueBadge({ email }: { email: EmailHeaders }) {
  const due = email["alo:flagDue"];
  if (due == null) return null;
  const when = new Date(due);
  const overdue = when.getTime() < Date.now();
  const label = when.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  return (
    <span className={cx(styles.dueBadge, overdue && styles.dueOverdue)} title={when.toLocaleString()}>
      <CalendarClock size={12} />
      {label}
    </span>
  );
}

/** A small checkbox box (avoids depending on a specific lucide check-square name). */
function CheckBox({ on }: { on: boolean }) {
  return (
    <span className={cx(styles.check, on && styles.checkOn)} aria-hidden="true">
      {on && <Check size={13} strokeWidth={3} />}
    </span>
  );
}

export function MessageList({
  folderName,
  showsRecipient = false,
  initialQuery = "",
  flat,
  onToggleView,
  categories,
  canSnooze,
  emails,
  selectedThreadId,
  readIds,
  flagOverrides,
  foldersCollapsed,
  onToggleFolders,
  onSelect,
  onArchive,
  onDelete,
  onMarkRead,
  onSnooze,
  onToggleFlag,
  onCompose,
}: MessageListProps) {
  const client = useJmapClient();
  const [query, setQuery] = useState(initialQuery);
  const [results, setResults] = useState<EmailHeaders[] | null>(null);
  // Adopt a new seed from the Home search bar when it arrives.
  useEffect(() => {
    if (initialQuery.length > 0) setQuery(initialQuery);
  }, [initialQuery]);
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const isSearch = query.trim() !== "";

  useEffect(() => {
    const q = query.trim();
    if (q === "") {
      setResults(null);
      return undefined;
    }
    setResults(null);
    let live = true;
    const timer = setTimeout(() => {
      client
        .searchEmails(q)
        .then((r) => live && setResults(r))
        .catch(() => live && setResults([]));
    }, 250);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [query, client]);

  const list = isSearch ? (results ?? []) : emails.status === "ready" ? (emails.data ?? []) : [];
  const threads = useMemo(
    () => (flat ? flatRows(list, readIds, flagOverrides) : groupThreads(list, readIds, flagOverrides)),
    [flat, list, readIds, flagOverrides],
  );
  const loading = isSearch ? results === null : emails.status === "loading";
  const error = !isSearch && emails.status === "error";

  // Clear selection when the folder or the visible set changes.
  useEffect(() => setSelected(new Set()), [folderName, isSearch]);

  const selectedThreads = useMemo(
    () => threads.filter((t) => selected.has(t.threadId)),
    [threads, selected],
  );
  const allSelected = threads.length > 0 && selected.size === threads.length;
  const anyUnreadSelected = selectedThreads.some((t) => t.hasUnread);

  function toggleOne(threadId: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(threadId)) next.delete(threadId);
      else next.add(threadId);
      return next;
    });
  }

  function toggleAll() {
    setSelected(allSelected ? new Set() : new Set(threads.map((t) => t.threadId)));
  }

  function runBulk(action: (ts: ThreadRow[]) => void) {
    action(selectedThreads);
    setSelected(new Set());
  }

  return (
    <section className={styles.column}>
      {selected.size > 0 ? (
        <header className={cx(styles.header, styles.bulkBar)}>
          <button
            type="button"
            className={styles.bulkCheck}
            onClick={toggleAll}
            aria-label={allSelected ? strings.selectNone : strings.selectAll}
          >
            <CheckBox on={allSelected} />
          </button>
          <span className={styles.bulkCount}>{strings.selectedCount(selected.size)}</span>
          <div className={styles.headSpacer} />
          <IconButton
            size="sm"
            label={strings.archive}
            icon={<Archive />}
            onClick={() => runBulk(onArchive)}
          />
          <IconButton
            size="sm"
            label={strings.delete}
            icon={<Trash2 />}
            onClick={() => runBulk(onDelete)}
          />
          <IconButton
            size="sm"
            label={anyUnreadSelected ? strings.markRead : strings.markUnread}
            icon={anyUnreadSelected ? <MailOpen /> : <Mail />}
            onClick={() => runBulk((ts) => onMarkRead(ts, anyUnreadSelected))}
          />
          {canSnooze && (
            <SnoozeMenu compact onPick={(until) => runBulk((ts) => onSnooze(ts, until))} />
          )}
          <IconButton
            size="sm"
            label={strings.selectNone}
            icon={<X />}
            onClick={() => setSelected(new Set())}
          />
        </header>
      ) : (
        <header className={styles.header}>
          <div className={styles.titleRow}>
            <IconButton
              size="sm"
              label={foldersCollapsed ? strings.expandFolders : strings.collapseFolders}
              icon={foldersCollapsed ? <PanelLeftOpen /> : <PanelLeftClose />}
              onClick={onToggleFolders}
            />
            <h1 className={styles.title}>{folderName}</h1>
            <div className={styles.headSpacer} />
            <IconButton
              size="sm"
              label={flat ? strings.viewAsConversations : strings.viewAsMessages}
              icon={flat ? <MessagesSquare /> : <AlignJustify />}
              onClick={onToggleView}
            />
          </div>
          <div className={styles.search}>
            <Search size={16} className={styles.searchIcon} />
            <input
              className={styles.searchInput}
              type="search"
              placeholder={strings.mailSearchPlaceholder}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              aria-label={strings.mailSearchPlaceholder}
            />
          </div>
        </header>
      )}

      {loading && (
        <div className={styles.listSkeleton} role="status" aria-label={isSearch ? strings.mailSearching : strings.mailLoading} aria-busy="true">
          {Array.from({ length: 7 }, (_, index) => (
            <span key={index} className={styles.listSkeletonRow}>
              <span className={styles.listSkeletonSender} />
              <span className={styles.listSkeletonSubject} />
            </span>
          ))}
        </div>
      )}

      {error && (
        <div className={styles.state}>
          <p>{strings.mailListError}</p>
          <button type="button" className={styles.retry} onClick={emails.reload}>
            {strings.mailRetry}
          </button>
        </div>
      )}

      {!loading && !error && (
        <ul className={styles.list}>
          {threads.length === 0 && (
            <li className={styles.empty}>
              <span className={styles.emptyArt} aria-hidden="true"><MailPlus /></span>
              <p>{isSearch ? strings.mailSearchEmpty : strings.mailEmpty}</p>
              <button type="button" className={styles.emptyAction} onClick={isSearch ? () => setQuery("") : onCompose}>
                {isSearch ? strings.eqSearchClear : strings.compose}
              </button>
            </li>
          )}
          {threads.map((thread) => {
            const email = thread.latest;
            const active = thread.threadId === selectedThreadId;
            const isSel = selected.has(thread.threadId);
            return (
              <li
                key={thread.threadId}
                className={cx(
                  styles.row,
                  active && styles.active,
                  isSel && styles.selectedRow,
                  thread.hasUnread && styles.unread,
                )}
              >
                <button
                  type="button"
                  className={styles.checkBtn}
                  onClick={() => toggleOne(thread.threadId)}
                  aria-label={isSel ? strings.selectNone : strings.selectAll}
                  aria-pressed={isSel}
                >
                  <CheckBox on={isSel} />
                </button>
                <button
                  type="button"
                  className={styles.flagBtn}
                  aria-label={thread.hasFlagged ? strings.unflag : strings.flag}
                  onClick={() => onToggleFlag(thread)}
                >
                  <Star className={cx(styles.star, thread.hasFlagged && styles.starOn)} />
                </button>
                <button
                  type="button"
                  className={styles.rowOpen}
                  onClick={() => onSelect(thread)}
                  aria-current={active ? "true" : undefined}
                  draggable
                  onDragStart={(e) => {
                    e.dataTransfer.setData(DRAG_EMAIL_MIME, thread.memberIds.join(","));
                    e.dataTransfer.effectAllowed = "move";
                  }}
                >
                  <span className={styles.line1}>
                    <span className={styles.sender}>
                      {showsRecipient ? recipientName(email) : senderName(email)}
                      {thread.count > 1 && <span className={styles.count}> ({thread.count})</span>}
                    </span>
                  </span>
                  <span className={styles.line2}>
                    <CategoryChips categories={rowCategories(thread, categories)} variant="dots" />
                    <span className={styles.subject}>{subjectOr(email)}</span>
                    {email.preview.length > 0 && (
                      <span className={styles.snippet}> — {email.preview}</span>
                    )}
                  </span>
                </button>
                <div className={styles.rowRight}>
                  <DueBadge email={email} />
                  <span className={styles.time}>{formatDate(email.receivedAt)}</span>
                  {thread.hasAttachment && (
                    <Paperclip className={styles.rowClip} aria-label={strings.attachments} />
                  )}
                  <div className={styles.actions}>
                    <button
                      type="button"
                      className={styles.actionBtn}
                      aria-label={strings.archive}
                      title={strings.archive}
                      onClick={() => onArchive([thread])}
                    >
                      <Archive size={16} />
                    </button>
                    <button
                      type="button"
                      className={styles.actionBtn}
                      aria-label={strings.delete}
                      title={strings.delete}
                      onClick={() => onDelete([thread])}
                    >
                      <Trash2 size={16} />
                    </button>
                    <button
                      type="button"
                      className={styles.actionBtn}
                      aria-label={thread.hasUnread ? strings.markRead : strings.markUnread}
                      title={thread.hasUnread ? strings.markRead : strings.markUnread}
                      onClick={() => onMarkRead([thread], thread.hasUnread)}
                    >
                      {thread.hasUnread ? <MailOpen size={16} /> : <Mail size={16} />}
                    </button>
                  </div>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
