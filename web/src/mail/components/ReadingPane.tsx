// The reading pane — a CONVERSATION view. It shows the whole thread (all its
// messages, across folders) stacked oldest-first, the newest expanded and the
// rest collapsed to a click-to-open summary. The action toolbar operates on the
// conversation: Reply/Forward act on the latest message; Flag toggles it;
// Archive/Delete/Move act on this folder's copies of the whole thread.
import { useEffect, useState } from "react";
import {
  Archive,
  ArrowLeft,
  Ban,
  CalendarClock,
  Code2,
  Download,
  FolderInput,
  Forward,
  Handshake,
  Inbox,
  ListChecks,
  MailOpen,
  MoreHorizontal,
  Paperclip,
  Pencil,
  Printer,
  Reply,
  ReplyAll,
  Send,
  ShieldAlert,
  Sparkles,
  Star,
  Trash2,
} from "lucide-react";

import { RecordAgentPanel } from "../../agents";
import { strings } from "../../i18n";
import { Button, IconButton, Menu, Toolbar, ToolbarSpacer } from "../../ds";
import type { MenuItem } from "../../ds";
import {
  KEYWORD_FLAGGED,
  type Category,
  type EmailFull,
  type Mailbox,
  useJmapClient,
} from "../../jmap";
import { useAuth } from "../../auth";
import type { Async } from "../state/useAsync";
import { senderName, subjectOr } from "../format";
import { threadCategoryIds } from "../categories";
import { htmlContent, textContent, threadDigest } from "../body";
import { ThreadMessage } from "./ThreadMessage";
import { CategoryChips } from "./CategoryChips";
import { CategoryPicker } from "./CategoryPicker";
import { FlagDueControl } from "./FlagDueControl";
import { SpamBanner } from "./SpamBanner";
import { SnoozeMenu } from "./SnoozeMenu";
import styles from "./ReadingPane.module.css";

/** Below this many characters a thread isn't worth summarizing — the message is
 * already short enough to read directly, and a one-line "summary" adds noise. */
const SUMMARY_MIN_CHARS = 600;

