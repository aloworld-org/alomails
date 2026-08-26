// The folder sidebar (Figma app shell): the Compose action, the account's
// system folders with unread counts, and a FOLDERS section for custom
// mailboxes — create, rename (inline), nest (parent/child), color, and delete.
// Selecting a folder drives the message list.
import { useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import {
  Archive,
  CalendarClock,
  ChevronDown,
  ChevronRight,
  Clock,
  FolderPlus,
  Hash,
  Inbox,
  Lock,
  Mails,
  MoreHorizontal,
  PenLine,
  Pencil,
  Plus,
  Send,
  ShieldAlert,
  Star,
  Trash2,
  FileText,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { strings } from "../../i18n";
import { cx } from "../../ds";
import type { Mailbox } from "../../jmap";
import type { Async } from "../state/useAsync";
import { DRAG_EMAIL_MIME } from "../dnd";
import styles from "./FolderSidebar.module.css";

const ROLE_ICON: Record<string, LucideIcon> = {
  inbox: Inbox,
  snoozed: Clock,
  drafts: FileText,
  scheduled: CalendarClock,
  sent: Send,
  archive: Archive,
  junk: ShieldAlert,
  trash: Trash2,
};

const ROLE_ORDER: Record<string, number> = {
  inbox: 0,
  snoozed: 1,
  drafts: 2,
  scheduled: 3,
  sent: 4,
  archive: 5,
  junk: 6,
  trash: 7,
};

function systemFolders(list: Mailbox[]): Mailbox[] {
  return list
    .filter((m) => m.role !== null)
    .sort((a, b) => (ROLE_ORDER[a.role ?? ""] ?? 50) - (ROLE_ORDER[b.role ?? ""] ?? 50));
}

/** Custom folders in tree order (parent before children), each with its depth. */
function nestCustom(list: Mailbox[]): { box: Mailbox; depth: number }[] {
  const custom = list.filter((m) => m.role === null);
  const ids = new Set(custom.map((m) => m.id));
  const byParent = new Map<string | null, Mailbox[]>();
  for (const m of custom) {
    const p = m.parentId !== null && ids.has(m.parentId) ? m.parentId : null;
    const arr = byParent.get(p) ?? [];
    arr.push(m);
    byParent.set(p, arr);
  }
  for (const arr of byParent.values()) {
    arr.sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name));
  }
  const out: { box: Mailbox; depth: number }[] = [];
  const walk = (parent: string | null, depth: number) => {
    for (const m of byParent.get(parent) ?? []) {
      out.push({ box: m, depth });
      walk(m.id, depth + 1);
    }
  };
  walk(null, 0);
  return out;
}

/** The label color palette (warm-workshop hues + a few universals). */
const LABEL_COLORS = [
  "#5b8a72", "#3f7cac", "#7b6cae", "#c07a3e",
  "#c0603e", "#b03a4b", "#4c9a8f", "#8a8f3a",
];

/** An accessible mailbox other than the active one — mounted below the active
 * account's folders as a navigation tree (Outlook-style always-mounted). */
export interface OtherAccount {
  id: string;
  name: string;
  boxes: Mailbox[];
  readOnly: boolean;
}

interface FolderSidebarProps {
  mailboxes: Async<Mailbox[]>;
  selectedId: string | null;
  collapsed: boolean;
  /** Extra class on the root (the mail module uses it for the mobile drawer). */
  className?: string;
  /** Name of the active account (own email or shared mailbox name), shown as a
   * header above its folders when other mailboxes are mounted. */
  activeLabel: string;
  /** Whether to show the active-account header (true when shared mailboxes exist). */
  showAccountHeader: boolean;
  /** Other accessible mailboxes, each mounted as a navigation tree below. */
  otherAccounts: OtherAccount[];
  /** Open a folder in one of the other mailboxes (switches the active account). */
  onSelectAccount: (accountId: string, mailboxId: string) => void;
  /** Whether the active account may be managed — false for a read-only shared
   * mailbox, which hides create/rename/delete/colour affordances. */
  canManage: boolean;
  onSelect: (id: string) => void;
  onCompose: () => void;
  onDropMessage: (emailIds: string[], mailboxId: string) => void;
  onSetColor: (mailboxId: string, color: string | null) => void;
  /** Create a folder (optionally nested under `parentId`). */
  onCreateFolder: (name: string, parentId: string | null) => void;
  onRenameFolder: (id: string, name: string) => void;
  onDeleteFolder: (box: Mailbox) => void;
  /** Whether the cross-folder Flagged smart view is the active selection. */
  flaggedActive: boolean;
  onSelectFlagged: () => void;
  /** Rendered below the folder list (the Categories section). Hidden when the
   * sidebar is collapsed, alongside the other labels. */
  extraSection?: ReactNode;
}

