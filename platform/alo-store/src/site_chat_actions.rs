//! The tenant-facing transcript of what the site assistant did (ADR 0040,
//! item S3.03e): one bounded, tenant-scoped ledger of the assistant's own
//! acts and offers, written by the public conversation endpoints and read on
//! the same screen that switches the assistant on.
//!
//! Every entry answers the tenant's three questions — *what did it do, which
//! fact did it use, which page did that fact come from*: an answer carries
//! its citations (the published pages it drew on), a booking offer and a
//! booking carry the published service's name, a lead entry says a card was
//! raised (or that a returning contact was told "we know you"). What it
//! never carries is the conversation: there is no question field, no
//! answer-text field, and no visitor identity of any kind — the type cannot
//! represent them, and the schema-privacy test on the table proves the
//! columns match the type. Who a visitor was lives only where the act itself
//! put it (the appointment row, the CRM card), records the tenant already
//! owns.
//!
//! The ledger is bounded by construction: every write prunes the site to its
//! newest [`CHAT_ACTIONS_KEPT`] entries, so the anonymous surface that feeds
//! it can churn the transcript but never grow it.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{SiteId, generate_token};
use crate::site_public::{PublishedSite, SitePublicStore};

/// How many transcript entries each site keeps — enough recent history to
/// audit the assistant, small enough that an anonymous stranger inside the
/// rate limits can only ever churn it.
pub const CHAT_ACTIONS_KEPT: i64 = 200;

/// The most characters a recorded fact keeps. Facts are the tenant's own
/// published names (already bounded at their sources); this cap is the
/// ledger's own belt, not a rule callers should meet.
const CHAT_ACTION_FACT_MAX_CHARS: usize = 200;

/// The most citations one answer entry keeps — mirrors the small citation
/// sets the answering pipeline produces.
const CHAT_ACTION_CITATIONS_MAX: usize = 12;

/// What the assistant did, as the stored vocabulary. Additive by design:
/// later acting slices join the list the way `lead_*` joined `booking_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteChatActionKind {
    /// A question was answered, grounded in the cited pages.
    Answered,
    /// The assistant declined a question it could not ground (or the model
    /// refused) — recorded only when the model was actually consulted, the
    /// same rule spend follows, so free off-topic traffic cannot churn the
    /// ledger.
    Refused,
    /// The conversation offered one published service's free times.
    BookingOffered,
    /// An appointment was reserved from the conversation.
    Booked,
    /// The conversation offered the lead form.
    LeadOffered,
    /// A lead card was raised through CRM's seam.
    LeadSaved,
    /// CRM answered that the visitor's address is already known; nothing was
    /// raised and the visitor heard only "we know you".
    LeadKnown,
}

impl SiteChatActionKind {
    /// The stored (and wire) word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Answered => "answered",
            Self::Refused => "refused",
            Self::BookingOffered => "booking_offered",
            Self::Booked => "booked",
            Self::LeadOffered => "lead_offered",
            Self::LeadSaved => "lead_saved",
            Self::LeadKnown => "lead_known",
        }
    }

    /// Parses a stored word. `None` for a word this build does not know —
    /// the tolerant read that lets later releases add kinds additively while
    /// an older reader simply skips them.
    #[must_use]
    pub fn from_stored(word: &str) -> Option<Self> {
        Some(match word {
            "answered" => Self::Answered,
            "refused" => Self::Refused,
            "booking_offered" => Self::BookingOffered,
            "booked" => Self::Booked,
            "lead_offered" => Self::LeadOffered,
            "lead_saved" => Self::LeadSaved,
            "lead_known" => Self::LeadKnown,
            _ => return None,
        })
    }
}

/// One published page (or knowledge document) an answer drew on: the fact's
/// source, as the tenant should see it. `path` is site-relative, `None` for
/// a knowledge document — which has no public URL and is named by title
/// alone.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChatActionCitation {
    pub title: String,
    pub path: Option<String>,
}

