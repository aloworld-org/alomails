// The subset of JMAP (RFC 8620 core, RFC 8621 mail) the web app uses. These
// mirror the wire shapes `alo-jmap` returns; they are the client-side half
// of the contract and change additively with it.

export const MAIL_CAPABILITY = "urn:ietf:params:jmap:mail";
export const CORE_CAPABILITY = "urn:ietf:params:jmap:core";
export const SUBMISSION_CAPABILITY = "urn:ietf:params:jmap:submission";
/** alo extension: user-defined colored message categories (Category/get+set). */
export const CATEGORIES_CAPABILITY = "urn:alo:params:jmap:categories";
/** JMAP Contacts (RFC 9610): the address book (Contact/get+set). */
export const CONTACTS_CAPABILITY = "urn:ietf:params:jmap:contacts";

/** One typed value on a contact — an email or phone with an optional label. */
export interface ContactField {
  kind: string | null;
  value: string;
}

/** An address-book contact, as `Contact/get` returns it. */
export interface Contact {
  id: string;
  name: string;
  firstName: string | null;
  lastName: string | null;
  emails: ContactField[];
  phones: ContactField[];
  organization: string | null;
  jobTitle: string | null;
  notes: string | null;
}

/** The editable fields of a contact (a create/update patch). */
export interface ContactDraft {
  name?: string;
  firstName?: string | null;
  lastName?: string | null;
  emails?: ContactField[];
  phones?: ContactField[];
  organization?: string | null;
  jobTitle?: string | null;
  notes?: string | null;
}

export interface Session {
  apiUrl: string;
  downloadUrl: string;
  uploadUrl: string;
  eventSourceUrl: string;
  /** capability URN → account id */
  primaryAccounts: Record<string, string>;
  state: string;
  /** alo extension: whether AI features are enabled for this tenant. */
  "alo:aiEnabled"?: boolean;
  /** alo extension: whether the signed-in user is a tenant admin. */
  "alo:isAdmin"?: boolean;
  /** alo extension: addresses this user may send from (canonical + aliases). */
  "alo:sendAs"?: string[];
  /** RFC 8620 accounts map: the user's own account plus any shared mailboxes
   * they were delegated (ADR 0017). Keyed by accountId. */
  accounts?: Record<string, SessionAccount>;
}

/** An account in the session's `accounts` map. */
export interface SessionAccount {
  /** Display name (the owner's email for a shared mailbox). */
  name: string;
  /** False for a shared mailbox delegated to this user. */
  isPersonal: boolean;
  isReadOnly: boolean;
  /** alo extension: whether the delegate may send as this mailbox. */
  "alo:canSend"?: boolean;
}

/** A shared mailbox the signed-in user was delegated access to. */
export interface SharedMailbox {
  id: string;
  name: string;
  canSend: boolean;
  readOnly: boolean;
}

/** How a delegate may send from a shared mailbox. */
export type SendMode = "none" | "as" | "on_behalf";

/** A person granted access to a mailbox (ADR 0017). */
export interface Delegate {
  id: string;
  email: string;
  canWrite: boolean;
  sendMode: SendMode;
  /** Per-folder restriction: the mailbox ids they're confined to. Empty = the
   * whole mailbox (no restriction). */
  folders: string[];
}

export interface EmailAddress {
  name: string | null;
  email: string;
}

export interface Mailbox {
  id: string;
  name: string;
  /** JMAP role: "inbox" | "sent" | "drafts" | "trash" | "archive" | "junk" | null */
  role: string | null;
  /** Optional "#rrggbb" label color, or null. */
  color: string | null;
  parentId: string | null;
  sortOrder: number;
  totalEmails: number;
  unreadEmails: number;
}

/** A server-side mail filter (rule). Mirrors the alo-jmap rule model; the
 * server compiles these to a Sieve script that runs at delivery. */
export type FilterField = "from" | "to" | "cc" | "subject";
export type FilterOp = "contains" | "is";
export type FilterMatch = "all" | "any";

export interface FilterCondition {
  field: FilterField;
  op: FilterOp;
  value: string;
}

export type FilterAction =
  | { type: "fileInto"; mailbox: string }
  | { type: "markRead" }
  | { type: "star" }
  | { type: "delete" };

export interface MailFilterRule {
  id: string;
  name: string;
  match: FilterMatch;
  conditions: FilterCondition[];
  actions: FilterAction[];
  enabled: boolean;
}

