// Grouping a folder's messages into conversation rows for the list. Pure and
// unit-tested; the reading pane loads the full cross-folder thread separately.
import { KEYWORD_FLAGGED, type EmailHeaders } from "../jmap";
import { isUnread } from "./format";

export interface ThreadRow {
  threadId: string;
  /** The most recent message in this thread (drives the row's summary). */
  latest: EmailHeaders;
  /** Ids of this thread's messages within the current folder. */
  memberIds: string[];
  count: number;
  hasUnread: boolean;
  hasFlagged: boolean;
  hasAttachment: boolean;
}

/** Groups messages by thread, one row per conversation, newest thread first. */
export function groupThreads(
  emails: EmailHeaders[],
  readIds: ReadonlySet<string>,
  flagOverrides: ReadonlyMap<string, boolean>,
): ThreadRow[] {
  const groups = new Map<string, EmailHeaders[]>();
  for (const email of emails) {
    const existing = groups.get(email.threadId);
    if (existing === undefined) groups.set(email.threadId, [email]);
    else existing.push(email);
  }

  const rows: ThreadRow[] = [];
  for (const [threadId, members] of groups) {
    const latest = members.reduce((a, b) => (b.receivedAt > a.receivedAt ? b : a));
    rows.push({
      threadId,
      latest,
      memberIds: members.map((m) => m.id),
      count: members.length,
      hasUnread: members.some((m) => isUnread(m) && !readIds.has(m.id)),
      hasFlagged: members.some(
        (m) => flagOverrides.get(m.id) ?? m.keywords[KEYWORD_FLAGGED] === true,
      ),
      hasAttachment: members.some((m) => m.hasAttachment),
    });
  }

  return rows.sort((a, b) => b.latest.receivedAt.localeCompare(a.latest.receivedAt));
}

/** Flat (non-conversation) view: one row per message, newest first. Each row is
 * a single-message "thread" so the list renders identically. */
export function flatRows(
  emails: EmailHeaders[],
  readIds: ReadonlySet<string>,
  flagOverrides: ReadonlyMap<string, boolean>,
): ThreadRow[] {
  return emails
    .map((m) => ({
      threadId: m.threadId,
      latest: m,
      memberIds: [m.id],
      count: 1,
      hasUnread: isUnread(m) && !readIds.has(m.id),
      hasFlagged: flagOverrides.get(m.id) ?? m.keywords[KEYWORD_FLAGGED] === true,
      hasAttachment: m.hasAttachment,
    }))
    .sort((a, b) => b.latest.receivedAt.localeCompare(a.latest.receivedAt));
}