/** Escape text for safe interpolation into the print window's HTML. */
function escapeForPrint(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

type SummaryState =
  | { status: "off" }
  | { status: "loading" }
  | { status: "ready"; text: string };

type RepliesState =
  | { status: "off" }
  | { status: "ready"; options: string[] };

const ROLE_ORDER: Record<string, number> = {
  inbox: 0,
  drafts: 1,
  sent: 2,
  archive: 3,
  junk: 4,
  trash: 5,
};

interface ReadingPaneProps {
  thread: Async<EmailFull[]>;
  mailboxes: Mailbox[];
  /** The folder currently being viewed (excluded from "Move to"). */
  currentMailboxId: string | null;
  flagOverrides: ReadonlyMap<string, boolean>;
  /** On the single-pane (mobile) layout, returns to the message list.
   * `undefined` on desktop, where the list is always visible. */
  onBack?: () => void;
  onReply: () => void;
  onReplyAll: () => void;
  onForward: () => void;
  onEditDraft: () => void;
  /** Submit an already prepared message from Drafts through Mail's ordinary
   * audited send queue. Present only for a real `$draft` message. */
  onSendDraft: () => void;
  onToggleFlag: () => void;
  onArchive: () => void;
  onDelete: () => void;
  onMove: (targetMailboxId: string) => void;
  onMarkUnread: () => void;
  onSnooze: (until: number) => void;
  /** Move the conversation to Junk (or back to Inbox when already there). */
  onReportSpam: () => void;
  /** Compose a new message with this message attached as an .eml. */
  onForwardAttachment: () => void;
  /** Open a reply to the latest message pre-filled with a picked AI reply. */
  onSmartReply: (text: string) => void;
  /** Cancel a scheduled send (only shown while viewing the Scheduled folder). */
  onCancelSend: () => void;
  /** Block the sender of the open conversation (files their mail to Junk). */
  onBlockSender: (email: string) => void;
  /** Whether the open conversation is in the Scheduled folder (send later). */
  isScheduled: boolean;
  /** Whether the open conversation is in the Junk folder (flips Report/Not spam). */
  isJunk: boolean;
  /** The account's category catalog (for the Categorize picker + chips). */
  categories: Category[];
  /** Tag/untag the whole open conversation with a category. */
  onToggleCategory: (categoryId: string, on: boolean) => void;
  /** Unsubscribe from the latest message's mailing list (one-click / mailto /
   * open link — decided by the module). */
  onUnsubscribe: () => void;
  /** Whether snooze applies here (hidden in the cross-folder Flagged view,
   * which has no source folder to restore a snoozed message to). */
  canSnooze: boolean;
  /** Set/clear the follow-up due-date on the flagged conversation. */
  onSetFlagDue: (dueAt: number | null) => void;
  /** Create a task from this message (direct — no AI), carrying its source
   *  link so the task shows "From an email" and can jump back (ADR 0024). */
  onCreateTask: () => void;
  /** Review and create a Sales opportunity with this conversation linked. */
  onCreateOpportunity: () => void;
  /** Ask the AI to suggest tasks from this email; they land in the Suggestions
   *  inbox to accept/dismiss, never straight on the board (ADR 0023/0024). The
   *  action is offered only when the pane's own `aiEnabled` is true. */
  onSuggestTasks: () => void;
  onCompose: () => void;
}

export function ReadingPane({
  thread,
  mailboxes,
  currentMailboxId,
  flagOverrides,
  onBack,
  onReply,
  onReplyAll,
  onForward,
  onEditDraft,
  onSendDraft,
  onToggleFlag,
  onArchive,
  onDelete,
  onMove,
  onMarkUnread,
  onSnooze,
  onReportSpam,
  onForwardAttachment,
  onSmartReply,
  onCancelSend,
  onBlockSender,
  isScheduled,
  isJunk,
  categories,
  onToggleCategory,
  onUnsubscribe,
  canSnooze,
  onSetFlagDue,
  onCreateTask,
  onCreateOpportunity,
  onSuggestTasks,
  onCompose,
}: ReadingPaneProps) {
  const { identity } = useAuth();
  const client = useJmapClient();
  const messages = thread.status === "ready" ? (thread.data ?? []) : [];
  const latest = messages.length > 0 ? messages[messages.length - 1] : undefined;

  // Expanded set: the newest message opens by default; reset when the thread
  // changes (keyed by the latest message id).
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  useEffect(() => {
    setExpanded(latest !== undefined ? new Set([latest.id]) : new Set());
  }, [latest?.id]);

  // Whether the tenant has AI enabled — determines if we offer a summary.
  const [aiEnabled, setAiEnabled] = useState(false);
  useEffect(() => {
    let live = true;
    client
      .aiEnabled()
      .then((on) => live && setAiEnabled(on))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [client]);

  // The alo conversation summary. Fetched when a long-enough thread opens and
  // AI is on; soft-degrades to nothing on any error (never blocks reading).
  const [summary, setSummary] = useState<SummaryState>({ status: "off" });
  useEffect(() => {
    if (!aiEnabled || latest === undefined) {
      setSummary({ status: "off" });
      return;
    }
    const digest = threadDigest(messages);
    if (digest.length < SUMMARY_MIN_CHARS) {
      setSummary({ status: "off" });
      return;
    }
    let live = true;
    setSummary({ status: "loading" });
    client
      .summarizeThread(digest)
      .then((text) => {
        if (live && text.trim().length > 0) setSummary({ status: "ready", text: text.trim() });
        else if (live) setSummary({ status: "off" });
      })
      .catch(() => live && setSummary({ status: "off" }));
    return () => {
      live = false;
    };
    // Re-run only when the conversation identity or the AI toggle changes; the
    // digest and client are derived from those and intentionally not deps.
  }, [aiEnabled, latest?.id]);

  // AI smart replies — three short, ready-to-send options for the open thread.
  // Only when AI is on and the newest message is from someone else (replying to
  // your own last message is meaningless); soft-degrades to nothing on error.
  const [replies, setReplies] = useState<RepliesState>({ status: "off" });
  useEffect(() => {
    const meEmail = identity?.email.toLowerCase();
    const lastFromMe = latest?.from?.some((a) => a.email.toLowerCase() === meEmail) ?? false;
    if (!aiEnabled || latest === undefined || lastFromMe) {
      setReplies({ status: "off" });
      return;
    }
    const digest = threadDigest(messages);
    if (digest.trim().length === 0) {
      setReplies({ status: "off" });
      return;
    }
    let live = true;
    setReplies({ status: "off" });
    client
      .smartReplies(digest)
      .then((options) => {
        const clean = options.map((o) => o.trim()).filter((o) => o.length > 0);
        if (live && clean.length > 0) setReplies({ status: "ready", options: clean });
      })
      .catch(() => live && setReplies({ status: "off" }));
    return () => {
      live = false;
    };
    // Same dependency reasoning as the summary effect above.
  }, [aiEnabled, latest?.id]);

  if (thread.status === "loading") {
    return (
      <div className={styles.readingSkeleton} role="status" aria-label={strings.mailLoading} aria-busy="true">
        <span className={styles.readingSkeletonToolbar} />
        <span className={styles.readingSkeletonSubject} />
        <span className={styles.readingSkeletonMeta} />
        <span className={styles.readingSkeletonBody} />
      </div>
    );
  }
  if (thread.status === "error") {
    return (
      <div className={styles.state}>
        <p>{strings.mailListError}</p>
        <button type="button" className={styles.retry} onClick={thread.reload}>
          {strings.mailRetry}
        </button>
      </div>
    );
  }
  if (latest === undefined) {
    return (
      <div className={styles.emptyState}>
        <span className={styles.emptyArt} aria-hidden="true">
          <Inbox size={34} strokeWidth={1.7} />
        </span>
        <h2 className={styles.emptyTitle}>{strings.mailSelectPrompt}</h2>
        <p className={styles.emptyBody}>{strings.mailSelectBody}</p>
        <button type="button" className={styles.emptyAction} onClick={onCompose}>{strings.compose}</button>
      </div>
    );
  }

  const flagged = flagOverrides.get(latest.id) ?? latest.keywords[KEYWORD_FLAGGED] === true;
  const isDraft = latest.keywords.$draft === true;
  // Categories present on any message of the conversation, and their catalog
  // entries (for the pills below the subject).
  const activeCategoryIds = threadCategoryIds(messages, categories);
  const activeCategories = categories.filter((c) => activeCategoryIds.has(c.id));
  const canUnsubscribe = latest["alo:listUnsubscribe"] != null;
  const me = identity?.email.toLowerCase();

  function toggle(id: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  const moveItems: MenuItem[] = mailboxes
    // Snoozed/Scheduled are managed by their own flows — a manual move there
    // wouldn't set a wake/send time, so they're not valid "Move to" targets.
    .filter((m) => m.id !== currentMailboxId && m.role !== "snoozed" && m.role !== "scheduled")
    .sort(
      (a, b) =>
        (ROLE_ORDER[a.role ?? ""] ?? 50) - (ROLE_ORDER[b.role ?? ""] ?? 50) ||
        a.name.localeCompare(b.name),
    )
    .map((m) => ({ key: m.id, label: m.name, onClick: () => onMove(m.id) }));

  /** The message these single-message actions apply to (the newest). */
  const target = latest;
  const senderEmail = target.from?.[0]?.email;
  const emlName = `${(subjectOr(target) || "message").replace(/[^\w.-]+/g, "_").slice(0, 60)}.eml`;

  async function fetchRaw(): Promise<Blob> {
    return client.downloadAttachment(target.blobId, emlName);
  }

  // "Show original": the raw RFC 822 in a new tab, as plain text (no HTML parse).
  async function showOriginal() {
    try {
      const raw = await (await fetchRaw()).text();
      const w = window.open("", "_blank", "noopener");
      if (w === null) return;
      w.document.title = strings.showOriginal;
      const pre = w.document.createElement("pre");
      pre.textContent = raw;
      pre.style.cssText =
        "white-space:pre-wrap;word-break:break-word;font-family:ui-monospace,Menlo,Consolas,monospace;font-size:12px;line-height:1.5;padding:20px;margin:0";
      w.document.body.appendChild(pre);
    } catch {
      // downloading the raw message failed — nothing to show
    }
  }

  async function downloadEml() {
    try {
      const blob = await fetchRaw();
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = emlName;
      a.click();
      URL.revokeObjectURL(url);
    } catch {
      // ignore
    }
  }

  // Print the whole conversation in a new window (CSP blocks scripts in bodies).
  function printThread() {
    const parts = messages
      .map((m) => {
        const who = senderName(m);
        const when = new Date(m.receivedAt).toLocaleString();
        const text = textContent(m);
        const bodyHtml =
          text !== null
            ? `<pre style="white-space:pre-wrap;font-family:inherit;margin:0">${escapeForPrint(text)}</pre>`
            : (htmlContent(m) ?? `<p>${escapeForPrint(m.preview)}</p>`);
        return `<section style="margin:0 0 24px;padding-bottom:16px;border-bottom:1px solid #ddd"><div style="color:#555;font-size:13px;margin-bottom:8px"><strong>${escapeForPrint(who)}</strong> · ${escapeForPrint(when)}</div>${bodyHtml}</section>`;
      })
      .join("");
    const doc = `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data: https:; style-src 'unsafe-inline'"><title>${escapeForPrint(subjectOr(target))}</title><style>body{font-family:Inter,system-ui,sans-serif;color:#111;padding:32px;max-width:720px;margin:auto}h1{font-size:20px;font-weight:600}</style></head><body><h1>${escapeForPrint(subjectOr(target))}</h1>${parts}</body></html>`;
    const w = window.open("", "_blank", "noopener");
    if (w === null) return;
    w.document.write(doc);
    w.document.close();
    w.focus();
    w.print();
  }

  const moreItems: MenuItem[] = [
    ...(isScheduled
      ? [
          {
            key: "cancel-send",
            label: strings.cancelSend,
            icon: <CalendarClock />,
            onClick: onCancelSend,
          },
        ]
      : []),
    { key: "task", label: strings.createTask, icon: <ListChecks />, onClick: onCreateTask },
    {
      key: "opportunity",
      label: strings.crmCreateOpportunity,
      icon: <Handshake />,
      onClick: onCreateOpportunity,
    },
    ...(aiEnabled
      ? [
          {
            key: "ai-tasks",
            label: strings.suggestTasks,
            icon: <Sparkles />,
            onClick: onSuggestTasks,
          },
        ]
      : []),
    { key: "unread", label: strings.markUnread, icon: <MailOpen />, onClick: onMarkUnread },
    {
      key: "spam",
      label: isJunk ? strings.notSpam : strings.reportSpam,
      icon: <ShieldAlert />,
      onClick: onReportSpam,
    },
    {
      key: "fwd-att",
      label: strings.forwardAsAttachment,
      icon: <Paperclip />,
      onClick: onForwardAttachment,
    },
    ...(senderEmail !== undefined && senderEmail.toLowerCase() !== me
      ? [
          {
            key: "block",
            label: strings.blockSenderNamed(senderEmail),
            icon: <Ban />,
            onClick: () => onBlockSender(senderEmail),
          },
        ]
      : []),
    { key: "print", label: strings.print, icon: <Printer />, onClick: () => printThread() },
    { key: "original", label: strings.showOriginal, icon: <Code2 />, onClick: () => void showOriginal() },
    { key: "download", label: strings.downloadEml, icon: <Download />, onClick: () => void downloadEml() },
    { key: "delete", label: strings.delete, icon: <Trash2 />, danger: true, onClick: onDelete },
  ];

  return (
    <article className={styles.pane}>
      <Toolbar label={strings.conversationActions} surface="bar" density="compact">
        {onBack !== undefined && (
          <IconButton
            size="sm"
            className={styles.backBtn}
            label={strings.composeBack}
            icon={<ArrowLeft />}
            onClick={onBack}
          />
        )}
        {isDraft ? (
          <>
            <Button size="sm" icon={<Pencil />} onClick={onEditDraft}>
              {strings.composeEdit}
            </Button>
            <Button size="sm" variant="ghost" icon={<Send />} onClick={onSendDraft}>
              {strings.composeSend}
            </Button>
          </>
        ) : (
          <>
            <Button size="sm" icon={<Reply />} onClick={onReply}>
              {strings.reply}
            </Button>
            <Button size="sm" variant="ghost" icon={<ReplyAll />} onClick={onReplyAll}>
              {strings.replyAll}
            </Button>
            <Button size="sm" variant="ghost" icon={<Forward />} onClick={onForward}>
              {strings.forward}
            </Button>
          </>
        )}
        <ToolbarSpacer />
        {canSnooze && <SnoozeMenu onPick={onSnooze} />}
        <IconButton size="sm" label={strings.archive} icon={<Archive />} onClick={onArchive} />
        <Menu label={strings.moveTo} icon={<FolderInput />} items={moveItems} />
        <IconButton
          size="sm"
          label={flagged ? strings.unflag : strings.flag}
          active={flagged}
          icon={<Star className={flagged ? styles.starOn : ""} />}
          onClick={onToggleFlag}
        />
        <CategoryPicker
          categories={categories}
          activeIds={activeCategoryIds}
          onToggle={onToggleCategory}
        />
        <IconButton size="sm" label={strings.markUnread} icon={<MailOpen />} onClick={onMarkUnread} />
        <IconButton size="sm" label={strings.delete} icon={<Trash2 />} onClick={onDelete} />
        <Menu label={strings.moreActions} icon={<MoreHorizontal />} items={moreItems} />
      </Toolbar>

      <div className={styles.bodyScroll}>
        {isJunk && (
          <SpamBanner
            auth={latest["alo:authentication"]}
            from={latest.from}
            onNotSpam={onReportSpam}
          />
        )}
        <div className={styles.subjectRow}>
          <h1 className={styles.subject}>{subjectOr(latest)}</h1>
          {canUnsubscribe && (
            <button type="button" className={styles.unsubscribe} onClick={onUnsubscribe}>
              {strings.unsubscribe}
            </button>
          )}
          {messages.length > 1 && (
            <span className={styles.threadCount}>
              {messages.length} {strings.threadMessages}
            </span>
          )}
        </div>
        {(activeCategories.length > 0 || flagged) && (
          <div className={styles.categoryRow}>
            <CategoryChips categories={activeCategories} variant="pills" />
            {flagged && (
              <FlagDueControl due={latest["alo:flagDue"] ?? null} onSet={onSetFlagDue} />
            )}
          </div>
        )}

        {summary.status !== "off" && (
          <section className={styles.summary} aria-live="polite">
            <div className={styles.summaryHead}>
              <Sparkles size={14} className={styles.summaryIcon} />
              <span>{strings.aloSummary}</span>
            </div>
            {summary.status === "loading" ? (
              <p className={styles.summaryPending}>{strings.summaryPending}</p>
            ) : (
              <p className={styles.summaryText}>{summary.text}</p>
            )}
          </section>
        )}

        <div className={styles.messages}>
          {messages.map((message) => (
            <ThreadMessage
              key={message.id}
              email={message}
              expanded={expanded.has(message.id)}
              me={me}
              onToggle={() => toggle(message.id)}
            />
          ))}
        </div>

        {/* The conversation as a record (A8.4): who sent the message these
            actions act on, what @mail can do with it, and a question about it
            answered in place. Under the thread, because the mail is what the
            pane was opened for — and it is the same panel every module has. */}
        <div className="mt-5">
          <RecordAgentPanel
            product="mail"
            recordKind="message"
            recordId={target.id}
            recordLabel={subjectOr(target)}
            origin={{
              kind: "sender",
              id: target.id,
              label: senderName(target),
            }}
          />
        </div>

        {replies.status === "ready" && (
          <div className={styles.smartReplies} aria-label={strings.smartReplies}>
            <Sparkles size={15} className={styles.smartReplyIcon} aria-hidden />
            {replies.options.map((option, i) => (
              <button
                key={i}
                type="button"
                className={styles.smartReply}
                onClick={() => onSmartReply(option)}
              >
                {option}
              </button>
            ))}
          </div>
        )}
      </div>
    </article>
  );
}
