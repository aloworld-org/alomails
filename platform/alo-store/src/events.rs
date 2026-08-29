//! The tenant's event stream (ADR 0058 §5) — one row per intent execution.
//!
//! Everything that happens in an alo app happens through an intent, and every
//! execution of one leaves an event: the verb's name, the record it touched
//! when it touched exactly one, whose access it ran through, and which agent
//! ran it when one did. Consumers **read the stream instead of polling the
//! record tables** — audit first (a record's history shows what agents did to
//! it), notifications, standing instructions and memory extraction after.
//!
//! **Append-only.** This module is the whole surface over the `events` table,
//! and it writes and reads only: there is no update and no delete, because an
//! event is wrong only if it never happened — and then the bug is at the
//! emitter, not in the stream.
//!
//! Two reads, two doors, on purpose:
//!
//! - [`TenantStore::list_record_events`] answers a record's history and is
//!   **writes only** — what was *done to* the record, readable by anyone who
//!   can open the record, exactly like the audit trail it feeds. A read that
//!   merely looked at a record is not part of the record's story, and showing
//!   colleagues what somebody's agent looked at would leak exactly what the
//!   access rules withhold.
//! - [`AccountStore::my_events`] answers "what have my agents run lately",
//!   reads included, and is scoped to the **caller's own** events for the same
//!   reason `agent_tool_runs` is.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{ChatAgentId, DomainEventId};
use crate::store::TenantStore;

/// One event, as a consumer reads it. `actor` is the acting person's address
/// when they are still a user of the tenant, else `None` — never a raw user
/// id, which names nobody to the person reading a history. `agent` is the
/// handle people type after `@`, resolved the same way.
#[derive(Debug, Clone)]
pub struct DomainEvent {
    pub id: DomainEventId,
    /// The verb that ran, as the registry names it (`send_quote`).
    pub kind: String,
    /// `"read"` or `"write"`.
    pub effect: String,
    /// The record word the executor's reply used (`quote`), when the
    /// execution touched exactly one record.
    pub record_type: Option<String>,
    pub record_id: Option<String>,
    /// The acting person's email address, when resolvable.
    pub actor: Option<String>,
    /// The handle of the agent that ran it, when one did.
    pub agent: Option<String>,
    pub created_at: OffsetDateTime,
}

/// What to emit about one execution. A struct rather than five positional
/// arguments, for the same reason `NewAgentToolRun` is one.
#[derive(Debug, Clone)]
pub struct NewDomainEvent<'a> {
    /// The verb's registry name. Lowercase words joined by `_` or `.`.
    pub kind: &'a str,
    /// `"read"` or `"write"`.
    pub effect: &'a str,
    /// The record the execution touched, when it touched exactly one.
    pub record_type: Option<&'a str>,
    pub record_id: Option<&'a str>,
    /// The agent that ran it; `None` for a person's own tap.
    pub agent: Option<&'a ChatAgentId>,
}

/// The ceiling both reads clamp `limit` to.
const MAX_LISTED: i64 = 500;

/// The columns every event read returns, actor and agent already resolved to
/// the labels a person can read. One string so the two reads can never drift
/// into answering with different shapes.
const SELECT_EVENTS: &str = "SELECT e.id, e.kind, e.effect, e.record_type, e.record_id, \
            u.email, ag.handle, e.created_at \
     FROM events e \
     LEFT JOIN users u ON u.id = e.actor_user_id AND u.tenant_id = e.tenant_id \
     LEFT JOIN chat_agents ag ON ag.id = e.agent_id AND ag.tenant_id = e.tenant_id";

/// One row of [`SELECT_EVENTS`], before it becomes a [`DomainEvent`].
type EventRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    OffsetDateTime,
);

fn event_of(row: EventRow) -> DomainEvent {
    let (id, kind, effect, record_type, record_id, actor, agent, created_at) = row;
    DomainEvent {
        id: DomainEventId::new(id),
        kind,
        effect,
        record_type,
        record_id,
        actor,
        agent,
        created_at,
    }
}

/// Whether a name is one the stream's vocabulary accepts: non-empty, at most
/// 64 bytes, lowercase words of `a-z0-9` joined by `.` or `_` — the same
/// shape the audit trail's vocabulary uses, so the two never diverge into
/// needing a translator.
pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_')
}

