//! Entity types returned across the store's public API.

use time::OffsetDateTime;

use crate::id::{
    BlobId, CalendarId, CategoryId, ContactId, EventId, MailboxId, MessageId, ThreadId,
};

/// The resolved AI backend a tenant's default provider points at (ADR 0011),
/// mapped for the inference client. `api_key` is a secret, never returned to
/// clients or logged.
#[derive(Debug, Clone)]
pub struct AiConfigRow {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub enabled: bool,
}

/// A group row for the admin console: name, optional distribution-list address,
/// and member count.
#[derive(Debug, Clone)]
pub struct GroupRow {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub member_count: i64,
}

/// A user row for the admin console: identity plus read-only usage (message
/// count and storage bytes). Secrets (password hash, TOTP) are never here.
#[derive(Debug, Clone)]
pub struct UserRow {
    pub id: String,
    pub email: String,
    pub is_admin: bool,
    pub created_at: OffsetDateTime,
    pub message_count: i64,
    pub storage_bytes: i64,
}

/// One configured AI provider (admin console). Several may exist per tenant;
/// exactly one enabled provider is the default the AI features use. `api_key`
/// is a secret — the admin API exposes only whether a key is set.
#[derive(Debug, Clone)]
pub struct AiProviderRow {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub enabled: bool,
    pub is_default: bool,
}

/// A tenant summary for the platform control plane (ADR 0012): identity,
/// lifecycle status, and read-only usage aggregated across the tenant. This is
/// deployment-global data (the operator's view), never a tenant's own data.
#[derive(Debug, Clone)]
pub struct TenantSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: OffsetDateTime,
    pub user_count: i64,
    pub storage_bytes: i64,
    /// The tenant's storage cap in bytes, or `None` for unlimited (ADR 0012).
    pub storage_quota_bytes: Option<i64>,
}

/// One audit-log entry for the tenant-admin audit view (ADR 0012). `actor` is
/// the acting user's email when resolvable, else a label (e.g. `operator`).
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub id: String,
    pub actor: Option<String>,
    pub action: String,
    pub target: Option<String>,
    pub detail: Option<String>,
    /// The kind of business record this entry is about (`billing.invoice`,
    /// `crm.deal`), or `None` for an administrative action whose subject is
    /// the `target` label instead.
    pub entity_type: Option<String>,
    /// The record's own id within the tenant, paired with `entity_type`.
    pub entity_id: Option<String>,
    pub created_at: OffsetDateTime,
}

/// A DKIM key for the admin/operator Domains view (ADR 0014). The secret seed
/// is NEVER in here — only the selector and the raw public key needed to build
/// the DNS record.
#[derive(Debug, Clone)]
pub struct DkimKeyRow {
    pub selector: String,
    pub algorithm: String,
    pub public_raw: Vec<u8>,
    pub active: bool,
    pub created_at: OffsetDateTime,
}

/// A domain owned by a tenant (ADR 0012). `verified_at` is `None` until the
/// DNS TXT proof is observed; `verify_token` is the value to publish at
/// `_alo-verify.<domain>`.
#[derive(Debug, Clone)]
pub struct DomainRow {
    pub domain: String,
    pub tenant_id: String,
    pub verify_token: String,
    pub verified_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

/// A stored blob's metadata (for JMAP download).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    /// Opaque id (the store's — JMAP has no second id space).
    pub id: BlobId,
    /// Size in octets.
    pub size: i64,
    /// Declared Content-Type (served verbatim on download).
    pub content_type: Option<String>,
}

/// Sort direction for `Email/query` (by `receivedAt`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    /// Oldest first.
    Asc,
    /// Newest first (the JMAP default).
    Desc,
}

