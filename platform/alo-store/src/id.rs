//! Opaque, random, URL-safe identifiers.
//!
//! Every id that crosses the API boundary is `base64url(16 random
//! bytes)` — 128 bits, non-sequential, unguessable. A leaked id reveals
//! nothing about its neighbours and cannot be incremented into another
//! tenant's row (the auditor's first probe).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};

static FALLBACK_COUNTER: AtomicU64 = AtomicU64::new(0);
static PROCESS_SALT: OnceLock<[u8; 32]> = OnceLock::new();

/// A per-process secret salt, drawn once at startup (when the RNG almost
/// certainly still works). Mixed into the RNG-failure fallback so those
/// ids remain unguessable rather than a predictable counter.
fn process_salt() -> &'static [u8; 32] {
    PROCESS_SALT.get_or_init(|| {
        let mut salt = [0u8; 32];
        let _ = SystemRandom::new().fill(&mut salt);
        salt
    })
}

/// 16 cryptographically-random bytes. Infallible (never panics — this
/// runs on the delivery path): on the essentially impossible event that
/// the system RNG is unavailable at runtime, derives the bytes from
/// `SHA-256(process-salt || counter || clock)` so they stay unguessable
/// and non-sequential, not a bare counter.
fn random_bytes() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    if SystemRandom::new().fill(&mut bytes).is_err() {
        let n = FALLBACK_COUNTER.fetch_add(1, Ordering::Relaxed);
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let mut hasher = Sha256::new();
        hasher.update(process_salt());
        hasher.update(n.to_le_bytes());
        hasher.update(t.to_le_bytes());
        bytes.copy_from_slice(&hasher.finalize()[..16]);
    }
    bytes
}

/// Generates one opaque id token. `pub(crate)` so sibling modules can mint
/// non-id opaque tokens (e.g. a domain DNS-verification token) from the same
/// cryptographically-random, non-sequential source.
pub(crate) fn generate_token() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes())
}

/// Defines a typed, opaque id newtype over `String`.
macro_rules! opaque_id {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        pub struct $name(String);

        impl $name {
            /// Generates a fresh random id.
            pub fn generate() -> Self {
                Self(generate_token())
            }

            /// Wraps an existing id string (e.g. one read back from the
            /// database or received over the API).
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            /// The id as a string slice (for binding into queries).
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(id: String) -> Self {
                Self(id)
            }
        }
    };
}

