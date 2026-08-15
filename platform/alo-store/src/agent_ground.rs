//! What an agent is allowed to *look at* before it answers (ADR 0034, A1.3) —
//! retrieval scoped to the agent's own product instead of one shared workspace
//! search.
//!
//! # The bug this replaces
//!
//! Every agent turn used to ground itself with
//! [`AccountStore::workspace_search_terms`], which unions Drive, Tasks and the
//! asker's mail. So the Inventory agent, asked whether the X100 was in stock,
//! was handed eight of the asker's private emails and no stock at all — and an
//! agent that answers plausibly from a search snippet is a failure rather than
//! a partial success. Grounding is now a property of the product, read from the
//! one table below.
//!
//! # Why some products ground in nothing, on purpose
//!
//! A source is listed here only when the caller's right to read it is already
//! settled by a predicate this file can state exactly — their own mailbox,
//! their own address book, their own diary, the rooms they are in, the files
//! and tasks they can already open. The business modules are different:
//! [`crate::user_modules`] narrows but never widens, so Finance still wants an
//! accountant and People still wants the HR role, and *those* gates live on the
//! routes rather than in a search predicate. Retrieving their rows here would
//! be a second door into role-gated records, which is exactly the kind of quiet
//! widening Law 1 exists to prevent.
//!
//! So Billing, CRM, Projects, Finance, Inventory and People ground in nothing
//! and reach their records the way ADR 0047 decided they should: through a
//! **reading tool**, executed inside the turn, which carries the module's own
//! gate with it. `stock_answer` is how the Inventory agent learns about stock.
//! Insights, Meet and Sites have neither yet (A2.1, A2.4, A3.2) and say so.
//! Empty grounding is a narrower reach than the shared search they had before,
//! never a wider one.
//!
//! # The one agent that looks everywhere
//!
//! [`AgentProduct::Workspace`] — "Ask alo" — keeps the workspace-wide view,
//! because working across products is its whole job (ADR 0034). It is the only
//! value for which this file delegates straight back to the shared search.

use crate::account::AccountStore;
use crate::agent_product::AgentProduct;
use crate::error::{Result, StoreError};
use crate::search::{SearchHit, keywords};

/// One kind of record an agent may be grounded in.
///
/// Each variant is a query whose access predicate is the same one the module it
/// belongs to already uses — never a widened copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroundSource {
    /// The caller's own messages (subject, participants, body).
    Mail,
    /// The caller's own address book.
    Contacts,
    /// The caller's own diary.
    Events,
    /// Active tasks on a project the caller can see.
    Tasks,
    /// Messages in rooms the caller is in, or public rooms.
    Chat,
    /// Files in the caller's personal area or a Space they belong to.
    Drive,
}

/// Mail's own records: the mailbox and the address book, which is why the Mail
/// agent has `find_contact` among its tools.
const MAIL: &[GroundSource] = &[GroundSource::Mail, GroundSource::Contacts];
const AGENDA: &[GroundSource] = &[GroundSource::Events];
const TASKS: &[GroundSource] = &[GroundSource::Tasks];
const CHAT: &[GroundSource] = &[GroundSource::Chat];
const DRIVE: &[GroundSource] = &[GroundSource::Drive];

/// A product that reaches its records through a reading tool rather than
/// through retrieval — see the module header for why that is the safe answer
/// and not a missing one.
const BY_TOOL_ONLY: &[GroundSource] = &[];

/// What this product's agent may be grounded in, in the order it is offered.
///
/// One table, read by [`AccountStore::agent_ground`] and by nothing else, so
/// adding a product's retrieval is filling in one row.
#[must_use]
pub fn sources_for(product: AgentProduct) -> &'static [GroundSource] {
    match product {
        AgentProduct::Mail => MAIL,
        AgentProduct::Agenda => AGENDA,
        AgentProduct::Tasks => TASKS,
        AgentProduct::Chat => CHAT,
        AgentProduct::Drive => DRIVE,
        AgentProduct::Billing
        | AgentProduct::Crm
        | AgentProduct::Projects
        | AgentProduct::Finance
        | AgentProduct::Inventory
        | AgentProduct::Hr
        | AgentProduct::Insights
        | AgentProduct::Meet
        | AgentProduct::Sites => BY_TOOL_ONLY,
        // Ask alo is the workspace-wide view and never comes through this
        // table — `agent_ground` answers it before asking.
        AgentProduct::Workspace => BY_TOOL_ONLY,
    }
}

