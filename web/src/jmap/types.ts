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
  /** alo extension: the tenant-wide scoped roles this user holds (ADR 0035,
   * B4.12) — separate from `alo:isAdmin`, which is the console. Absent on a
   * server that predates the roles, which reads as "none". */
  "alo:roles"?: TenantRole[];
  /** alo extension: the rail modules a tenant admin has switched off for this
   * person (migration 0208). The shell hides them, because offering an app
   * that answers 403 is worse than not offering it — but the server refuses
   * the routes regardless, and a client is never an access decision. Absent
   * on a server that predates the switches, which reads as "none denied".
   *
   * Always empty for an admin, who is never denied. */
  "alo:deniedModules"?: AppModuleId[];
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

/** A tenant-wide scoped role (ADR 0035). Two today, and neither implies the
 * other:
 *
 * - `accountant` (B4.12) — the books and nothing else: every finance report,
 *   the approvals inbox and the period lock, billing and CRM read-only, no
 *   admin console.
 * - `hr` (B6) — the workforce: the directory including its private fields,
 *   leave decisions, hiring and the payroll export. An external bookkeeper
 *   reading everybody's contract is exactly the failure `accountant` exists to
 *   prevent, so somebody who genuinely runs both is granted both. */
export type TenantRole = "accountant" | "hr";

/** A rail module whose access an admin can switch off per person
 * (migration 0208).
 *
 * Mail and Home are absent and cannot be switched off: `/jmap` carries the
 * session, uploads and the event stream every other surface needs, so a
 * denial there would read as a broken login rather than as a missing app.
 *
 * The same set, spelled the same way, as the store's CHECK and the API's
 * route table — those two hold each other honest in Rust; this one is the
 * third copy and is why `AppModuleId` is a union rather than `string`. */
export type AppModuleId =
  | "agenda"
  | "billing"
  | "chat"
  | "crm"
  | "drive"
  | "finance"
  | "hr"
  | "insights"
  | "inventory"
  | "meet"
  | "projects"
  | "sites"
  | "tasks";

/** One app switch as the admin console reads it: the module, and whether this
 * person has it. Reports what was **stored** — an admin is never denied at the
 * gate, but their own switches are shown as they were set. */
export interface UserModuleAccess {
  id: AppModuleId;
  allowed: boolean;
}

/** A user in the admin console: identity + read-only usage + aliases + the
 * scoped roles they hold (separate from `isAdmin`, which is the console). */