export interface EmailHeaders {
  id: string;
  threadId: string;
  /** The raw RFC 822 message blob (for "show original", .eml, forward-as-attachment). */
  blobId: string;
  mailboxIds: Record<string, boolean>;
  keywords: Record<string, boolean>;
  from: EmailAddress[] | null;
  to: EmailAddress[] | null;
  cc: EmailAddress[] | null;
  /**
   * Blind-carbon recipients. Populated only on the sender's own (Sent/draft)
   * copy; a received copy always has this empty, so it never discloses another
   * recipient's blind copies.
   */
  bcc: EmailAddress[] | null;
  subject: string | null;
  receivedAt: string;
  size: number;
  preview: string;
  hasAttachment: boolean;
  /** RFC 5322 Message-ID(s), for reply threading. */
  messageId: string[] | null;
  references: string[] | null;
  /**
   * alo's parsed inbound-authentication verdict (non-standard, additive).
   * Absent on outgoing copies; each field is "pass" | "fail" | "none" | etc.
   */
  "alo:authentication"?: MessageAuthentication | null;
  /**
   * alo's parsed List-Unsubscribe options (RFC 2369 / RFC 8058), present only
   * on the full email (reading pane) when the message carries one.
   */
  "alo:listUnsubscribe"?: ListUnsubscribe | null;
  /** A flagged message's follow-up due-date (UTCDate), or null/absent. */
  "alo:flagDue"?: string | null;
  /** An inbound calendar invitation (iMIP REQUEST) parsed from the message's
   * text/calendar part, present only on the full email when the message is an
   * invitation — drives the reading pane's Accept/Decline card. */
  "alo:invitation"?: CalendarInvitation | null;
}

/** A received invitation, summarised for the reading pane. Times are RFC 3339
 * (UTC). RSVP acts on the message's blobId, so no event fields are writable. */
export interface CalendarInvitation {
  /** `REQUEST` (an invitation, shows Accept/Decline), `CANCEL` (the organizer
   *  withdrew it, shows a notice and removes the event), or `REPLY` (a guest
   *  responded — the organizer's copy records it on the event). */
  method: "REQUEST" | "CANCEL" | "REPLY";
  uid: string;
  summary: string;
  organizer: string | null;
  startsAt: string;
  endsAt: string;
  allDay: boolean;
  location: string | null;
  /** For a `REPLY`: the responding guest's email. */
  attendee?: string | null;
  /** For a `REPLY`: their status. */
  partstat?: RsvpResponse | null;
}

/** The reply to an invitation. */
export type RsvpResponse = "accepted" | "declined" | "tentative";

export interface MessageAuthentication {
  spf: string | null;
  dkim: string | null;
  dmarc: string | null;
}

export interface ListUnsubscribe {
  /** The first http(s) unsubscribe URL, if any. */
  http: string | null;
  /** The first mailto: unsubscribe address, if any. */
  mailto: string | null;
  /** True when a silent RFC 8058 one-click POST is supported (https + the
   * List-Unsubscribe-Post header). */
  oneClick: boolean;
}

export interface EmailBodyValue {
  value: string;
  isTruncated: boolean;
}

export interface EmailFull extends EmailHeaders {
  textBody: EmailBodyPart[];
  htmlBody: EmailBodyPart[];
  bodyValues: Record<string, EmailBodyValue>;
  attachments: EmailAttachment[];
}

export interface EmailBodyPart {
  partId: string | null;
  type: string;
}

/** A downloadable attachment on a message (JMAP EmailBodyPart, disposition
 * "attachment"). `blobId` resolves via the session download URL. */
export interface EmailAttachment {
  blobId: string;
  type: string;
  name: string;
  size: number;
  /** Content-ID (no angle brackets) — an HTML `cid:` reference resolves to the
   * inline part with this id, so it renders as an embedded image. */
  cid: string | null;
  /** "inline" (embedded image) or "attachment" (downloadable file). */
  disposition: string;
}

/** One configured AI provider (admin console). The API key is never returned —
 * only whether one is set (`hasKey`). */
export interface AiProvider {
  id: string;
  kind: string;
  label: string;
  baseUrl: string;
  model: string;
  enabled: boolean;
  isDefault: boolean;
  hasKey: boolean;
}

/** A user in the admin console: identity + read-only usage + aliases. */
export interface AdminUser {
  id: string;
  email: string;
  isAdmin: boolean;
  createdAt: string;
  messageCount: number;
  storageBytes: number;
  aliases: string[];
}

/** A group in the admin console. `address` present means it's a distribution
 * list (mail to it fans out to members). */
export interface AdminGroup {
  id: string;
  name: string;
  address: string | null;
  memberCount: number;
  members: { id: string; email: string }[];
}