/// `Email/query` filter conditions — all present ones are ANDed.
#[derive(Debug, Clone, Default)]
pub struct EmailFilter {
    /// `inMailbox`: only emails in this mailbox.
    pub in_mailbox: Option<MailboxId>,
    /// `from` substring match.
    pub from: Option<String>,
    /// `to` substring match.
    pub to: Option<String>,
    /// `subject` substring match.
    pub subject: Option<String>,
    /// `text` full-text match over subject/addresses/body.
    pub text: Option<String>,
    /// `before`: `receivedAt` strictly before.
    pub before: Option<OffsetDateTime>,
    /// `after`: `receivedAt` at or after.
    pub after: Option<OffsetDateTime>,
    /// `hasKeyword`: has this keyword.
    pub has_keyword: Option<String>,
    /// `notKeyword`: lacks this keyword.
    pub not_keyword: Option<String>,
}

/// A full `Email/query` request.
#[derive(Debug, Clone)]
pub struct EmailQuery {
    /// Filter conditions.
    pub filter: EmailFilter,
    /// Sort by `receivedAt` in this direction.
    pub sort: SortDirection,
    /// Bounded window.
    pub page: Page,
}

/// A mailbox with its live counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mailbox {
    /// Opaque id.
    pub id: MailboxId,
    /// Parent mailbox, or `None` at the root.
    pub parent_id: Option<MailboxId>,
    /// Display name (unique among siblings).
    pub name: String,
    /// JMAP role (`inbox`/`sent`/…), or `None`.
    pub role: Option<String>,
    /// Optional display color ("#rrggbb"), for color-coded labels.
    pub color: Option<String>,
    /// Total messages in the mailbox.
    pub total_messages: i64,
    /// Messages without the `$seen` keyword.
    pub unread_messages: i64,
}

/// A user-defined message category (an Outlook-style colored label). Its id is
/// embedded in the `$category_<id>` keyword carried by tagged messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    /// Opaque id.
    pub id: CategoryId,
    /// Display name (unique within the user's account).
    pub name: String,
    /// Optional display color ("#rrggbb").
    pub color: Option<String>,
    /// Order among the user's categories (ascending).
    pub sort_order: i32,
}

/// One typed value on a contact — an email address or phone number with
/// an optional label (vCard `TYPE`, e.g. `work`, `home`, `mobile`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContactField {
    /// The label (`work`/`home`/`mobile`/…), or `None` for unlabelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The address or number itself.
    pub value: String,
}

/// An address-book contact (the JMAP Contacts / CardDAV unit). Multi-valued
/// fields (`emails`, `phones`) round-trip to vCard `EMAIL`/`TEL` properties;
/// `id` is the vCard `UID`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    /// Opaque id (also the vCard `UID`).
    pub id: ContactId,
    /// Formatted name shown everywhere (vCard `FN`; never empty).
    pub display_name: String,
    /// Given name (vCard `N` component), if known.
    pub first_name: Option<String>,
    /// Family name (vCard `N` component), if known.
    pub last_name: Option<String>,
    /// Email addresses, in display order.
    pub emails: Vec<ContactField>,
    /// Phone numbers, in display order.
    pub phones: Vec<ContactField>,
    /// Organization (vCard `ORG`).
    pub organization: Option<String>,
    /// Job title (vCard `TITLE`).
    pub job_title: Option<String>,
    /// Free-form note (vCard `NOTE`).
    pub notes: Option<String>,
}

/// A calendar: a named collection of events a user owns (and, once grants land,
/// may share). Every event belongs to exactly one. The auto-created `personal`
/// calendar is the default and cannot be deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    /// Opaque id (also the CalDAV collection name).
    pub id: CalendarId,
    /// The user who owns the calendar.
    pub owner: String,
    /// Display name (e.g. "Personal", "Team").
    pub name: String,
    /// Optional display colour (hex, e.g. `#e76f51`).
    pub color: Option<String>,
    /// `personal` (the auto-created default; not deletable) or `shared`.
    pub kind: String,
    /// The **viewing** user's access: `owner`, `editor`, or `viewer`. Set when a
    /// calendar is listed for a user; drives what the UI lets them do.
    pub role: String,
}