/// One transcript entry, as the tenant's screen reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteChatAction {
    pub id: String,
    pub kind: SiteChatActionKind,
    /// The tenant-owned published fact the act used (today: the booking
    /// service's name). Never visitor input.
    pub fact: Option<String>,
    /// The booked instant, for [`SiteChatActionKind::Booked`].
    pub slot_at: Option<OffsetDateTime>,
    /// The pages an [`SiteChatActionKind::Answered`] entry drew on.
    pub citations: Vec<ChatActionCitation>,
    pub occurred_at: OffsetDateTime,
}

/// A new transcript entry, built by the public conversation endpoints
/// through the constructors below — the only shapes an entry can take, so a
/// call site cannot, say, attach a visitor's words as a fact.
#[derive(Debug, Clone)]
pub struct NewChatAction {
    kind: SiteChatActionKind,
    fact: Option<String>,
    slot_at: Option<OffsetDateTime>,
    citations: Vec<ChatActionCitation>,
}

impl NewChatAction {
    /// An answered question, with the published pages it drew on.
    #[must_use]
    pub fn answered(citations: &[ChatActionCitation]) -> Self {
        Self {
            kind: SiteChatActionKind::Answered,
            fact: None,
            slot_at: None,
            citations: citations
                .iter()
                .take(CHAT_ACTION_CITATIONS_MAX)
                .cloned()
                .collect(),
        }
    }

    /// A refusal the model was consulted for.
    #[must_use]
    pub fn refused() -> Self {
        Self::bare(SiteChatActionKind::Refused)
    }

    /// One published service's free times were offered.
    #[must_use]
    pub fn booking_offered(service_name: &str) -> Self {
        Self {
            fact: Some(bounded(service_name)),
            ..Self::bare(SiteChatActionKind::BookingOffered)
        }
    }

    /// An appointment was reserved: the service, and the instant.
    #[must_use]
    pub fn booked(service_name: &str, starts_at: OffsetDateTime) -> Self {
        Self {
            fact: Some(bounded(service_name)),
            slot_at: Some(starts_at),
            ..Self::bare(SiteChatActionKind::Booked)
        }
    }

    /// The lead form was offered.
    #[must_use]
    pub fn lead_offered() -> Self {
        Self::bare(SiteChatActionKind::LeadOffered)
    }

    /// A lead card was raised.
    #[must_use]
    pub fn lead_saved() -> Self {
        Self::bare(SiteChatActionKind::LeadSaved)
    }

    /// CRM answered "already known"; nothing was raised.
    #[must_use]
    pub fn lead_known() -> Self {
        Self::bare(SiteChatActionKind::LeadKnown)
    }

    const fn bare(kind: SiteChatActionKind) -> Self {
        Self {
            kind,
            fact: None,
            slot_at: None,
            citations: Vec::new(),
        }
    }
}

/// The first [`CHAT_ACTION_FACT_MAX_CHARS`] characters, whole — a maximal
/// service name degrades to a shorter fact rather than a refused entry.
fn bounded(value: &str) -> String {
    value.chars().take(CHAT_ACTION_FACT_MAX_CHARS).collect()
}