/** One deliverability check result (admin Security & trust). */
export interface SecurityCheck {
  key: string;
  title: string;
  status: "pass" | "warn" | "fail";
  detail: string;
}

/** One audit-log entry for the admin audit view (ADR 0012). `actor` is the
 * acting user's email, or a label like "operator", or null. */
export interface AuditEntry {
  id: string;
  actor: string | null;
  action: string;
  target: string | null;
  detail: string | null;
  at: string;
}

/** A tenant summary in the platform control plane (ADR 0012). */
export interface ControlTenant {
  id: string;
  name: string;
  status: "active" | "suspended";
  createdAt: string;
  userCount: number;
  storageBytes: number;
  /** Storage cap in bytes, or null for unlimited. */
  storageQuotaBytes: number | null;
}

/** A domain owned by a tenant (control plane). `verifyRecord` is the DNS TXT
 * record to publish to prove ownership. */
export interface ControlDomain {
  domain: string;
  tenantId: string;
  verified: boolean;
  verifiedAt: string | null;
  verifyRecord: { name: string; type: string; value: string };
  createdAt: string;
  /** The active DKIM record to publish (ADR 0014), present once verified; null
   * if no key yet. Only the tenant-admin `/admin/domains` listing includes it. */
  dkim?: { name: string; type: string; value: string; selector: string } | null;
}

/** A JMAP method invocation: [name, arguments, call-id]. */
export type MethodCall = [string, Record<string, unknown>, string];
export type MethodResponse = [string, Record<string, unknown>, string];

export interface JmapRequest {
  using: string[];
  methodCalls: MethodCall[];
}

export interface JmapResponse {
  methodResponses: MethodResponse[];
  sessionState: string;
}

/** JMAP keyword constants we read/set. */
export const KEYWORD_SEEN = "$seen";
export const KEYWORD_FLAGGED = "$flagged";

/** A user-defined message category (alo extension): a colored label a message
 * can be tagged with. Membership is the `$category_<id>` keyword; this is the
 * catalog entry giving that keyword a name and color. */
export interface Category {
  id: string;
  name: string;
  /** "#rrggbb" or null. */
  color: string | null;
  /** The `$category_<id>` keyword a tagged message carries (server-supplied). */
  keyword: string;
}

/** Keyword prefix recording category membership; mirrors the store's
 * `CATEGORY_KEYWORD_PREFIX`. Prefer a Category's own `keyword`; use this only
 * to detect category keywords on a message without the catalog. */
export const CATEGORY_KEYWORD_PREFIX = "$category_";

/** The `$category_<id>` keyword for a category id. */
export function categoryKeyword(id: string): string {
  return `${CATEGORY_KEYWORD_PREFIX}${id}`;
}

/** The category id embedded in a `$category_<id>` keyword, or null. */
export function categoryIdOf(keyword: string): string | null {
  return keyword.startsWith(CATEGORY_KEYWORD_PREFIX)
    ? keyword.slice(CATEGORY_KEYWORD_PREFIX.length)
    : null;
}

/** A calendar (a named collection of events). */
export interface Calendar {
  id: string;
  name: string;
  /** Display colour (hex like `#e76f51`), or null. */
  color: string | null;
  /** `personal` (the default, not deletable) or `shared`. */
  kind: "personal" | "shared";
  /** The viewer's access: `owner` (created it — may share), `editor` (may add
   *  and change events), or `viewer` (read-only). Shared calendars are the ones
   *  where role is not `owner`. */
  role: "owner" | "editor" | "viewer";
}

/** A share on a calendar the viewer owns: who it's shared with, and at what role. */
export interface CalendarGrant {
  /** `user` (a person) or `group` (a team). */
  kind: "user" | "group";
  /** The stored subject id (a user id or group id). */
  subject: string;
  /** Human label — the person's email or the group's name. */
  label: string;
  role: "viewer" | "editor";
}

/** A group the viewer can share a calendar with (team access). */
export interface ShareableGroup {
  id: string;
  name: string;
}

/** One person's free/busy: their busy intervals in the queried window (no event
 *  detail), or `known: false` if the email isn't a user in the tenant. */
export interface FreeBusyPerson {
  email: string;
  known: boolean;
  busy: { start: string; end: string }[];
}