/// A share of a calendar with a subject (a user or a group) at a role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarGrant {
    /// `user` or `group`.
    pub subject_kind: String,
    /// The granted user id or group id.
    pub subject_id: String,
    /// `viewer` or `editor`.
    pub role: String,
}

/// A calendar event (also the CalDAV/iCalendar `VEVENT`). Times are UTC
/// instants; an all-day event uses midnight-UTC bounds and the client renders it
/// date-only. `ends_at` is exclusive and must be `>= starts_at`. Every event
/// belongs to a `calendar_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEvent {
    /// Opaque id (also the iCalendar `UID`).
    pub id: EventId,
    /// The calendar this event belongs to.
    pub calendar_id: CalendarId,
    /// Title shown on the event (iCalendar `SUMMARY`; never empty).
    pub summary: String,
    /// Free-form details (iCalendar `DESCRIPTION`).
    pub description: Option<String>,
    /// Where it happens (iCalendar `LOCATION`).
    pub location: Option<String>,
    /// Start instant (UTC).
    pub starts_at: OffsetDateTime,
    /// End instant (UTC), exclusive. For a recurring event this is the end of
    /// the FIRST occurrence; every occurrence shares this duration.
    pub ends_at: OffsetDateTime,
    /// All-day (date-only) event.
    pub all_day: bool,
    /// An iCalendar `RRULE` recurrence (e.g. `FREQ=WEEKLY`), or `None` for a
    /// one-off. `starts_at`/`ends_at` describe the first occurrence; the store
    /// expands the rest within a queried range. Slice scope: simple
    /// `FREQ`+`INTERVAL`+`COUNT`/`UNTIL`; per-occurrence exceptions come later.
    pub recurrence: Option<String>,
    /// Guest email addresses (iCalendar `ATTENDEE`s). The owner emails each an
    /// iMIP invitation when the event is saved; their status tracking + RSVP
    /// replies are a later slice, so this is just the invited addresses.
    pub attendees: Vec<String>,
    /// Excluded occurrence start-instants (iCalendar `EXDATE`): individual
    /// instances of a recurring series that were skipped/cancelled while the
    /// rest stays. Empty for a one-off or an untouched series. Each matches an
    /// occurrence's `starts_at`; the store omits them when expanding a range.
    pub exdates: Vec<OffsetDateTime>,
    /// For an expanded occurrence of a recurring series, its **original** slot
    /// (iCalendar `RECURRENCE-ID`) — the start the occurrence would have had
    /// before any per-occurrence override moved it. `None` on a stored master or
    /// a one-off. It is the stable handle for editing/skipping that instance,
    /// since an override's `starts_at` may differ from its slot.
    pub recurrence_id: Option<OffsetDateTime>,
    /// The IANA zone whose **wall-clock** a recurring series follows (from an
    /// iCalendar `DTSTART;TZID=…` or the API's `timezone`): expansion repeats
    /// the local time in this zone, so the UTC instant shifts across its DST
    /// changes. `None` → occurrences are fixed UTC instants (one-offs, and
    /// UTC/floating series). Ignored for all-day events (dates don't shift).
    pub timezone: Option<String>,
    /// Extra occurrence start-instants (iCalendar `RDATE`) beyond what the
    /// `RRULE` yields; each occurrence gets the master's duration. May recur
    /// without any `RRULE` (an RDATE-only series). Empty for most events.
    pub rdates: Vec<OffsetDateTime>,
    /// Reminder lead-time in minutes before the start (iCalendar `VALARM` with a
    /// negative `TRIGGER`), or `None` for no reminder. A recurring series shares
    /// one reminder across its occurrences.
    pub reminder_minutes: Option<i32>,
    /// Per-guest RSVP status on the **organizer's** copy: `(email, PARTSTAT)`
    /// (`ACCEPTED` | `DECLINED` | `TENTATIVE` | `NEEDS-ACTION`), sorted by email.
    /// Populated as guests reply; empty otherwise. Read-only on writes — set via
    /// [`AccountStore::set_attendee_status`], preserved across event edits.
    pub attendee_status: Vec<(String, String)>,
}