opaque_id!(
    /// A tenant — the isolation root. The only key to a [`crate::TenantStore`].
    TenantId
);
opaque_id!(
    /// A user (JMAP account) within a tenant.
    UserId
);
opaque_id!(
    /// A group (a named membership set) within a tenant.
    GroupId
);
opaque_id!(
    /// A mailbox.
    MailboxId
);
opaque_id!(
    /// A message.
    MessageId
);
opaque_id!(
    /// A thread.
    ThreadId
);
opaque_id!(
    /// A content-addressed blob.
    BlobId
);
opaque_id!(
    /// A user-defined message category (colored label). The id is embedded in
    /// the message's `$category_<id>` keyword to record membership.
    CategoryId
);
opaque_id!(
    /// An address-book contact. Also serves as the vCard `UID`, so a contact
    /// keeps its identity across a CardDAV/JMAP round-trip.
    ContactId
);
opaque_id!(
    /// A calendar event. Also serves as the iCalendar `UID`, so an event keeps
    /// its identity across a CalDAV round-trip once calendar sync lands.
    EventId
);
opaque_id!(
    /// A calendar (a named collection of events). Also the CalDAV collection
    /// name. Every event belongs to exactly one calendar.
    CalendarId
);
opaque_id!(
    /// A task — the core record of the Tasks module (ADR 0021).
    TaskId
);
opaque_id!(
    /// A task project (board): the group a task belongs to, and how personal
    /// vs team is expressed (ADR 0021).
    ProjectId
);
opaque_id!(
    /// A task subtask (checklist item).
    SubtaskId
);
opaque_id!(
    /// A task comment.
    CommentId
);
opaque_id!(
    /// A file attached to a task (a reference to a tenant blob).
    AttachmentId
);
opaque_id!(
    /// A task label (tag) — reusable and tenant-scoped.
    LabelId
);
opaque_id!(
    /// A Space — the membership group modules attach to (ADR 0026).
    SpaceId
);
opaque_id!(
    /// A node in the Drive tree: a folder, file, or document (ADR 0027).
    DriveNodeId
);
opaque_id!(
    /// A table within an alo Base (ADR 0032).
    BaseTableId
);
opaque_id!(
    /// A typed field (column) of a Base table.
    BaseFieldId
);
opaque_id!(
    /// A record (row) in a Base table.
    BaseRecordId
);
opaque_id!(
    /// A saved view over a Base table.
    BaseViewId
);
opaque_id!(
    /// A tenant's website built with alo Sites (ADR 0036).
    SiteId
);
opaque_id!(
    /// One page of an alo Sites website.
    SitePageId
);
opaque_id!(
    /// One publish of an alo Sites website — an immutable record of the
    /// snapshot set the public service serves.
    SitePublishId
);
opaque_id!(
    /// A contact form on an alo Sites website — the object a `contact_form`
    /// section references by id.
    SiteFormId
);
opaque_id!(
    /// One visitor submission posted to a site contact form.
    SiteFormSubmissionId
);
opaque_id!(
    /// One blog post on an alo Sites website; its body lives in alo Docs.
    SitePostId
);
opaque_id!(
    /// A billing customer — the company or person a tenant invoices
    /// (alo Billing, ADR 0035).
    BillingCustomerId
);
opaque_id!(
    /// A billing product — one line of a tenant's price list
    /// (alo Billing, ADR 0035).
    BillingProductId
);
opaque_id!(
    /// A billing invoice — the document a tenant raises against a customer
    /// (alo Billing, ADR 0035).
    BillingInvoiceId
);
opaque_id!(
    /// A billing quote — the offer a tenant makes a customer before invoicing
    /// them (alo Billing, ADR 0035).
    BillingQuoteId
);
opaque_id!(
    /// A billing schedule — a standing arrangement that raises the same invoice
    /// again every month, quarter or year (alo Billing, ADR 0035, wave B2).
    /// What it raises is always a **draft**: the schedule never issues anything.
    BillingScheduleId
);
opaque_id!(
    /// One line of a billing document. Invoices and quotes share the line
    /// model (`crate::billing_line`), so they share its id type.
    BillingLineId
);
opaque_id!(
    /// One payment received against a billing invoice — money that arrived,
    /// not a state of the document (alo Billing, ADR 0035).
    BillingPaymentId
);
opaque_id!(
    /// A bill — a supplier's invoice, read from the e-invoice file they sent
    /// and waiting to be approved. The mirror of [`BillingInvoiceId`]: that one
    /// is a document we raise, this one a document we receive
    /// (alo Billing, ADR 0035).
    BillingBillId
);
opaque_id!(
    /// A CRM pipeline — one board a tenant's deals move across (alo CRM,
    /// ADR 0035, wave B2).
    CrmPipelineId
);
opaque_id!(
    /// One column of a CRM pipeline. Its `is_won`/`is_lost` flags are what
    /// make a column mean "closed" (alo CRM, ADR 0035, wave B2).
    CrmStageId
);
opaque_id!(
    /// A CRM deal — one opportunity moving across a board (alo CRM, ADR 0035,
    /// wave B2).
    CrmDealId
);
opaque_id!(
    /// One row of a deal's append-only stage history: the record that it moved,
    /// from where, to where, by whom and when.
    CrmEventId
);
opaque_id!(
    /// One entry in a deal's log of what was said and done — a note, a call, a
    /// meeting. A deal's *next step* has no id of this kind: it is a real task
    /// (alo CRM, ADR 0035, wave B2).
    CrmActivityId
);
opaque_id!(
    /// An Insights dashboard — one board a tenant reads its numbers from
    /// (alo Insights, ADR 0037, wave BI-1).
    InsightDashboardId
);
opaque_id!(
    /// One tile pinned to an Insights dashboard. A tile holds the *question*
    /// (its ChartSpec), never an answer: nothing computed is stored.
    InsightTileId
);
opaque_id!(
    /// One recorded piece of work — a person, a day, a duration (alo Projects,
    /// ADR 0035, wave B3). The row a timesheet, an approval and eventually an
    /// invoice line are all folds over.
    TimeEntryId
);
opaque_id!(
    /// One milestone of a project — a named date the plan is drawn from (alo
    /// Projects, ADR 0035, wave B3). A task points at one through
    /// `task_milestones`, so a milestone owns no work: it places it.
    ProjectMilestoneId
);
opaque_id!(
    /// One account of a tenant's chart of accounts — a place money can be
    /// (alo Finance, ADR 0035, wave B4). A posting rule finds it by its
    /// `role`, never by this id and never by its code.
    FinAccountId
);
opaque_id!(
    /// One thing said in a room (alo Chat, ADR 0038). Edits and withdrawals
    /// address a message by this id; ORDER is the room's own `seq`, never this.
    ChatMessageId
);
opaque_id!(
    /// One chat room — a named channel or a DM (alo Chat, ADR 0038). Rooms are
    /// never addressable across tenants; membership is the permission that
    /// makes one visible at all.
    ChatChannelId
);
opaque_id!(
    /// One agent that can be named in a conversation (ADR 0034 §chat). It is
    /// deliberately **not** a [`UserId`]: an agent has an identity to post
    /// under and no authority of its own, so it must never be usable anywhere
    /// a person is expected — mail, assignment, seats, Space membership.
    ChatAgentId
);
opaque_id!(
    /// One action an agent has proposed, waiting for a tap (ADR 0023, ADR
    /// 0034). Addressed by this id when approved or turned down.
    ChatProposalId
);
opaque_id!(
    /// One journal entry — everything one document event did to the books, in
    /// one transaction (alo Finance, ADR 0035, wave B4). An entry is written
    /// whole and never edited: a correction is another entry pointing back at
    /// this id.
    FinEntryId
);
opaque_id!(
    /// One posting — a single line of a journal entry, moving one signed amount
    /// on one account (alo Finance, ADR 0035, wave B4). Addressed by id only so
    /// a row can be pointed at; nothing in the module ever updates one.
    FinPostingId
);
opaque_id!(
    /// One person's week, once it has a status — the row that submit, approve
    /// and reject decide, and that every hour in that week is locked by (alo
    /// Projects, ADR 0035, wave B3). A week nobody has submitted has no row and
    /// therefore no id: on the personal door a week is addressed by its Monday.
    TimeWeekId
);
opaque_id!(
    /// One expense category — a word a person picks on a claim form, and the
    /// account it books to (alo Finance, ADR 0035, wave B4). Tenant-wide
    /// configuration, like the chart it points into.
    FinCategoryId
);
opaque_id!(
    /// One expense claim — what a person spent, on what day, out of whose
    /// pocket (alo Finance, ADR 0035, wave B4). A claim is personal data about
    /// an employee: on the personal door it is only ever reachable by the
    /// person who made it.
    FinExpenseId
);
opaque_id!(
    /// One journey somebody drove in their own car (alo Finance, ADR 0035, wave
    /// B4). A journey is not money: it points at the [`FinExpenseId`] the rate
    /// table turned it into, and that claim carries the amount.
    FinMileageId
);
opaque_id!(
    /// One row of a tenant's per-kilometre rate table — what a kilometre is
    /// worth from a given day on (alo Finance, ADR 0035, wave B4). Tenant-wide
    /// configuration; a journey copies the *value*, never this id, so correcting
    /// the table never restates a claim already paid.
    FinMileageRateId
);
opaque_id!(
    /// One imported bank statement — a file a bank produced, held as what it
    /// said rather than as anything booked (alo Finance, ADR 0035, wave B4.08).
    BankStatementId
);
opaque_id!(
    /// One staged bank line: a transaction the bank states, waiting for a human
    /// to say what it *was* (alo Finance, ADR 0035, wave B4.08). It is not an
    /// event and posts nothing — confirming its match is what does.
    BankLineId
);
opaque_id!(
    /// One confirmed match between a staged bank line and the document it
    /// turned out to be (alo Finance, ADR 0035, wave B4.09). A person confirmed
    /// it, and it is what created the payment and moved the books.
    BankMatchId
);
opaque_id!(
    /// One rule a tenant taught the reconciliation screen — "money from this
    /// counterparty is that customer's" (alo Finance, ADR 0035, wave B4.09b). A
    /// rule only ever ranks a suggestion higher; confirming is still a person's
    /// act.
    FinMatchRuleId
);
opaque_id!(
    /// One fiscal period of a tenant — a quarter, a month or a year the books
    /// are reported on, and closed as a whole (alo Finance, ADR 0035, wave
    /// B4.10). Closing it is what shuts the journal to entries dated inside it.
    FinPeriodId
);