impl SitePublicStore {
    /// Appends one entry to the resolved site's transcript and prunes the
    /// site to its newest [`CHAT_ACTIONS_KEPT`] entries in the same call.
    /// Scoped by the resolved value's private tenant pairing, like every
    /// write on this door. Call sites treat a failure as log-and-continue:
    /// the transcript is accountability, never the visitor's answer.
    ///
    /// # Errors
    /// [`StoreError::Db`] on backend failure.
    pub async fn record_chat_action(
        &self,
        site: &PublishedSite,
        action: &NewChatAction,
    ) -> Result<()> {
        let citations = if action.citations.is_empty() {
            None
        } else {
            Some(serde_json::to_value(&action.citations).map_err(|error| {
                StoreError::Validation(format!("assistant citations failed to encode: {error}"))
            })?)
        };
        sqlx::query(
            "INSERT INTO site_chat_actions \
                 (id, tenant_id, site_id, kind, fact, slot_at, citations) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(generate_token())
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(action.kind.as_str())
        .bind(&action.fact)
        .bind(action.slot_at)
        .bind(citations)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        // The bound: keep the newest entries, shed the rest. Its own
        // statement — a failed prune leaves one extra row for the next
        // write to shed, never an unrecorded act.
        sqlx::query(
            "DELETE FROM site_chat_actions \
             WHERE tenant_id = $1 AND site_id = $2 \
               AND id NOT IN ( \
                   SELECT id FROM site_chat_actions \
                    WHERE tenant_id = $1 AND site_id = $2 \
                    ORDER BY occurred_at DESC, id DESC \
                    LIMIT $3)",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(CHAT_ACTIONS_KEPT)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }
}

impl AccountStore {
    /// The site's transcript, newest first — at most [`CHAT_ACTIONS_KEPT`]
    /// entries, which is also all the table keeps per site. Entries whose
    /// stored kind this build does not know (written by a later release) are
    /// skipped rather than failed on.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Db`] on backend failure.
    pub async fn site_chat_actions(&self, site: &SiteId) -> Result<Vec<SiteChatAction>> {
        self.require_site(site).await?;
        let rows = sqlx::query_as::<_, ActionRow>(
            "SELECT id, kind, fact, slot_at, citations, occurred_at \
             FROM site_chat_actions \
             WHERE tenant_id = $1 AND site_id = $2 \
             ORDER BY occurred_at DESC, id DESC \
             LIMIT $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(CHAT_ACTIONS_KEPT)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .filter_map(ActionRow::into_action)
            .collect())
    }
}

#[derive(sqlx::FromRow)]
struct ActionRow {
    id: String,
    kind: String,
    fact: Option<String>,
    slot_at: Option<OffsetDateTime>,
    citations: Option<serde_json::Value>,
    occurred_at: OffsetDateTime,
}

impl ActionRow {
    fn into_action(self) -> Option<SiteChatAction> {
        let kind = SiteChatActionKind::from_stored(&self.kind)?;
        let citations = self
            .citations
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        Some(SiteChatAction {
            id: self.id,
            kind,
            fact: self.fact,
            slot_at: self.slot_at,
            citations,
            occurred_at: self.occurred_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every kind survives the store-and-read word round trip, and the
    /// stored words match the migration's CHECK list.
    #[test]
    fn every_kind_round_trips_through_its_stored_word() {
        for kind in [
            SiteChatActionKind::Answered,
            SiteChatActionKind::Refused,
            SiteChatActionKind::BookingOffered,
            SiteChatActionKind::Booked,
            SiteChatActionKind::LeadOffered,
            SiteChatActionKind::LeadSaved,
            SiteChatActionKind::LeadKnown,
        ] {
            assert_eq!(SiteChatActionKind::from_stored(kind.as_str()), Some(kind));
        }
        assert_eq!(SiteChatActionKind::from_stored("paid"), None);
    }

    #[test]
    fn facts_are_bounded_at_character_boundaries() {
        let long = "é".repeat(CHAT_ACTION_FACT_MAX_CHARS + 40);
        let action = NewChatAction::booking_offered(&long);
        assert_eq!(
            action.fact.as_deref().map(|fact| fact.chars().count()),
            Some(CHAT_ACTION_FACT_MAX_CHARS)
        );
    }

    #[test]
    fn answered_keeps_a_bounded_citation_set() {
        let many: Vec<ChatActionCitation> = (0..40)
            .map(|n| ChatActionCitation {
                title: format!("Page {n}"),
                path: Some(format!("/{n}")),
            })
            .collect();
        let action = NewChatAction::answered(&many);
        assert_eq!(action.citations.len(), CHAT_ACTION_CITATIONS_MAX);
        assert_eq!(action.citations[0].title, "Page 0");
    }
}