/// The editable fields of a single overridden occurrence (iCalendar
/// `RECURRENCE-ID` override). Placement (which calendar) and the recurrence rule
/// are not per-occurrence, so they are absent here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OccurrenceOverride {
    /// Title (never empty; the caller enforces).
    pub summary: String,
    pub description: Option<String>,
    pub location: Option<String>,
    /// The occurrence's NEW bounds (UTC; `ends_at` exclusive, >= `starts_at`).
    pub starts_at: OffsetDateTime,
    pub ends_at: OffsetDateTime,
    pub all_day: bool,
}

/// A compact message row for mailbox listings (no body).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageSummary {
    /// Opaque id.
    pub id: MessageId,
    /// The thread this message belongs to.
    pub thread_id: ThreadId,
    /// Unfolded subject.
    pub subject: String,
    /// Unfolded `From`.
    pub from_addr: String,
    /// `Date` header, when present.
    pub sent_at: Option<OffsetDateTime>,
    /// When the store received it.
    pub received_at: OffsetDateTime,
    /// Size of the raw message in octets.
    pub size: i64,
}

/// A message's full metadata (the bytes are fetched separately as a blob).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Opaque id.
    pub id: MessageId,
    /// The thread this message belongs to.
    pub thread_id: ThreadId,
    /// The content-addressed blob holding the raw bytes.
    pub blob_id: BlobId,
    /// `Message-ID` header (angle brackets included).
    pub message_id_hdr: Option<String>,
    /// Unfolded subject.
    pub subject: String,
    /// Unfolded `From`.
    pub from_addr: String,
    /// Unfolded `To`.
    pub to_addrs: String,
    /// Unfolded `Cc`.
    pub cc_addrs: String,
    /// Unfolded `Bcc` — populated only on the sender's own copy (received
    /// copies never carry it; it is stripped from the wire message).
    pub bcc_addrs: String,
    /// Whether the message carries an attachment; `None` if not yet computed
    /// (a backfill fills existing rows).
    pub has_attachment: Option<bool>,
    /// `Date` header, when present.
    pub sent_at: Option<OffsetDateTime>,
    /// When the store received it.
    pub received_at: OffsetDateTime,
    /// Size of the raw message in octets.
    pub size: i64,
    /// Parsed Authentication-Results SPF result (RFC 8601).
    pub auth_spf: Option<String>,
    /// Parsed Authentication-Results DKIM result.
    pub auth_dkim: Option<String>,
    /// Parsed Authentication-Results DMARC result.
    pub auth_dmarc: Option<String>,
}

/// A bounded page request — every list API takes one, so no call can
/// return an unbounded result set.
#[derive(Debug, Clone, Copy)]
pub struct Page {
    limit: i64,
    offset: i64,
}

/// The largest page any single query will return.
pub const MAX_PAGE: i64 = 500;
/// The largest offset any single query will skip. A deep `OFFSET` makes
/// Postgres scan-and-discard O(offset) rows; bound it (keyset pagination
/// replaces offset paging for large collections in a later pass).
pub const MAX_OFFSET: i64 = 100_000;

impl Page {
    /// A page with `limit` clamped to `1..=MAX_PAGE` and `offset` clamped
    /// to `0..=MAX_OFFSET`.
    pub fn new(limit: i64, offset: i64) -> Self {
        Self {
            limit: limit.clamp(1, MAX_PAGE),
            offset: offset.clamp(0, MAX_OFFSET),
        }
    }

    /// The first page of `limit` rows.
    pub fn first(limit: i64) -> Self {
        Self::new(limit, 0)
    }

    /// The clamped row limit.
    pub fn limit(&self) -> i64 {
        self.limit
    }

    /// The clamped row offset.
    pub fn offset(&self) -> i64 {
        self.offset
    }
}

impl Default for Page {
    fn default() -> Self {
        Self::first(50)
    }
}