opaque_id!(
    /// A supplier — a company a tenant buys from (alo Inventory, ADR 0035,
    /// wave B5.03). Deliberately not a [`BillingCustomerId`] with a flag: the
    /// failure mode of a wrong flag is invoicing a supplier
    /// (`docs/design/inventory.md`, "Suppliers").
    InvSupplierId
);

opaque_id!(
    /// A place stock can be — a warehouse, a shop floor, a van, or one of the
    /// four virtual counterparties goods arrive from and leave to (alo
    /// Inventory, ADR 0035, wave B5.04a). Both ends of every movement are one
    /// of these, so the ledger is a closed system whose quantities sum to zero
    /// (`docs/design/inventory.md`, "Why virtual locations").
    InvLocationId
);

opaque_id!(
    /// One movement of one product between two locations (alo Inventory, ADR
    /// 0035, wave B5.04a). Append-only, like a [`FinEntryId`]: a movement
    /// recorded in error is corrected by a movement the other way, never by an
    /// edit.
    InvMoveId
);

opaque_id!(
    /// One order placed with a supplier (alo Inventory, ADR 0035, wave B5.05a)
    /// — what we asked for, at what price, and what has arrived against it.
    /// Distinct from a [`BillingBillId`], which is what they later charge us:
    /// the order is our document, the bill is theirs
    /// (`docs/design/inventory.md`, "Purchase orders").
    InvPurchaseOrderId
);

opaque_id!(
    /// A meeting. Distinct from the opaque room name the media engine is told:
    /// that is generated separately so the engine cannot be correlated back to
    /// a workspace record by anyone reading its logs.
    MeetingId
);

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_opaque_urlsafe_and_unique() {
        let mut seen = HashSet::new();
        for _ in 0..10_000 {
            let id = MessageId::generate();
            let s = id.as_str();
            // 16 bytes → 22 base64url chars, no padding, URL-safe set.
            assert_eq!(s.len(), 22);
            assert!(
                s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "unexpected char in id {s}"
            );
            assert!(seen.insert(s.to_owned()), "duplicate id {s}");
        }
    }

    #[test]
    fn ids_are_not_sequential() {
        // Two consecutive ids share no long common prefix (not a counter).
        let a = TenantId::generate();
        let b = TenantId::generate();
        let common = a
            .as_str()
            .chars()
            .zip(b.as_str().chars())
            .take_while(|(x, y)| x == y)
            .count();
        assert!(common < 8, "ids look sequential: {a} vs {b}");
    }
}