/// How much of a chat message is worth showing as a source line.
const CHAT_SNIPPET: i64 = 160;

impl AccountStore {
    /// Retrieval for one agent turn, scoped to the agent's own product (A1.3).
    ///
    /// Returns up to `limit` hits **per source** that this product grounds in,
    /// matched on the question's content words exactly as
    /// [`Self::workspace_search_terms`] does. [`AgentProduct::Workspace`] is
    /// that shared search; every other product draws only on its own records,
    /// and several deliberately draw on none (see the module header).
    ///
    /// Access is unchanged and unwidened: every query below carries the same
    /// predicate its module's own reads carry, so an agent is never shown a row
    /// the person who asked could not already open.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn agent_ground(
        &self,
        product: AgentProduct,
        question: &str,
        limit: i64,
    ) -> Result<Vec<SearchHit>> {
        if product == AgentProduct::Workspace {
            return self.workspace_search_terms(question, limit).await;
        }
        let sources = sources_for(product);
        if sources.is_empty() {
            return Ok(Vec::new());
        }
        let Some(terms) = ground_terms(question) else {
            return Ok(Vec::new());
        };
        let mut hits = Vec::new();
        for source in sources {
            let found = match source {
                GroundSource::Mail => self.mail_term_hits(&terms, limit).await?,
                GroundSource::Contacts => self.contact_term_hits(&terms, limit).await?,
                GroundSource::Events => self.event_term_hits(&terms, limit).await?,
                GroundSource::Tasks => self.task_term_hits(&terms, limit).await?,
                GroundSource::Chat => self.chat_term_hits(&terms, limit).await?,
                GroundSource::Drive => self.drive_term_hits(&terms, limit).await?,
            };
            hits.extend(found);
        }
        Ok(hits)
    }

    /// The caller's **own** address book: any keyword as a substring of the
    /// name, the organisation or the job title.
    ///
    /// Scoped by `user_id` exactly as every other contacts read is — an address
    /// book is per person, and a colleague's is not this agent's to see.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    async fn contact_term_hits(&self, terms: &[String], limit: i64) -> Result<Vec<SearchHit>> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT id, display_name FROM contacts \
             WHERE tenant_id = $1 AND user_id = $2 \
               AND EXISTS (SELECT 1 FROM unnest($3::text[]) kw \
                             WHERE strpos(lower(display_name), kw) > 0 \
                                OR strpos(lower(coalesce(organization, '')), kw) > 0 \
                                OR strpos(lower(coalesce(job_title, '')), kw) > 0) \
             ORDER BY updated_at DESC LIMIT $4",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(terms)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|(id, display_name)| SearchHit {
                kind: "contact".to_owned(),
                id,
                title: display_name,
                space: None,
            })
            .collect())
    }

    /// The caller's diary: any keyword as a substring of the summary, the
    /// location or the description, on a calendar they may **see** — the
    /// [`crate::calendar`] module's own visibility predicate, so a colleague's
    /// diary is reachable here exactly when it is reachable in the app and
    /// never otherwise.
    ///
    /// Ordered by how close the event is to now rather than by when it was
    /// written, because a diary question is nearly always about the days either
    /// side of today — "the meeting" means the next one, not the oldest one on
    /// record.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    async fn event_term_hits(&self, terms: &[String], limit: i64) -> Result<Vec<SearchHit>> {
        let visible = crate::calendar::visible_pred();
        let rows = sqlx::query_as::<_, (String, String)>(&format!(
            "SELECT e.id, e.summary FROM calendar_events e \
             WHERE e.tenant_id = $1 \
               AND e.calendar_id IN ( \
                     SELECT c.id FROM calendars c WHERE c.tenant_id = $1 AND {visible}) \
               AND EXISTS (SELECT 1 FROM unnest($3::text[]) kw \
                             WHERE strpos(lower(e.summary), kw) > 0 \
                                OR strpos(lower(coalesce(e.location, '')), kw) > 0 \
                                OR strpos(lower(coalesce(e.description, '')), kw) > 0) \
             ORDER BY abs(extract(epoch FROM (e.starts_at - now()))) LIMIT $4"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(terms)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|(id, summary)| SearchHit {
                kind: "event".to_owned(),
                id,
                title: summary,
                space: None,
            })
            .collect())
    }

    /// Messages in rooms the caller is a member of, or in a public room they
    /// could open — the **same predicate** [`Self::search_messages`] uses, so
    /// grounding can never reach a private channel the asker is not in.
    ///
    /// Withdrawn messages are excluded: their words are gone, and a source with
    /// nothing to show is noise.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    async fn chat_term_hits(&self, terms: &[String], limit: i64) -> Result<Vec<SearchHit>> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT m.id, left(m.body, $5::int) FROM chat_messages m \
             JOIN chat_channels c ON c.tenant_id = m.tenant_id AND c.id = m.channel_id \
             WHERE m.tenant_id = $1 AND m.deleted_at IS NULL \
               AND EXISTS (SELECT 1 FROM unnest($3::text[]) kw \
                             WHERE to_tsvector('simple', m.body) \
                                   @@ plainto_tsquery('simple', kw)) \
               AND ( EXISTS (SELECT 1 FROM chat_members mm \
                               WHERE mm.tenant_id = c.tenant_id AND mm.channel_id = c.id \
                                 AND mm.user_id = $2) \
                  OR (c.visibility = 'public' AND c.archived_at IS NULL) ) \
             ORDER BY m.created_at DESC LIMIT $4",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(terms)
        .bind(limit)
        .bind(CHAT_SNIPPET)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|(id, body)| SearchHit {
                kind: "chat".to_owned(),
                id,
                title: body,
                space: None,
            })
            .collect())
    }
}