impl AccountStore {
    /// Append one event to the tenant's stream.
    ///
    /// The actor is always the caller: this store handle *is* the person
    /// whose access produced the execution, so there is no parameter for it
    /// and therefore no way to attribute an event to somebody else.
    ///
    /// # Errors
    /// [`StoreError::Validation`] for an effect that is neither `read` nor
    /// `write`, a kind outside the vocabulary, a half-given record reference,
    /// or an overlong record id; [`StoreError::Db`] on a database failure.
    pub async fn emit_event(&self, event: &NewDomainEvent<'_>) -> Result<DomainEventId> {
        if !matches!(event.effect, "read" | "write") {
            return Err(StoreError::Validation(format!(
                "unknown event effect {}",
                event.effect
            )));
        }
        if !valid_name(event.kind) {
            return Err(StoreError::Validation(
                "event kind must be lowercase words joined by '.' or '_'".to_owned(),
            ));
        }
        if event.record_type.is_some() != event.record_id.is_some() {
            return Err(StoreError::Validation(
                "a record reference is a type and an id together".to_owned(),
            ));
        }
        if let Some(record_type) = event.record_type
            && !valid_name(record_type)
        {
            return Err(StoreError::Validation(
                "record type must be lowercase words joined by '.' or '_'".to_owned(),
            ));
        }
        if let Some(record_id) = event.record_id
            && (record_id.is_empty() || record_id.len() > 128)
        {
            return Err(StoreError::Validation(
                "record id must be 1..=128 bytes".to_owned(),
            ));
        }
        let id = DomainEventId::generate();
        sqlx::query(
            "INSERT INTO events \
                 (tenant_id, id, kind, record_type, record_id, actor_user_id, agent_id, effect) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(event.kind)
        .bind(event.record_type)
        .bind(event.record_id)
        .bind(self.user.as_str())
        .bind(event.agent.map(ChatAgentId::as_str))
        .bind(event.effect)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The caller's own recent events, newest first — reads included, because
    /// "what have my agents run lately" is the question this answers.
    ///
    /// **Only the caller's own.** An event is an act taken through one
    /// person's access, and a colleague reading which records were looked at
    /// on somebody else's behalf would learn from the stream exactly what the
    /// access rules exist to withhold.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn my_events(&self, limit: i64) -> Result<Vec<DomainEvent>> {
        let rows: Vec<EventRow> = sqlx::query_as(&format!(
            "{SELECT_EVENTS} \
             WHERE e.tenant_id = $1 AND e.actor_user_id = $2 \
             ORDER BY e.created_at DESC, e.id DESC LIMIT $3"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(limit.clamp(1, MAX_LISTED))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(event_of).collect())
    }
}

impl TenantStore {
    /// One record's events, newest first — **writes only**, because a
    /// record's history is what was done to it, not who looked at it.
    ///
    /// The record is addressed the way the audit trail addresses one
    /// (`billing.quote` + id); an event whose emitter stored the bare record
    /// word (`quote`) is matched by the address's own last segment, so the
    /// two vocabularies meet without a mapping table that would drift.
    ///
    /// The tenant clause is what makes this safe to expose from a record
    /// page: another tenant's record id is not a different answer but an
    /// empty one, exactly like an id that was never issued.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn list_record_events(
        &self,
        entity_type: &str,
        entity_id: &str,
        limit: i64,
    ) -> Result<Vec<DomainEvent>> {
        let record_word = entity_type.rsplit('.').next().unwrap_or(entity_type);
        let rows: Vec<EventRow> = sqlx::query_as(&format!(
            "{SELECT_EVENTS} \
             WHERE e.tenant_id = $1 AND e.record_id = $2 \
               AND (e.record_type = $3 OR e.record_type = $4) \
               AND e.effect = 'write' \
             ORDER BY e.created_at DESC, e.id DESC LIMIT $5"
        ))
        .bind(self.tenant().as_str())
        .bind(entity_id)
        .bind(entity_type)
        .bind(record_word)
        .bind(limit.clamp(1, MAX_LISTED))
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(event_of).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::valid_name;

    #[test]
    fn the_vocabulary_is_lowercase_words_joined_by_dot_or_underscore() {
        assert!(valid_name("send_quote"));
        assert!(valid_name("billing.quote.send"));
        assert!(valid_name("open_quotes"));
        assert!(!valid_name(""));
        assert!(!valid_name("Send_Quote"));
        assert!(!valid_name("send quote"));
        assert!(!valid_name("drop;--"));
        assert!(!valid_name(&"x".repeat(65)));
    }
}
