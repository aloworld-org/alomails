// The Mail module: the four-column surface (folders · conversations · reading
// pane) and the state tying them together — folder + thread selection,
// optimistic read/flag state, and conversation-level actions (reply/forward on
// the latest message; flag, archive, delete, move, mark-unread, and
// drag-and-drop on the whole thread within the current folder).
import { useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { useSearchParams } from "react-router-dom";

import { strings } from "../i18n";
import { ModuleSidebar, ResizeHandle, cx, usePanelWidth, useIsMobile, useDialogs } from "../ds";
import { KEYWORD_FLAGGED, useJmapClient } from "../jmap";
import type { Category, EmailAddress, EmailFull, SharedMailbox } from "../jmap";
import { useAuth } from "../auth";
import { useCategories, useEmailHeaders, useFlagged, useMailboxTrees, useThread } from "./state/useMail";
import { mailErrorReason, senderName } from "./format";
import type { ThreadRow } from "./threads";
import { FolderSidebar } from "./components/FolderSidebar";
import { CategorySection } from "./components/CategorySection";
import { MessageList } from "./components/MessageList";
import { ReadingPane } from "./components/ReadingPane";
import { ComposeModal, formatSendAt } from "./components/ComposeModal";
import type { ComposeContext, QueuedSend } from "./components/ComposeModal";
import styles from "./MailModule.module.css";

/** Parse a `mailto:` unsubscribe URI into compose seeds (recipients + optional
 * subject/body from the query). */
function parseMailto(mailto: string): {
  to: EmailAddress[];
  subject: string | undefined;
  body: string | undefined;
} {
  const withoutScheme = mailto.replace(/^mailto:/i, "");
  const [addrPart, query] = withoutScheme.split("?");
  const to: EmailAddress[] = (addrPart ?? "")
    .split(",")
    .map((a) => decodeURIComponent(a.trim()))
    .filter((a) => a.length > 0)
    .map((email) => ({ name: null, email }));
  const params = new URLSearchParams(query ?? "");
  return {
    to,
    subject: params.get("subject") ?? undefined,
    body: params.get("body") ?? undefined,
  };
}

export function MailModule() {
  const client = useJmapClient();
  const { confirm } = useDialogs();
  const { identity } = useAuth();
  const categories = useCategories();
  const categoryList = categories.status === "ready" ? (categories.data ?? []) : [];

  // Resizable panels (drag the dividers; persisted across sessions).
  const folders = usePanelWidth("alo.mail.foldersWidth", 232, 176, 420);
  const list = usePanelWidth("alo.mail.listWidth", 372, 300, 640);

  const [mailboxId, setMailboxId] = useState<string | null>(null);
  // Category filter (a facet on the current folder): null = show everything.
  const [categoryFilter, setCategoryFilter] = useState<string | null>(null);
  // The cross-folder "Flagged" smart view (a virtual folder, not a mailbox).
  const [flaggedView, setFlaggedView] = useState(false);
  const [threadId, setThreadId] = useState<string | null>(null);
  // Conversation vs flat (per-message) list — a per-device preference.
  const [flatView, setFlatView] = useState(() => localStorage.getItem("alo.mail.flat") === "1");
  function toggleView() {
    setFlatView((v) => {
      const next = !v;
      localStorage.setItem("alo.mail.flat", next ? "1" : "0");
      setThreadId(null);
      return next;
    });
  }
  const [readIds, setReadIds] = useState<ReadonlySet<string>>(new Set());
  const [flags, setFlags] = useState<ReadonlyMap<string, boolean>>(new Map());
  const [toast, setToast] = useState<string | null>(null);
  const [compose, setCompose] = useState<ComposeContext | null>(null);
  // The user's signature + tenant footer, inserted into new/reply drafts.
  const [mailSettings, setMailSettings] = useState<{ signature: string; orgFooter: string }>({
    signature: "",
    orgFooter: "",
  });
  // Addresses the user may send from (canonical + aliases), for the From picker.
  const [sendAs, setSendAs] = useState<string[]>([]);
  // Shared mailboxes the user was delegated (ADR 0017), and which one (if any)
  // is currently open. null = the user's own mailbox.
  const [shared, setShared] = useState<SharedMailbox[]>([]);
  const [activeAccount, setActiveAccount] = useState<string | null>(null);
  // The user's own account id (loaded once) — the anchor for the "own" tree.
  const [ownId, setOwnId] = useState<string | null>(null);
  const activeShared = shared.find((s) => s.id === activeAccount);
  // The signed-in user's own account id (captured once), the account currently
  // being viewed, and the reload action — held in refs so the long-lived push
  // subscription always reads current values without re-subscribing.
  const ownIdRef = useRef<string | null>(null);
  const watchIdRef = useRef<string | null>(null);
  const reloadRef = useRef<() => void>(() => {});
  const [foldersCollapsed, setFoldersCollapsed] = useState<boolean>(() => {
    try {
      return localStorage.getItem("alo.mail.foldersCollapsed") === "1";
    } catch {
      return false;
    }
  });

  // Phone layout: one pane at a time. Folders are an off-canvas drawer
  // (`ds/ModuleSidebar`); the list and the reading pane swap on selection.
  const isMobile = useIsMobile();
  const [foldersOpen, setFoldersOpen] = useState(false);
  // Stable: the drawer's trap re-arms (and re-seizes focus) if this changes.
  const closeFolders = useCallback(() => setFoldersOpen(false), []);

  function toggleFolders() {
    // On mobile the same control opens/closes the folders drawer; on
    // desktop it collapses the folders column.
    if (isMobile) {
      setFoldersOpen((open) => !open);
      return;
    }
    setFoldersCollapsed((collapsed) => {
      const next = !collapsed;
      try {
        localStorage.setItem("alo.mail.foldersCollapsed", next ? "1" : "0");
      } catch {
        // ignore — collapse state simply won't persist
      }
      return next;
    });
  }

  const folderEmails = useEmailHeaders(mailboxId, categoryFilter);
  const flaggedEmails = useFlagged(flaggedView);
  const emails = flaggedView ? flaggedEmails : folderEmails;
  const thread = useThread(threadId);

  // Every accessible account's folder tree at once — the user's own plus each
  // delegated shared mailbox (ADR 0017) — so the sidebar can mount them all.
  const accountIds = ownId === null ? [] : [ownId, ...shared.map((s) => s.id)];
  const trees = useMailboxTrees(accountIds);
  const treeMap = trees.status === "ready" ? (trees.data ?? {}) : {};
  const activeId = activeAccount ?? ownId ?? "";
  // The active account's folders drive the current view and folder actions. A
  // shim keeps the old Async<Mailbox[]> shape (status/data/reload) the rest of
  // the module used, so existing call sites are unchanged.
  const boxes = treeMap[activeId] ?? [];
  const mailboxes = { status: trees.status, data: boxes, error: trees.error, reload: trees.reload };

  // Keep the push refs current every render (the subscription reads them).
  watchIdRef.current = activeAccount ?? ownIdRef.current;
  reloadRef.current = () => {
    emails.reload();
    mailboxes.reload();
    if (threadId !== null) thread.reload();
  };
  const folderName = flaggedView
    ? strings.flaggedView
    : (boxes.find((b) => b.id === mailboxId)?.name ?? strings.moduleMail);
  const draftsMailboxId =
    boxes.find((b) => b.role === "drafts")?.id ?? mailboxId ?? boxes[0]?.id ?? null;

  // The open conversation's messages, its latest, and the ids actions apply to.
  // In a real folder that's the folder's copies; in the cross-folder Flagged
  // view it's the whole conversation (a message's folder isn't the selection).
  const threadMessages = thread.status === "ready" ? (thread.data ?? []) : [];
  const latest = threadMessages.length > 0 ? threadMessages[threadMessages.length - 1] : undefined;
  const currentFolderIds = flaggedView
    ? threadMessages.map((m) => m.id)
    : mailboxId === null
      ? []
      : threadMessages.filter((m) => m.mailboxIds[mailboxId] === true).map((m) => m.id);

  // Default to the Inbox (or the first mailbox) once folders load.
  useEffect(() => {
    if (mailboxId !== null || mailboxes.status !== "ready") return;
    const list = mailboxes.data ?? [];
    const inbox = list.find((m) => m.role === "inbox") ?? list[0];
    if (inbox !== undefined) setMailboxId(inbox.id);
  }, [mailboxId, mailboxes.status, mailboxes.data]);

  // Toasts self-dismiss.
  useEffect(() => {
    if (toast === null) return undefined;
    const timer = setTimeout(() => setToast(null), 3500);
    return () => clearTimeout(timer);
  }, [toast]);

  // Load the signature + org footer once, for the compose surface.
  useEffect(() => {
    let live = true;
    void client
      .mailSettings()
      .then((s) => {
        if (live) setMailSettings(s);
      })
      .catch(() => {
        // best-effort — compose just opens without a signature
      });
    return () => {
      live = false;
    };
  }, [client]);

  // Load the user's sendable addresses once, for the compose From picker.
  useEffect(() => {
    let live = true;
    void client
      .sendableAddresses()
      .then((list) => {
        if (live) setSendAs(list);
      })
      .catch(() => {
        // best-effort — compose falls back to the signed-in address
      });
    return () => {
      live = false;
    };
  }, [client]);

  // Load the shared mailboxes the user was delegated (ADR 0017).
  useEffect(() => {
    let live = true;
    void client
      .sharedMailboxes()
      .then((list) => {
        if (live) setShared(list);
      })
      .catch(() => {
        // best-effort — no switcher shown if this fails
      });
    return () => {
      live = false;
    };
  }, [client]);

  // Capture the user's own (personal) account id once — the anchor for the own
  // folder tree and the push watch, stable regardless of shared-mailbox switches.
  useEffect(() => {
    void client
      .ownAccountId()
      .then((id) => {
        setOwnId(id);
        ownIdRef.current = id;
        if (watchIdRef.current === null) watchIdRef.current = id;
      })
      .catch(() => undefined);
  }, [client]);

  // Real-time updates: subscribe to the server's push stream and refetch when
  // the account being viewed changes — including changes made by another
  // delegate in a shared mailbox (ADR 0017). Reconnects on drop.
  useEffect(() => {
    const controller = new AbortController();
    let stopped = false;
    let debounce: ReturnType<typeof setTimeout> | null = null;
    const onChange = (ids: string[], delegationChanged: boolean) => {
      if (delegationChanged) {
        // A grant was added or revoked — re-list shared mailboxes so the sidebar
        // mounts/unmounts it. The server has already updated this stream's
        // subscription, so the mailbox's live updates start flowing at once.
        void client
          .sharedMailboxes()
          .then(setShared)
          .catch(() => undefined);
      }
      const watch = watchIdRef.current;
      if (watch === null || !ids.includes(watch)) return;
      if (debounce !== null) clearTimeout(debounce);
      debounce = setTimeout(() => reloadRef.current(), 400);
    };
    async function run() {
      while (!stopped) {
        try {
          await client.subscribeChanges(onChange, controller.signal);
        } catch {
          // failed to open or dropped — fall through to the reconnect backoff
        }
        if (stopped) break;
        await new Promise((r) => setTimeout(r, 3000));
      }
    }
    void run();
    return () => {
      stopped = true;
      controller.abort();
      if (debounce !== null) clearTimeout(debounce);
    };
  }, [client]);

  const afterChange = (message: string) => {
    setToast(message);
    emails.reload();
    mailboxes.reload();
  };
  const fail = () => setToast(strings.mailActionFailed);

  // --- Email → task (ADR 0024) ---------------------------------------------

  /** Direct: create an active task from the open message, carrying its source
   *  link so the task shows "From an email" and can jump back. */
  async function createTaskFromMessage() {
    if (latest === undefined) return;
    try {
      await client.createTask({
        title: latest.subject?.trim() || strings.mailNoSubject,
        sourceKind: "email",
        sourceId: latest.id,
      });
      setToast(strings.taskCreatedFromMail);
    } catch {
      setToast(strings.mailActionFailed);
    }
  }

  /** AI: extract candidate tasks from the message and PROPOSE them — they go to
   *  the Suggestions inbox, never straight on the board (ADR 0023/0024). */
  async function suggestTasksFromMessage() {
    if (latest === undefined) return;
    setToast(strings.taskSuggesting);
    try {
      const text = `${latest.subject ?? ""}\n\n${latest.preview}`.trim();
      const suggested = await client.extractTasks(text);
      if (suggested.length === 0) {
        setToast(strings.taskNoSuggestions);
        return;
      }
      await client.proposeTasks(
        suggested.map((s) => ({
          title: s.title,
          ...(s.dueAt ? { dueAt: s.dueAt } : {}),
          sourceKind: "email",
          sourceId: latest.id,
        })),
      );
      setToast(strings.taskSuggested(suggested.length));
    } catch {
      // AI off / no provider / backend error — the user's mail is untouched.
      setToast(strings.taskAiOff);
    }
  }

  // Jump back from a task's "From an email" link: /mail?open=<messageId> opens
  // the source message's thread (tenant-scoped — a foreign id resolves to
  // nothing), then clears the parameter.
  // /mail?thread=<threadId> is the same door for a CRM deal's linked
  // conversation (B2.07), which knows the conversation and not any one message
  // in it. The thread is read through this user's own account door, so an id
  // they do not hold simply shows nothing — CRM hands over an id, never a right
  // to read it.
  // A search term seeded from elsewhere (the Home search bar → /mail?q=…),
  // handed to the message list as its initial query.
  const [searchSeed, setSearchSeed] = useState("");
  const [searchParams, setSearchParams] = useSearchParams();
  useEffect(() => {
    const open = searchParams.get("open");
    const openThread = searchParams.get("thread");
    const compose = searchParams.get("compose");
    const q = searchParams.get("q");
    if (open === null && openThread === null && compose === null && q === null) return;
    // Home routes "Compose email" and global search into Mail via query params.
    if (compose !== null) setCompose({ mode: "new" });
    if (q !== null) setSearchSeed(q);
    if (openThread !== null) {
      setFlaggedView(false);
      setThreadId(openThread);
    }
    void (async () => {
      if (open !== null) {
        try {
          const email = await client.email(open);
          if (email !== null) {
            const inBox = Object.entries(email.mailboxIds).find(([, v]) => v)?.[0];
            if (inBox !== undefined) setMailboxId(inBox);
            setFlaggedView(false);
            setThreadId(email.threadId);
          }
        } catch {
          /* a foreign or missing id just does nothing */
        }
      }
      const next = new URLSearchParams(searchParams);
      next.delete("open");
      next.delete("thread");
      next.delete("compose");
      next.delete("q");
      setSearchParams(next, { replace: true });
    })();
  }, [searchParams]);

  // Undo send: a created draft is held for a few seconds before it is actually
  // submitted, so a mistaken send can be taken back — Undo just leaves it in
  // Drafts. One send is held at a time.
  const [pendingSend, setPendingSend] = useState<QueuedSend | null>(null);
  const pendingRef = useRef<QueuedSend | null>(null);
  // The account the draft was composed in, captured when the send is queued —
  // switching to another mailbox during the undo window must not re-target
  // the submission (the draft lives where it was written, ADR 0017).
  const pendingAccountRef = useRef<string | null>(null);
  const undoTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  async function flushSend() {
    const queued = pendingRef.current;
    if (queued === null) return;
    const queuedAccount = pendingAccountRef.current;
    pendingRef.current = null;
    pendingAccountRef.current = null;
    setPendingSend(null);
    if (undoTimer.current !== null) {
      clearTimeout(undoTimer.current);
      undoTimer.current = null;
    }
    try {
      await client.submitEmail(
        queued.emailId,
        queued.fromEmail,
        queued.rcpts,
        queuedAccount ?? undefined,
      );
      afterChange(strings.composeSent);
      if (threadId !== null) thread.reload(); // a sent reply joins the open thread
    } catch (error) {
      const reason = mailErrorReason(error);
      setToast(reason === null ? strings.composeSendError : strings.mailSubmitErrorDetail(reason));
    }
  }

  function queueSend(queued: QueuedSend) {
    if (pendingRef.current !== null) void flushSend(); // never hold two at once
    setCompose(null);
    pendingRef.current = queued;
    pendingAccountRef.current = activeAccount ?? ownId;
    setPendingSend(queued);
    undoTimer.current = setTimeout(() => void flushSend(), 5000);
  }

  function undoSend() {
    if (undoTimer.current !== null) {
      clearTimeout(undoTimer.current);
      undoTimer.current = null;
    }
    pendingRef.current = null;
    pendingAccountRef.current = null;
    setPendingSend(null);
    setToast(strings.composeSendUndone);
    emails.reload();
    mailboxes.reload();
  }

  // Don't silently drop a queued send if the module unmounts mid-window.
  const flushRef = useRef(flushSend);
  flushRef.current = flushSend;
  useEffect(() => () => void flushRef.current(), []);

  // Close the mobile folders drawer once a folder/category/flagged view is
  // chosen (the choice is the reason it was opened).
  useEffect(() => {
    setFoldersOpen(false);
  }, [mailboxId, flaggedView, categoryFilter, activeAccount]);

  // Send later: the draft is created; schedule it server-side (it moves to the
  // Scheduled mailbox and a sweeper sends it when due). No Undo window — the
  // Scheduled folder's "Cancel send" is the take-back.
  async function scheduleSend(queued: QueuedSend & { sendAt: number }) {
    setCompose(null);
    try {
      await client.scheduleSend(queued.emailId, queued.fromEmail, queued.rcpts, queued.sendAt);
      afterChange(strings.mailScheduled(formatSendAt(queued.sendAt)));
    } catch (error) {
      const reason = mailErrorReason(error);
      setToast(reason === null ? strings.scheduleError : strings.mailScheduleErrorDetail(reason));
      emails.reload();
      mailboxes.reload();
    }
  }

  // Set (or clear) a label's color, then refresh the folder list.
  async function setLabelColor(id: string, color: string | null) {
    try {
      await client.setMailboxColor(id, color);
      mailboxes.reload();
    } catch {
      fail();
    }
  }

  // Folder management: create (optionally nested), rename, delete.
  async function createFolder(name: string, parentId: string | null) {
    try {
      await client.createMailbox(name, parentId);
      mailboxes.reload();
    } catch {
      setToast(strings.folderActionFailed);
    }
  }
  async function renameFolder(id: string, name: string) {
    try {
      await client.renameMailbox(id, name);
      mailboxes.reload();
    } catch {
      setToast(strings.folderActionFailed);
    }
  }
  async function deleteFolder(box: { id: string; name: string }) {
    if (!(await confirm({ message: strings.folderDeleteConfirm(box.name), danger: true }))) return;
    try {
      await client.deleteMailbox(box.id);
      if (mailboxId === box.id) setMailboxId(null);
      mailboxes.reload();
    } catch {
      setToast(strings.folderActionFailed);
    }
  }

  // Unsubscribe from the open message's mailing list. One-click (RFC 8058) is
  // done server-side (SSRF-guarded); a mailto: list opens a pre-filled compose;
  // a plain link opens the sender's unsubscribe page in a new tab.
  async function unsubscribe() {
    const opts = latest?.["alo:listUnsubscribe"];
    if (latest === undefined || opts == null) return;
    const who = senderName(latest);
    if (opts.oneClick) {
      if (!(await confirm({ message: strings.unsubscribeConfirm(who) }))) return;
      try {
        await client.unsubscribe(latest.id);
        setToast(strings.unsubscribed);
      } catch {
        setToast(strings.unsubscribeFailed);
      }
    } else if (opts.mailto !== null) {
      const { to, subject, body } = parseMailto(opts.mailto);
      setCompose({
        mode: "new",
        to,
        subject: subject ?? strings.unsubscribe,
        ...(body !== undefined ? { body } : {}),
      });
    } else if (opts.http !== null) {
      window.open(opts.http, "_blank", "noopener,noreferrer");
      setToast(strings.unsubscribeOpened);
    }
  }

  // Block a sender: append a server-side rule that files their mail to Junk.
  async function blockSender(email: string) {
    try {
      await client.blockSender(email);
      setToast(strings.senderBlocked(email));
    } catch {
      fail();
    }
  }

  // Cancel a scheduled send: the draft returns to Drafts, editable again.
  async function cancelScheduledSend(emailId: string) {
    try {
      await client.cancelScheduledSend(emailId);
      afterChange(strings.sendCancelled);
      setThreadId(null);
    } catch {
      fail();
    }
  }

  function openMailbox(id: string) {
    setMailboxId(id);
    setCategoryFilter(null);
    setFlaggedView(false);
    setThreadId(null);
  }

  // Switch the whole mail view to a shared mailbox (or back to own, id = null).
  // The client retargets every subsequent call; we reset the selection and let
  // the default-inbox effect pick the new account's inbox (unless `folderId` is
  // given, in which case that folder is opened directly).
  function switchAccount(id: string | null, folderId?: string) {
    if (id === activeAccount) {
      if (folderId !== undefined) openMailbox(folderId);
      return;
    }
    client.setActiveAccountId(id);
    setActiveAccount(id);
    setMailboxId(folderId ?? null);
    setThreadId(null);
    setCategoryFilter(null);
    setFlaggedView(false);
    categories.reload();
  }

  // Select a folder in a specific account (the always-mounted sidebar lists
  // every accessible mailbox's folders); switches accounts first if needed.
  function selectAccountFolder(accountId: string, folderId: string) {
    const target = accountId === ownId ? null : accountId;
    switchAccount(target, folderId);
  }

  // Open the cross-folder Flagged smart view.
  function openFlagged() {
    setFlaggedView(true);
    setCategoryFilter(null);
    setThreadId(null);
  }

  // Filter the current folder to one category (null clears the facet).
  function selectCategory(id: string | null) {
    setCategoryFilter(id);
    setThreadId(null);
  }

  // Category catalog management: create, rename/recolor, delete.
  async function createCategory(name: string, color: string | null) {
    try {
      await client.createCategory(name, color);
      categories.reload();
    } catch {
      setToast(strings.categoryActionFailed);
    }
  }
  async function updateCategory(id: string, name: string, color: string | null) {
    try {
      await client.updateCategory(id, name, color);
      categories.reload();
      emails.reload();
    } catch {
      setToast(strings.categoryActionFailed);
    }
  }
  async function deleteCategory(cat: Category) {
    if (!(await confirm({ message: strings.categoryDeleteConfirm(cat.name), danger: true }))) return;
    try {
      await client.deleteCategory(cat.id);
      if (categoryFilter === cat.id) selectCategory(null);
      categories.reload();
      emails.reload();
    } catch {
      setToast(strings.categoryActionFailed);
    }
  }

  // Tag/untag the whole open conversation with a category (all its messages).
  function toggleThreadCategory(categoryId: string, on: boolean) {
    const ids = threadMessages.map((m) => m.id);
    if (ids.length === 0) return;
    void client
      .setCategoryMany(ids, categoryId, on)
      .then(() => {
        emails.reload();
        thread.reload();
      })
      .catch(fail);
  }

  function openThread(row: ThreadRow) {
    setThreadId(row.threadId);
    if (row.hasUnread) {
      setReadIds((prev) => {
        const next = new Set(prev);
        row.memberIds.forEach((id) => next.add(id));
        return next;
      });
      void client.setSeenMany(row.memberIds, true).catch(() => {
        // Optimistic; the server reconciles on the next folder load.
      });
    }
  }

  function toggleFlag(message: Pick<EmailFull, "id" | "keywords">) {
    const base = message.keywords[KEYWORD_FLAGGED] === true;
    const current = flags.get(message.id) ?? base;
    const next = !current;
    setFlags((prev) => new Map(prev).set(message.id, next));
    void client.setFlagged(message.id, next).catch(() => {
      setFlags((prev) => new Map(prev).set(message.id, current));
    });
    // Unflagging clears any follow-up due-date (the flag is "done").
    if (!next) void client.setFlagDue(message.id, null).catch(() => undefined);
  }

  // Set/clear the follow-up due-date on the open conversation's latest message.
  function setFlagDue(dueAt: number | null) {
    if (latest === undefined) return;
    void client
      .setFlagDue(latest.id, dueAt)
      .then(() => {
        thread.reload();
        emails.reload();
      })
      .catch(fail);
  }

  // Move a set of messages (by id) to another folder. From a real folder this is
  // a source→target membership swap; from the Flagged view the source folder
  // isn't the selection, so we replace membership outright (moveToFolder).
  function moveIds(ids: string[], targetMailboxId: string) {
    if (ids.length === 0) return;
    if (ids.some((id) => currentFolderIds.includes(id))) setThreadId(null);
    if (flaggedView) {
      void client
        .moveToFolder(ids, targetMailboxId)
        .then(() => afterChange(strings.mailMoved))
        .catch(fail);
      return;
    }
    if (mailboxId === null || targetMailboxId === mailboxId) return;
    void client
      .moveMany(ids, mailboxId, targetMailboxId)
      .then(() => afterChange(strings.mailMoved))
      .catch(fail);
  }

  function moveThread(targetMailboxId: string) {
    moveIds(currentFolderIds, targetMailboxId);
  }

  // Archive a set of messages (by id) to the Archive folder. Used by the reading
  // pane (whole open thread) and the list rows (a specific conversation).
  function archiveIds(ids: string[]) {
    const archiveBox = boxes.find((b) => b.role === "archive");
    if (archiveBox === undefined || ids.length === 0 || (!flaggedView && mailboxId === null)) {
      setToast(strings.archiveUnavailable);
      return;
    }
    moveIds(ids, archiveBox.id);
  }

  // Delete a set of messages: to Trash from a normal folder; permanently when
  // already in Trash (or when there is no Trash folder).
  function deleteIds(ids: string[]) {
    if (ids.length === 0 || (!flaggedView && mailboxId === null)) return;
    if (ids.some((id) => currentFolderIds.includes(id))) setThreadId(null);
    const trash = boxes.find((b) => b.role === "trash");
    if (flaggedView) {
      const done = () => afterChange(strings.mailDeleted);
      if (trash === undefined) void client.destroyMany(ids).then(done).catch(fail);
      else void client.moveToFolder(ids, trash.id).then(done).catch(fail);
      return;
    }
    if (mailboxId === null) return; // (unreachable in a real folder; narrows the type)
    if (trash === undefined || mailboxId === trash.id) {
      void client.destroyMany(ids).then(() => afterChange(strings.mailDeleted)).catch(fail);
    } else {
      void client.moveMany(ids, mailboxId, trash.id).then(() => afterChange(strings.mailDeleted)).catch(fail);
    }
  }

  // Mark a set of messages seen/unseen (optimistic; the server reconciles).
  function markSeenIds(ids: string[], seen: boolean) {
    if (ids.length === 0) return;
    setReadIds((prev) => {
      const next = new Set(prev);
      ids.forEach((id) => (seen ? next.add(id) : next.delete(id)));
      return next;
    });
    void client
      .setSeenMany(ids, seen)
      .then(() => {
        emails.reload();
        mailboxes.reload();
      })
      .catch(fail);
  }

  // Snooze a set of messages until `until` (Unix seconds); a server sweeper
  // returns them to the Inbox. Closes the open thread if it's among them.
  function snoozeIds(ids: string[], until: number) {
    // Snooze needs the source folder to restore to, which the cross-folder
    // Flagged view doesn't have; the snooze control is hidden there.
    if (mailboxId === null || flaggedView || ids.length === 0) return;
    if (ids.some((id) => currentFolderIds.includes(id))) setThreadId(null);
    void client
      .snooze(ids, mailboxId, until)
      .then(() => afterChange(strings.mailSnoozed))
      .catch(fail);
  }

  function archiveThread() {
    archiveIds(currentFolderIds);
  }

  function deleteThread() {
    deleteIds(currentFolderIds);
  }

  function markThreadUnread() {
    markSeenIds(currentFolderIds, false);
  }

  // Report spam: move the conversation to Junk; when already in Junk, "Not spam"
  // moves it back to the Inbox.
  function reportSpam() {
    const current = boxes.find((b) => b.id === mailboxId);
    const junk = boxes.find((b) => b.role === "junk");
    const inbox = boxes.find((b) => b.role === "inbox");
    if (current?.role === "junk") {
      if (inbox !== undefined) moveIds(currentFolderIds, inbox.id);
    } else if (junk !== undefined) {
      moveIds(currentFolderIds, junk.id);
    } else {
      setToast(strings.junkUnavailable);
    }
  }

  // Forward the open message as an .eml attachment (a fresh "Fwd:" compose).
  function forwardAttachment() {
    if (latest === undefined) return;
    const base = (latest.subject ?? "message").replace(/[^\w.-]+/g, "_").slice(0, 60);
    setCompose({
      mode: "new",
      subject: `${strings.composeForwardPrefix}${latest.subject ?? ""}`,
      attachments: [
        { blobId: latest.blobId, type: "message/rfc822", name: `${base}.eml`, size: latest.size },
      ],
    });
  }

  const ownLabel = identity?.email ?? strings.sharedMyMailbox;
  // The name of the account whose folders have full management (the active one),
  // and every OTHER accessible mailbox mounted below as a navigation tree.
  const activeLabel = activeShared !== undefined ? activeShared.name : ownLabel;
  // A read-only shared mailbox can be read but not organised — hide its
  // create/rename/delete affordances (the server would refuse them anyway).
  const canManage = activeShared === undefined || !activeShared.readOnly;
  function startCompose() {
    if (activeShared !== undefined && !activeShared.canSend) {
      setToast(strings.sharedNoSend);
      return;
    }
    setCompose({ mode: "new" });
  }
  const otherAccounts = [
    ...(activeAccount !== null && ownId !== null
      ? [{ id: ownId, name: ownLabel, boxes: treeMap[ownId] ?? [], readOnly: false }]
      : []),
    ...shared
      .filter((s) => s.id !== activeAccount)
      .map((s) => ({ id: s.id, name: s.name, boxes: treeMap[s.id] ?? [], readOnly: s.readOnly })),
  ];

  const widthVars = {
    // Collapsed = a compact icon-only column (folders stay one-click reachable).
    "--sidebar-width": foldersCollapsed ? "56px" : `${folders.width}px`,
    "--list-width": `${list.width}px`,
  } as CSSProperties;

  // On mobile: folders live in a drawer, one content pane shows at a
  // time (list until a conversation is opened, then the reading pane).
  const showList = !isMobile || threadId === null;
  const showReading = !isMobile || threadId !== null;

  return (
    <div
      className={styles.mail}
      style={widthVars}
      data-mobile={isMobile ? "true" : undefined}
      data-view={threadId !== null ? "detail" : "list"}
    >
      <ModuleSidebar
        open={foldersOpen}
        onClose={closeFolders}
        label={strings.mailFolders}
      >
      <FolderSidebar
        {...(isMobile ? { className: "!w-full" } : {})}
        mailboxes={mailboxes}
        selectedId={flaggedView ? null : mailboxId}
        collapsed={isMobile ? false : foldersCollapsed}
        flaggedActive={flaggedView}
        onSelectFlagged={openFlagged}
        onSelect={openMailbox}
        activeLabel={activeLabel}
        showAccountHeader={shared.length > 0}
        otherAccounts={otherAccounts}
        onSelectAccount={selectAccountFolder}
        canManage={canManage}
        onCompose={startCompose}
        onDropMessage={moveIds}
        onSetColor={(id, color) => void setLabelColor(id, color)}
        onCreateFolder={(name, parentId) => void createFolder(name, parentId)}
        onRenameFolder={(id, name) => void renameFolder(id, name)}
        onDeleteFolder={(box) => void deleteFolder(box)}
        extraSection={
          <CategorySection
            categories={categoryList}
            selectedId={categoryFilter}
            onSelect={selectCategory}
            onCreate={(name, color) => void createCategory(name, color)}
            onUpdate={(id, name, color) => void updateCategory(id, name, color)}
            onDelete={(cat) => void deleteCategory(cat)}
            canManage={canManage}
          />
        }
      />
      </ModuleSidebar>
      {!isMobile && !foldersCollapsed && (
        <ResizeHandle
          ariaLabel={strings.resizeFolders}
          onResize={folders.applyDelta}
          onCommit={folders.commit}
          onReset={folders.reset}
        />
      )}
      {showList && (
      <MessageList
        folderName={folderName}
        emails={emails}
        initialQuery={searchSeed}
        selectedThreadId={threadId}
        readIds={readIds}
        flagOverrides={flags}
        foldersCollapsed={isMobile ? false : foldersCollapsed}
        onToggleFolders={toggleFolders}
        flat={flatView}
        onToggleView={toggleView}
        categories={categoryList}
        canSnooze={!flaggedView}
        onSelect={openThread}
        onArchive={(ts) => archiveIds(ts.flatMap((t) => t.memberIds))}
        onDelete={(ts) => deleteIds(ts.flatMap((t) => t.memberIds))}
        onMarkRead={(ts, read) => markSeenIds(ts.flatMap((t) => t.memberIds), read)}
        onSnooze={(ts, until) => snoozeIds(ts.flatMap((t) => t.memberIds), until)}
        onToggleFlag={(t) => toggleFlag(t.latest)}
        onCompose={startCompose}
      />
      )}
      {!isMobile && (
      <ResizeHandle
        ariaLabel={strings.resizeMessages}
        onResize={list.applyDelta}
        onCommit={list.commit}
        onReset={list.reset}
      />
      )}
      {showReading && (
      <ReadingPane
        thread={thread}
        mailboxes={boxes}
        currentMailboxId={mailboxId}
        flagOverrides={flags}
        {...(isMobile ? { onBack: () => setThreadId(null) } : {})}
        onReply={() => latest !== undefined && setCompose({ mode: "reply", replyTo: latest })}
        onReplyAll={() => latest !== undefined && setCompose({ mode: "replyAll", replyTo: latest })}
        onForward={() => latest !== undefined && setCompose({ mode: "forward", replyTo: latest })}
        onToggleFlag={() => latest !== undefined && toggleFlag(latest)}
        onArchive={archiveThread}
        onDelete={deleteThread}
        onMove={moveThread}
        onMarkUnread={markThreadUnread}
        onSnooze={(until) => snoozeIds(currentFolderIds, until)}
        onReportSpam={reportSpam}
        onForwardAttachment={forwardAttachment}
        onSmartReply={(text) =>
          latest !== undefined && setCompose({ mode: "reply", replyTo: latest, body: text })
        }
        onCancelSend={() => latest !== undefined && void cancelScheduledSend(latest.id)}
        onBlockSender={(email) => void blockSender(email)}
        isScheduled={!flaggedView && boxes.find((b) => b.id === mailboxId)?.role === "scheduled"}
        isJunk={!flaggedView && boxes.find((b) => b.id === mailboxId)?.role === "junk"}
        categories={categoryList}
        onToggleCategory={toggleThreadCategory}
        onUnsubscribe={() => void unsubscribe()}
        canSnooze={!flaggedView}
        onSetFlagDue={setFlagDue}
        onCreateTask={() => void createTaskFromMessage()}
        onSuggestTasks={() => void suggestTasksFromMessage()}
        onCompose={startCompose}
      />
      )}
      {compose !== null && (
        <ComposeModal
          context={compose}
          fromEmail={activeShared !== undefined ? activeShared.name : (identity?.email ?? "")}
          fromName={activeShared !== undefined ? "" : (identity?.name ?? "")}
          fromOptions={activeShared !== undefined ? [activeShared.name] : sendAs}
          draftsMailboxId={draftsMailboxId}
          signature={mailSettings.signature}
          orgFooter={mailSettings.orgFooter}
          onClose={() => setCompose(null)}
          onQueueSend={queueSend}
          onScheduleSend={scheduleSend}
        />
      )}
      {pendingSend !== null && (
        <div className={cx(styles.toast, styles.undoToast)} role="status">
          <span>{strings.composeUndoWindow}</span>
          <button type="button" className={styles.undoButton} onClick={undoSend}>
            {strings.composeUndoSend}
          </button>
        </div>
      )}
      {toast !== null && pendingSend === null && (
        <div className={styles.toast} role="status">
          {toast}
        </div>
      )}
    </div>
  );
}