export function FolderSidebar({
  mailboxes,
  selectedId,
  collapsed,
  className,
  activeLabel,
  showAccountHeader,
  otherAccounts,
  onSelectAccount,
  canManage,
  onSelect,
  onCompose,
  onDropMessage,
  onSetColor,
  onCreateFolder,
  onRenameFolder,
  onDeleteFolder,
  flaggedActive,
  onSelectFlagged,
  extraSection,
}: FolderSidebarProps) {
  const [dragOverId, setDragOverId] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ box: Mailbox; x: number; y: number } | null>(null);
  const [editing, setEditing] = useState<{ id: string; value: string } | null>(null);
  // A pending new folder, with the parent it nests under (null = root).
  const [creating, setCreating] = useState<{ parentId: string | null; value: string } | null>(null);
  // Which mounted "other mailbox" trees are collapsed (default expanded).
  const [collapsedAccounts, setCollapsedAccounts] = useState<ReadonlySet<string>>(new Set());
  function toggleAccount(id: string) {
    setCollapsedAccounts((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function commitRename() {
    if (editing !== null && editing.value.trim().length > 0) {
      onRenameFolder(editing.id, editing.value.trim());
    }
    setEditing(null);
  }
  function commitCreate() {
    if (creating !== null && creating.value.trim().length > 0) {
      onCreateFolder(creating.value.trim(), creating.parentId);
    }
    setCreating(null);
  }
  function onEditKey(e: KeyboardEvent<HTMLInputElement>, commit: () => void, cancel: () => void) {
    if (e.key === "Enter") commit();
    else if (e.key === "Escape") cancel();
  }

  function row(box: Mailbox, leading: ReactNode, opts?: { colorable?: boolean; depth?: number }) {
    const active = box.id === selectedId;
    const depth = opts?.depth ?? 0;
    if (editing?.id === box.id) {
      return (
        <div key={box.id} className={styles.item} style={{ paddingLeft: 12 + depth * 14 }}>
          {leading}
          <input
            className={styles.rename}
            value={editing.value}
            autoFocus
            onChange={(e) => setEditing({ id: box.id, value: e.target.value })}
            onBlur={commitRename}
            onKeyDown={(e) => onEditKey(e, commitRename, () => setEditing(null))}
            aria-label={strings.folderRename}
          />
        </div>
      );
    }
    return (
      <button
        key={box.id}
        type="button"
        className={cx(styles.item, active && styles.active, dragOverId === box.id && styles.dropTarget)}
        style={depth > 0 ? { paddingLeft: 12 + depth * 14 } : undefined}
        onClick={() => onSelect(box.id)}
        aria-current={active ? "true" : undefined}
        title={box.name}
        onContextMenu={
          opts?.colorable && canManage
            ? (e) => {
                e.preventDefault();
                setMenu({ box, x: e.clientX, y: e.clientY });
              }
            : undefined
        }
        onDragOver={(e) => {
          if (e.dataTransfer.types.includes(DRAG_EMAIL_MIME)) {
            e.preventDefault();
            e.dataTransfer.dropEffect = "move";
          }
        }}
        onDragEnter={() => setDragOverId(box.id)}
        onDragLeave={(e) => {
          if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDragOverId(null);
        }}
        onDrop={(e) => {
          e.preventDefault();
          const ids = e.dataTransfer.getData(DRAG_EMAIL_MIME).split(",").filter((s) => s !== "");
          setDragOverId(null);
          if (ids.length > 0) onDropMessage(ids, box.id);
        }}
      >
        {leading}
        <span className={styles.name}>{box.name}</span>
        {box.unreadEmails > 0 && <span className={styles.count}>{box.unreadEmails}</span>}
      </button>
    );
  }

  function roleIcon(box: Mailbox): ReactNode {
    const Icon = (box.role !== null ? ROLE_ICON[box.role] : undefined) ?? Hash;
    return <Icon className={styles.icon} strokeWidth={1.75} />;
  }

  // A navigation-only folder row for a mounted "other" mailbox — selecting it
  // switches the active account to that mailbox and opens the folder.
  function otherRow(accountId: string, box: Mailbox, leading: ReactNode, depth = 0): ReactNode {
    return (
      <button
        key={box.id}
        type="button"
        className={styles.item}
        style={depth > 0 ? { paddingLeft: 12 + depth * 14 } : undefined}
        onClick={() => onSelectAccount(accountId, box.id)}
        title={box.name}
      >
        {leading}
        <span className={styles.name}>{box.name}</span>
        {box.unreadEmails > 0 && <span className={styles.count}>{box.unreadEmails}</span>}
      </button>
    );
  }

  function labelDot(box: Mailbox): ReactNode {
    return (
      <span
        className={styles.dot}
        style={box.color !== null ? { background: box.color } : undefined}
        aria-hidden
      />
    );
  }

  function newFolderInput(parentId: string | null, depth: number) {
    return (
      <div className={styles.item} style={{ paddingLeft: 12 + depth * 14 }}>
        <span className={styles.dot} aria-hidden />
        <input
          className={styles.rename}
          value={creating?.value ?? ""}
          autoFocus
          placeholder={strings.folderNamePlaceholder}
          onChange={(e) => setCreating({ parentId, value: e.target.value })}
          onBlur={commitCreate}
          onKeyDown={(e) => onEditKey(e, commitCreate, () => setCreating(null))}
          aria-label={strings.folderNew}
        />
      </div>
    );
  }

  const system = systemFolders(mailboxes.data ?? []);
  const custom = nestCustom(mailboxes.data ?? []);

  return (
    <nav
      className={cx(styles.sidebar, collapsed && styles.collapsed, className)}
      aria-label={strings.mailFolders}
    >
      <button type="button" className={styles.compose} onClick={onCompose} title={strings.compose}>
        <PenLine size={17} strokeWidth={2} />
        <span className={styles.composeLabel}>{strings.compose}</span>
      </button>

      {mailboxes.status === "loading" && (
        <div className={styles.folderSkeleton} role="status" aria-label={strings.mailLoading} aria-busy="true">
          {Array.from({ length: 6 }, (_, index) => <span key={index} className={styles.folderSkeletonRow} />)}
        </div>
      )}

      {mailboxes.status === "error" && (
        <div className={styles.state}>
          <p>{strings.mailFolderError}</p>
          <button type="button" className={styles.retry} onClick={mailboxes.reload}>
            {strings.mailRetry}
          </button>
        </div>
      )}

      {mailboxes.status === "ready" && (
        <div className={styles.scroll}>
          {!collapsed && showAccountHeader && (
            <div className={styles.accountHead} title={activeLabel}>
              <Mails size={14} className={styles.accountIcon} strokeWidth={1.75} />
              <span className={styles.accountName}>{activeLabel}</span>
            </div>
          )}
          <div className={styles.group}>
            {system.map((box) => {
              const Icon = (box.role !== null ? ROLE_ICON[box.role] : undefined) ?? Hash;
              return row(box, <Icon className={styles.icon} strokeWidth={1.75} />);
            })}
            {/* The cross-folder Flagged smart view — a virtual folder, so it
                sits with the system folders but drives its own selection. */}
            <button
              type="button"
              className={cx(styles.item, flaggedActive && styles.active)}
              onClick={onSelectFlagged}
              aria-current={flaggedActive ? "true" : undefined}
              title={strings.flaggedView}
            >
              <Star className={styles.icon} strokeWidth={1.75} />
              <span className={styles.name}>{strings.flaggedView}</span>
            </button>
          </div>
          <div className={styles.group}>
            <div className={styles.groupHead}>
              <h2 className={styles.heading}>{strings.mailFolders}</h2>
              {canManage && (
                <button
                  type="button"
                  className={styles.newFolder}
                  onClick={() => setCreating({ parentId: null, value: "" })}
                  title={strings.folderNew}
                  aria-label={strings.folderNew}
                >
                  <FolderPlus size={15} />
                </button>
              )}
            </div>
            {custom.map(({ box, depth }) => (
              <div key={box.id} className={styles.rowWrap}>
                {row(box, labelDot(box), { colorable: true, depth })}
                {canManage && editing?.id !== box.id && (
                  <button
                    type="button"
                    className={styles.kebab}
                    aria-label={strings.folderActions(box.name)}
                    title={strings.folderActions(box.name)}
                    onClick={(e) => {
                      e.stopPropagation();
                      const r = e.currentTarget.getBoundingClientRect();
                      setMenu({ box, x: r.right, y: r.bottom });
                    }}
                  >
                    <MoreHorizontal size={15} />
                  </button>
                )}
                {creating?.parentId === box.id && newFolderInput(box.id, depth + 1)}
              </div>
            ))}
            {creating?.parentId === null && newFolderInput(null, 0)}
          </div>
          {!collapsed && extraSection}
          {!collapsed && otherAccounts.length > 0 && (
            <div className={styles.group}>
              <h2 className={styles.heading}>{strings.sharedMailboxesHeading}</h2>
              {otherAccounts.map((acct) => {
                const acctCollapsed = collapsedAccounts.has(acct.id);
                return (
                  <div key={acct.id} className={styles.account}>
                    <button
                      type="button"
                      className={styles.accountToggle}
                      onClick={() => toggleAccount(acct.id)}
                      aria-expanded={!acctCollapsed}
                      title={acct.name}
                    >
                      {acctCollapsed ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
                      <Mails size={14} className={styles.accountIcon} strokeWidth={1.75} />
                      <span className={styles.accountName}>{acct.name}</span>
                      {acct.readOnly && (
                        <Lock size={11} className={styles.accountLock} aria-label={strings.sharedReadOnly} />
                      )}
                    </button>
                    {!acctCollapsed && (
                      <div className={styles.accountFolders}>
                        {systemFolders(acct.boxes).map((box) => otherRow(acct.id, box, roleIcon(box)))}
                        {nestCustom(acct.boxes).map(({ box, depth }) =>
                          otherRow(acct.id, box, labelDot(box), depth),
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}

      {menu !== null && (
        <>
          <button
            type="button"
            className={styles.pickerScrim}
            aria-hidden
            tabIndex={-1}
            onClick={() => setMenu(null)}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu(null);
            }}
          />
          <div
            className={styles.palette}
            role="menu"
            aria-label={menu.box.name}
            style={{ left: Math.min(menu.x, window.innerWidth - 200), top: menu.y }}
          >
            <button
              type="button"
              className={`${styles.menuItem} hover:!bg-accent-soft hover:!text-accent`}
              onClick={() => {
                setCreating({ parentId: menu.box.id, value: "" });
                setMenu(null);
              }}
            >
              <Plus size={14} />
              {strings.folderNewSub}
            </button>
            <button
              type="button"
              className={`${styles.menuItem} hover:!bg-accent-soft hover:!text-accent`}
              onClick={() => {
                setEditing({ id: menu.box.id, value: menu.box.name });
                setMenu(null);
              }}
            >
              <Pencil size={14} />
              {strings.folderRename}
            </button>
            <div className={styles.menuDivider} />
            <span className={styles.paletteHead}>{strings.labelColor}</span>
            <div className={styles.swatches}>
              {LABEL_COLORS.map((c) => (
                <button
                  key={c}
                  type="button"
                  className={styles.swatch}
                  style={{ background: c }}
                  aria-label={c}
                  onClick={() => {
                    onSetColor(menu.box.id, c);
                    setMenu(null);
                  }}
                />
              ))}
            </div>
            <button
              type="button"
              className={styles.clearColor}
              onClick={() => {
                onSetColor(menu.box.id, null);
                setMenu(null);
              }}
            >
              {strings.labelColorClear}
            </button>
            <div className={styles.menuDivider} />
            <button
              type="button"
              className={cx(styles.menuItem, styles.menuDanger)}
              onClick={() => {
                onDeleteFolder(menu.box);
                setMenu(null);
              }}
            >
              <Trash2 size={14} />
              {strings.folderDelete}
            </button>
          </div>
        </>
      )}
    </nav>
  );
}