/// The words a product-scoped query matches on, or `None` when there is nothing
/// to match.
///
/// A question with no content words falls back to the whole trimmed question as
/// one term — the same fallback [`AccountStore::workspace_search_terms`] makes,
/// expressed as a term rather than a second query because every source here
/// matches a lowercase term list.
fn ground_terms(question: &str) -> Option<Vec<String>> {
    let terms = keywords(question);
    if !terms.is_empty() {
        return Some(terms);
    }
    let literal = question.trim().to_lowercase();
    (!literal.is_empty()).then_some(vec![literal])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::agent_product::ALL_AGENT_PRODUCTS;

    /// The whole table, product by product. Written out rather than derived, so
    /// giving a product a new source is a visible change to this list and not a
    /// silent widening of what an agent may look at.
    #[test]
    fn each_product_grounds_in_exactly_its_own_records() {
        assert_eq!(
            sources_for(AgentProduct::Mail),
            [GroundSource::Mail, GroundSource::Contacts]
        );
        assert_eq!(sources_for(AgentProduct::Agenda), [GroundSource::Events]);
        assert_eq!(sources_for(AgentProduct::Tasks), [GroundSource::Tasks]);
        assert_eq!(sources_for(AgentProduct::Chat), [GroundSource::Chat]);
        assert_eq!(sources_for(AgentProduct::Drive), [GroundSource::Drive]);
        for by_tool in [
            AgentProduct::Billing,
            AgentProduct::Crm,
            AgentProduct::Projects,
            AgentProduct::Finance,
            AgentProduct::Inventory,
            AgentProduct::Hr,
            AgentProduct::Insights,
            AgentProduct::Meet,
            AgentProduct::Sites,
        ] {
            assert!(
                sources_for(by_tool).is_empty(),
                "{by_tool} reaches its records through a tool, not retrieval"
            );
        }
    }

    /// The property A1.3 exists for: no product but Ask alo may look at another
    /// product's records, and no product's source list overlaps another's.
    #[test]
    fn a_products_sources_are_its_own_and_nobody_elses() {
        let mut seen: Vec<GroundSource> = Vec::new();
        for product in ALL_AGENT_PRODUCTS {
            for source in sources_for(product) {
                assert!(
                    !seen.contains(source),
                    "{source:?} grounds two products, so one of them is looking at the other's records"
                );
                seen.push(*source);
            }
        }
        // Stated plainly, because it is the sentence the queue item asks for.
        assert!(!sources_for(AgentProduct::Mail).contains(&GroundSource::Drive));
        assert!(!sources_for(AgentProduct::Inventory).contains(&GroundSource::Mail));
        assert!(sources_for(AgentProduct::Mail).contains(&GroundSource::Mail));
    }

    /// A question with no content words still grounds — on the question itself.
    #[test]
    fn a_question_of_stop_words_falls_back_to_the_whole_phrase() {
        assert_eq!(
            ground_terms("what is the X100 stock?"),
            Some(vec!["x100".to_owned(), "stock".to_owned()])
        );
        assert_eq!(
            ground_terms("who are they?"),
            Some(vec!["who are they?".to_owned()])
        );
        assert_eq!(ground_terms("   "), None);
        assert_eq!(ground_terms(""), None);
    }
}