/** A calendar event as it crosses the wire (times are RFC 3339, UTC). */
export interface CalendarEvent {
  id: string;
  /** The calendar this event belongs to. */
  calendarId: string;
  summary: string;
  description: string | null;
  location: string | null;
  startsAt: string;
  endsAt: string;
  allDay: boolean;
  /** iCalendar RRULE (e.g. `FREQ=WEEKLY`) for a recurring event, else null. In
   *  a range listing, occurrences of one series share the master's id + rule. */
  recurrence: string | null;
  /** Guest email addresses invited to the event (empty when there are none).
   *  Saving with guests mails each an iMIP invitation from the owner. */
  attendees: string[];
  /** For an expanded occurrence of a recurring series, its ORIGINAL slot
   *  (RFC 3339) — the stable handle for editing/skipping just that instance,
   *  which differs from `startsAt` once the occurrence has been moved. Null on a
   *  stored master or a one-off. */
  recurrenceId: string | null;
  /** Reminder lead-time in minutes before the start, or null for none. */
  reminderMinutes: number | null;
  /** Organizer's view of who has responded (as guests reply); empty otherwise. */
  attendeeStatus: { email: string; status: string }[];
}

/** The writable fields when creating or replacing an event. */
export interface EventInput {
  summary: string;
  description?: string;
  location?: string;
  startsAt: string;
  endsAt: string;
  allDay: boolean;
  recurrence?: string;
  attendees?: string[];
  /** Which calendar to place the event on; omit for the personal calendar. */
  calendarId?: string;
  /** Reminder lead-time in minutes before the start; omit for no reminder. */
  reminderMinutes?: number;
}

// --- Tasks (ADR 0021–0023) ---------------------------------------------------

/** A task project (board): personal (private) or team (shared). */
export interface TaskProject {
  id: string;
  name: string;
  kind: "personal" | "team";
  color: string | null;
}

export type TaskPriority = "none" | "low" | "medium" | "high";

/** The core task record — one row that both board and list render (ADR 0022). */
export interface Task {
  id: string;
  projectId: string;
  title: string;
  description: string | null;
  /** The board column. */
  status: string;
  position: number;
  /** Assignee's user id and resolved email (null when unassigned). */
  assigneeId: string | null;
  assignee: string | null;
  dueAt: string | null;
  priority: TaskPriority;
  /** `active` work, or an AI `proposed` suggestion awaiting approval. */
  state: "active" | "proposed";
  /** The source link: `email` / `event` + its id (jump-back). */
  sourceKind: string | null;
  sourceId: string | null;
  subtaskDone: number;
  subtaskTotal: number;
  commentCount: number;
  completedAt: string | null;
  createdAt: string;
  /** Labels stamped onto the task (present in list/board responses). */
  labels?: TaskLabelDto[];
}

/** A reusable, tenant-scoped label (tag). */
export interface TaskLabelDto {
  id: string;
  name: string;
  color: string | null;
}

export interface TaskSubtask {
  id: string;
  title: string;
  done: boolean;
}

export interface TaskCommentDto {
  id: string;
  author: string;
  body: string;
  createdAt: string;
}

export interface TaskActivityDto {
  actor: string;
  kind: string;
  detail: unknown;
  createdAt: string;
}

/** A task with its subtasks, comments, and activity (the detail panel). */
export interface TaskAttachmentDto {
  id: string;
  blobId: string;
  filename: string;
  size: number;
  createdAt: string;
}

/** A task attachment rolled up to the project level (with its task). */
export interface ProjectFileDto extends TaskAttachmentDto {
  taskId: string;
  taskTitle: string;
}

/** A "blocked by" reference: another task this one depends on. */
export interface TaskDepRefDto {
  id: string;
  title: string;
  /** The blocker's board column, so the UI can colour it by state. */
  status: string;
}

/** One dependency edge in a project: `blocked` is blocked by `blockedBy`. */
export interface TaskDepEdgeDto {
  blocked: string;
  blockedBy: string;
}

export interface TaskDetailData {
  task: Task;
  subtasks: TaskSubtask[];
  comments: TaskCommentDto[];
  activity: TaskActivityDto[];
  attachments: TaskAttachmentDto[];
  labels: TaskLabelDto[];
  /** Emails of the users following this task. */
  followers: string[];
  /** Whether the current user follows it. */
  following: boolean;
  /** The tasks this one is blocked by. */
  blockedBy: TaskDepRefDto[];
}

/** The writable fields when creating or editing a task. */
export interface TaskInput {
  projectId?: string;
  title: string;
  description?: string;
  status?: string;
  /** Assignee email (resolved to a user in the tenant). */
  assignee?: string;
  dueAt?: string;
  priority?: TaskPriority;
  sourceKind?: string;
  sourceId?: string;
}