export interface AdminUser {
  id: string;
  email: string;
  isAdmin: boolean;
  roles: TenantRole[];
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

// ---- Spaces (ADR 0026) ------------------------------------------------------

export type SpaceRole = "viewer" | "editor" | "manager";

/** A Space the caller belongs to, with their own role. */
export interface SpaceDto {
  id: string;
  name: string;
  archived: boolean;
  myRole: SpaceRole;
  createdAt: string;
}

/** One membership row. */
export interface SpaceMemberDto {
  userId: string;
  email: string | null;
  role: SpaceRole;
  addedAt: string;
}

/** A Space with its membership and enabled modules (the detail view). */
export interface SpaceDetailDto {
  space: SpaceDto;
  members: SpaceMemberDto[];
  modules: string[];
}

// ---- Drive (ADR 0027) -------------------------------------------------------

export type DriveNodeKind =
  "folder" | "file" | "doc" | "sheet" | "slides" | "base";

/** A node in the Drive tree. `space` is the Space id when it lives in a Space,
 *  or null for the caller's personal My Files. */
export interface DriveNodeDto {
  id: string;
  parentId: string | null;
  space: string | null;
  kind: DriveNodeKind;
  name: string;
  blobId: string | null;
  size: number;
  contentType: string | null;
  trashed: boolean;
  sourceKind: string | null;
  sourceId: string | null;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

/** One entry in a node's version history. */
export interface DriveVersionDto {
  versionNo: number;
  blobId: string;
  size: number;
  createdBy: string;
  createdAt: string;
}

// ---- alo Base (ADR 0032) ----------------------------------------------------

export type BaseFieldType =
  | "text"
  | "number"
  | "date"
  | "checkbox"
  | "select"
  | "multiselect"
  | "attachment"
  | "person"
  | "link";

export type BaseViewKind = "grid" | "board" | "calendar" | "gallery";

export interface BaseFieldDto {
  id: string;
  name: string;
  type: BaseFieldType;
  options: Record<string, unknown>;
}

export interface BaseViewDto {
  id: string;
  kind: BaseViewKind;
  name: string;
  config: Record<string, unknown>;
}

export interface BaseRecordDto {
  id: string;
  /** Cell values keyed by field id. */
  cells: Record<string, unknown>;
}

export interface BaseTableDto {
  id: string;
  name: string;
  fields: BaseFieldDto[];
  views: BaseViewDto[];
  records: BaseRecordDto[];
}

/** A whole Base: its tables, each with fields, views, and records. */
export interface BaseDto {
  nodeId: string;
  tables: BaseTableDto[];
}

// ---- Workspace search (ADR 0029) --------------------------------------------

/** One search result: a Drive node (kind folder/file/doc/base), a task, or a
 * mail message (kind "message"). */
export interface SearchHitDto {
  kind: string;
  id: string;
  title: string;
  /** The Space id when it's a Space file, else null (personal / task / mail). */
  space: string | null;
}

/** Answer from "ask your workspace" (ADR 0029). `answer` is the cited text, or
 * null when no model produced one; `reason` says why (no model configured, or
 * the backend was unreachable). `sources` — the access-scoped matches — are
 * always present, so the UI shows results even when the AI half is unavailable. */
export interface AiAnswerDto {
  answer: string | null;
  reason: "unconfigured" | "unreachable" | null;
  sources: SearchHitDto[];
}

/** An action the "Ask alo" agent proposes (ADR 0034) — shown for approval, run
 *  only via `executeAgentAction`. `say` is a one-line human description. */
export interface AgentActionDto {
  tool: string;
  args: Record<string, unknown>;
  say: string;
}

/** The agent's reply: an answer, or a proposed action, over the same
 *  access-scoped sources as Ask AI. Exactly one of `answer` / `action` is set
 *  (both null when AI is off/unreachable — `reason` says which). */
export interface AgentAnswerDto {
  answer: string | null;
  action: AgentActionDto | null;
  reason: "unconfigured" | "unreachable" | null;
  sources: SearchHitDto[];
}

/** Result of executing an approved agent action. Most tools answer only the
 *  record they touched; the Projects tools (B3.10a, B3.10b) answer more, and their
 *  shapes are spelled out below so the UI can render them without guessing. */
export interface AgentExecuteResultDto {
  ok: boolean;
  result: AgentResultDto;
}

/** The record an executed tool produced. `kind` is open — a tool the client does
 *  not recognise still confirms cleanly — so the rich shapes are narrowed by
 *  the guards in `AgentResultCard`, never by the type alone. */
export interface AgentResultDto {
  kind: string;
  /** The record the tool touched. Absent for a tool that touched a *set* of
   *  them (`categorise_transactions` suggests on many claims at once), which is
   *  why it is optional rather than an empty string standing in for nothing. */
  id?: string;
  title?: string;
}

/** What `log_time` wrote: a *proposed* timesheet entry (ADR 0023). It is in no
 *  total until the person whose timesheet it is accepts it in Projects. */
export interface TimeEntryResultDto extends AgentResultDto {
  workDate: string;
  minutes: number;
  billable: boolean;
  note: string;
  proposed: boolean;
}

/** What `draft_timesheet_from_calendar` wrote: a batch of *proposed* entries
 *  drafted from the caller's own Agenda (B3.10b), and — just as much part of the
 *  answer — the meetings it left out, each with a machine-readable reason the
 *  catalogue writes words for. An empty `drafted` with a full `skipped` is a
 *  real and useful reply; an empty pair means the diary held nothing. */
export interface TimesheetDraftResultDto extends AgentResultDto {
  from: string;
  to: string;
  drafted: {
    id: string;
    workDate: string;
    minutes: number;
    note: string;
    /** Sits on top of the meeting drafted before it. Flagged, never resolved:
     *  which of two double-booked calls was the work is the user's to say. */
    overlaps: boolean;
  }[];
  /** The batch's own total, as the server counted it. */
  minutes: number;
  /** How many of the drafted entries overlap the one before them. */
  overlaps: number;
  billable: boolean;
  skipped: { summary: string; day: string; reason: string }[];
}

/** What `categorise_transactions` wrote (B4.14a): one *suggested* category per
 *  unclassified claim, each waiting for the claimant to accept or decline it —
 *  and, just as much part of the answer, the claims it suggested nothing for,
 *  with a machine-readable reason the catalogue writes words for.
 *
 *  Nothing here is classified: `categoryId` on the claim itself is untouched
 *  until a person accepts, which is what keeps a guess out of the books. */
export interface CategoryProposalsResultDto extends AgentResultDto {
  from: string;
  to: string;
  proposed: {
    /** The claim the suggestion is about. */
    id: string;
    merchant: string | null;
    spentOn: string | null;
    grossCents: number | null;
    currency: string | null;
    categoryId: string;
    /** The tenant's own word for it; `null` only if it was retired mid-flight. */
    categoryName: string | null;
    reason: string;
    /** How many of the claimant's own past claims back it — the argument for
     *  the suggestion, and the reason it is worth showing at all. */
    evidence: number;
  }[];
  skipped: {
    id: string;
    merchant: string | null;
    spentOn: string | null;
    reason: string;
  }[];
  /** How many suggestions were written, as the server counted them. */
  suggested: number;
  /** How many claims were looked at in total. */
  considered: number;
}

/** One side of the VAT figures the books carry — the sales side or the purchase
 *  side. Rates the period did not use are simply absent; `unrated*` is turnover
 *  or cost on no line of the return, reported apart from the totals because its
 *  absence would read as "the question does not arise" when it means "none". */
export interface VatSummarySideDto {
  rates: { rateBp: number; baseCents: number; vatCents: number }[];
  baseCents: number;
  vatCents: number;
  unratedBaseCents: number;
  unratedVatCents: number;
}

/** What `vat_summary` read (B4.14b): the same figures `GET /finance/reports/vat`
 *  answers, in the same shape — the agent is another reader of that report, not
 *  a second rendering of it. `netPayableCents` is positive when the tenant owes
 *  the authority and negative when it is owed a refund; the card says which in
 *  words, the number carries the sign either way.
 *
 *  These are **figures for a return, not a return**: nothing is filed anywhere. */
export interface VatSummaryResultDto extends AgentResultDto {
  from: string;
  to: string;
  /** The tenant's accounting currency — every figure here is in it. */
  currency: string;
  output: VatSummarySideDto;
  input: VatSummarySideDto;
  netPayableCents: number;
}

/** One thing worth a second look, with the entries that caused it.
 *
 *  `entries` is the whole of the argument for a finding: an unexplained flag is
 *  an accusation, so a card never shows one without them. Nothing here names a
 *  person — the server's rules never read a posting's user — and there is no
 *  score, no ranking and no confidence anywhere in the shape, because a number
 *  attached to a suspicion is read as evidence for it. */
export interface AnomalyFindingDto {
  /** `duplicate`, `unusualAmount` or `missingRecurring`. Open: a kind a newer
   *  server knows still renders, as "worth a look". */
  kind: string;
  accountId: string;
  /** The chart's own code and name, or `null` for an account outside the chart
   *  this read returned — an id is never shown as if it were a name. */
  accountCode: string | null;
  accountName: string | null;
  /** The other side of the transaction, when the postings name one. */
  counterparty: { kind: string; id: string; name: string | null } | null;
  /** The amount the finding is about, in the scan's currency. */
  amountCents: number;
  /** What the account, or the cost, usually moves — what the amount above is
   *  unusual against. `null` where the rule makes no comparison. */
  typicalCents: number | null;
  /** The first day of the month nothing was booked in (`missingRecurring`). */
  missingMonth: string | null;
  entries: {
    id: string;
    entryDate: string;
    entryKind: string;
    memo: string;
    amountCents: number;
  }[];
}

/** What `flag_anomalies` read (B4.14b). It wrote nothing: there is no anomaly
 *  record, no "reviewed" flag and no dismissal — the answer to a finding is a
 *  correcting entry in the journal.
 *
 *  `found` is what the scan found and `shown` is how many are in the list;
 *  `truncated` says the period holds more entries than one scan carries, and
 *  `notComparable` how many entries name no counterparty and so could not be
 *  compared for duplication. Silence would read as "nothing was wrong" when
 *  what it means is "I stopped looking". */
export interface JournalAnomaliesResultDto extends AgentResultDto {
  from: string;
  to: string;
  currency: string;
  findings: AnomalyFindingDto[];
  found: number;
  shown: number;
  scanned: number;
  truncated: boolean;
  notComparable: number;
}

/** What `reorder_proposals` wrote (B5.10): one **draft** purchase order per
 *  supplier for everything the tenant is under their own minimum on — and, just
 *  as much part of the answer, the shortages it ordered nothing for.
 *
 *  Nothing here has been sent: a draft carries no `number` and no `orderedDate`,
 *  and drawing both is the send button on the purchase-orders screen. Every
 *  figure — quantities, prices, totals — is the server's; the browser adds up
 *  none of it.
 *
 *  `supplier` and `location` echo what the run was narrowed to, so the card can
 *  say what it looked at rather than implying it looked everywhere. */
export interface ReorderProposalsResultDto extends AgentResultDto {
  supplier: { supplierId: string; supplierName: string } | null;
  location: {
    locationId: string;
    locationCode: string;
    locationName: string;
  } | null;
  /** The drafts. The server answers each in the full shape the purchase-order
   *  screens read; spelled out here is the part this card shows — including
   *  `number`, which is `null` on every one of them and is the proof that
   *  nothing has been sent. */
  drafted: {
    id: string;
    supplierId: string;
    supplierName: string;
    status: string;
    currency: string;
    number: string | null;
    lineCount: number;
    totals: { netCents: number; vatCents: number; grossCents: number };
  }[];
  /** A shortage no draft was written for, with a machine-readable reason the
   *  catalogue writes words for — `noSupplier` when nobody has quoted us for
   *  it, which is never an order placed blind. */
  skipped: {
    productId: string;
    productName: string;
    sku: string;
    locationCode: string;
    buyQtyMilli: number;
    reason: string;
  }[];
  /** How many shortages were looked at, as the server counted them. */
  shortages: number;
  /** How many of them became a line on a draft. */
  ordered: number;
}

/** What `stock_answer` read (B5.10): where one product stands right now — on
 *  the shelves, on order, promised out, and against the minimums the tenant set.
 *
 *  It wrote nothing, ordered nothing and reserved nothing. `stock` is the real
 *  locations only; a service carries an empty one with no quantity at all,
 *  because a service has no shelf. `availableQtyMilli` is the server's own sum
 *  (`on hand + on order − committed`), never re-derived here. */
export interface StockAnswerResultDto extends AgentResultDto {
  /** The catalog item, in the shape the catalog screens read; named here is the
   *  part this card shows. */
  product: {
    id: string;
    name: string;
    unit: string;
    sku: string;
    stocked: boolean;
  };
  stock: {
    locationId: string;
    locationCode: string;
    locationName: string;
    qtyMilli: number;
    valueCents: number;
  }[];
  onHandQtyMilli: number;
  onOrderQtyMilli: number;
  committedQtyMilli: number;
  availableQtyMilli: number;
  valueCents: number;
  /** Each place this product is watched, and whether that shelf is under the
   *  minimum set for it. */
  watched: {
    locationId: string;
    locationCode: string;
    locationName: string;
    minQtyMilli: number;
    targetQtyMilli: number;
    onHandQtyMilli: number;
    belowMinimum: boolean;
  }[];
}

/** What `who_is_off` read (B6.09): which colleagues are away over the days it
 *  was asked about — a name, an employee id and a day, and deliberately nothing
 *  else. The absence layer never loads the policy, the kind of leave or the
 *  note, so there is no reason here to render and none to infer.
 *
 *  `awayDays` is a **count of days**, not a span: somebody off on Monday and on
 *  Friday has `awayDays: 2` with `firstDay` Monday and `lastDay` Friday, and
 *  worked the days between. `days` carries the layer's own per-day shape (days
 *  with nobody away are omitted), so an empty one over a real `daysInRange` is
 *  the useful answer "nobody is off". */
export interface WhoIsOffResultDto extends AgentResultDto {
  from: string;
  to: string;
  /** How many days the window covered, as the server counted them. */
  daysInRange: number;
  people: {
    employeeId: string;
    name: string;
    awayDays: number;
    firstDay: string;
    lastDay: string;
  }[];
  days: { day: string; people: { employeeId: string; name: string }[] }[];
}

/** What `project_status_summary` read: figures only. The server composes no
 *  sentence — every label around these numbers comes from the UI's own
 *  catalogue, so the summary is in the reader's language. */
export interface ProjectStatusResultDto extends AgentResultDto {
  hours: {
    minutes: number;
    billableMinutes: number;
    billedMinutes: number;
    lastWorkedOn: string | null;
  };
  budget: {
    isClientWork: boolean;
    customer?: string | null;
    currency?: string;
    rateCents?: number | null;
    budgetMinutes?: number | null;
    budgetCents?: number | null;
    /** Consumption in basis points (10 000 = the whole budget), uncapped: a
     *  project past its budget reports over 10 000, which is the case the
     *  figure exists to show. */
    consumptionBp?: number | null;
  };
  milestones: {
    total: number;
    done: number;
    late: number;
    next: { name: string; dueOn: string; late: boolean } | null;
  };
  tasks: { total: number; open: number; overdue: number; done: number };
}
