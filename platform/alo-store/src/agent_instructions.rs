//! Standing instructions (ADR 0057, `docs/design/complete-agents.md` §7,
//! queue item A7.1): a person asks once, in advance, and the agent acts on a
//! trigger — a schedule, or a module event the intent registry names.
//!
//! **Each firing is a turn with the author as asker.** The instruction's text
//! is run verbatim as the question, through the author's own account door, in
//! the room the instruction lives in: reads post into the room, writes propose
//! to the author, exactly as if the author had typed the words at that moment.
//! The row here holds only what that takes — the words, the trigger, and the
//! clock; the firing itself belongs to the API layer, which owns turns.
//!
//! **The bounds are properties of the store, not hopes about the sweeper.**
//! One firing per instruction per hour: a schedule's repeat is at least sixty
//! minutes (schema CHECK), and an event trigger claims at most once an hour,
//! coalescing every matching event since the last firing into one turn.
//! Twenty instructions per channel: counted where the row is made.
//!
//! **Paused when the author leaves** — the firing runs on their access, and
//! access that walked out must not keep acting in the room. Nothing unpauses
//! in v1; the author re-creates the instruction instead. An archived room and
//! a removed agent go further: their instructions are deleted outright,
//! because there is no surface left for the card or the firing.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::chat::MemberRole;
use crate::error::{Result, StoreError};
use crate::events::valid_event_name;
use crate::id::{AgentInstructionId, ChatAgentId, ChatChannelId, TenantId, UserId};
use crate::store::Store;

/// The longest instruction the store will hold — the same "words, not a
/// transcript" ceiling a memory has ([`crate::agent_memories::MEMORY_FACT_MAX`]).
pub const INSTRUCTION_TEXT_MAX: usize = 400;

/// How many standing instructions one channel holds — the design's "twenty
/// per channel", enforced where the row is made.
pub const INSTRUCTIONS_PER_CHANNEL: i64 = 20;

/// The shortest schedule: one firing per instruction per hour.
pub const INSTRUCTION_MIN_MINUTES: i32 = 60;

/// The longest schedule: four weeks. Beyond that an instruction is a note in
/// somebody's diary, not a standing order a card should hold open.
pub const INSTRUCTION_MAX_MINUTES: i32 = 40_320;

/// What fires an instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstructionTrigger {
    /// Every so many minutes, at least hourly.
    Schedule { every_minutes: i32 },
    /// When the tenant's event stream gains an event of this kind — a verb
    /// the intent registry names, e.g. `issue_invoice`.
    Event { kind: String },
    /// Once, at its moment (`next_run`), and then the row goes with the
    /// firing — a task assigned to an agent is exactly this, with the task's
    /// due date as the moment (ADR 0058 §6, A8.2).
    Once,
}

/// One standing instruction, as the card reads it.
#[derive(Debug, Clone)]
pub struct AgentInstruction {
    pub id: AgentInstructionId,
    pub agent: ChatAgentId,
    /// The handle people type after `@`, joined on for the card.
    pub agent_handle: String,
    pub channel: ChatChannelId,
    pub author: UserId,
    /// The author's address, when they are still a user of the tenant — the
    /// card names a person, never a raw id.
    pub author_email: Option<String>,
    /// The instruction in the author's words.
    pub text: String,
    pub trigger: InstructionTrigger,
    /// When the schedule next fires; `None` for an event trigger.
    pub next_run: Option<OffsetDateTime>,
    pub last_fired_at: Option<OffsetDateTime>,
    /// Set when the author left the room; a paused instruction never fires.
    pub paused_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

type InstructionRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    Option<i32>,
    Option<OffsetDateTime>,
    Option<OffsetDateTime>,
    Option<OffsetDateTime>,
    OffsetDateTime,
);

/// The columns of one instruction with its agent's handle and its author's
/// address joined on — use with `FROM agent_instructions i` plus
/// [`INSTRUCTION_JOINS`].
const INSTRUCTION_COLUMNS: &str = "i.id, i.agent_id, a.handle, i.channel_id, i.author_id, \
     u.email, i.text, i.trigger_kind, i.event_kind, i.repeat_minutes, i.next_run, \
     i.last_fired_at, i.paused_at, i.created_at";

/// The joins [`INSTRUCTION_COLUMNS`] needs. A deleted user still leaves the
/// row readable (LEFT JOIN); a deleted agent cannot happen while the row
/// exists, but the join is tenant-pinned all the same.
const INSTRUCTION_JOINS: &str = "JOIN chat_agents a ON a.tenant_id = i.tenant_id AND a.id = i.agent_id \
     LEFT JOIN users u ON u.tenant_id = i.tenant_id AND u.id = i.author_id";

fn row_to_instruction(row: InstructionRow) -> Result<AgentInstruction> {
    let (
        id,
        agent,
        agent_handle,
        channel,
        author,
        author_email,
        text,
        trigger_kind,
        event_kind,
        repeat_minutes,
        next_run,
        last_fired_at,
        paused_at,
        created_at,
    ) = row;
    let trigger = match trigger_kind.as_str() {
        "schedule" => InstructionTrigger::Schedule {
            every_minutes: repeat_minutes
                .ok_or_else(|| StoreError::Validation("a schedule without a repeat".to_owned()))?,
        },
        "event" => InstructionTrigger::Event {
            kind: event_kind.ok_or_else(|| {
                StoreError::Validation("an event trigger without a kind".to_owned())
            })?,
        },
        "once" => InstructionTrigger::Once,
        other => {
            return Err(StoreError::Validation(format!(
                "unknown instruction trigger {other}"
            )));
        }
    };
    Ok(AgentInstruction {
        id: AgentInstructionId::new(id),
        agent: ChatAgentId::new(agent),
        agent_handle,
        channel: ChatChannelId::new(channel),
        author: UserId::new(author),
        author_email,
        text,
        trigger,
        next_run,
        last_fired_at,
        paused_at,
        created_at,
    })
}

/// The instruction's words, trimmed and held under [`INSTRUCTION_TEXT_MAX`].
fn validate_text(text: &str) -> Result<&str> {
    let text = text.trim();
    if text.is_empty() {
        return Err(StoreError::Validation(
            "an instruction needs words to run".to_owned(),
        ));
    }
    if text.chars().count() > INSTRUCTION_TEXT_MAX {
        return Err(StoreError::Validation(format!(
            "an instruction is one short ask — at most {INSTRUCTION_TEXT_MAX} characters"
        )));
    }
    Ok(text)
}

impl AccountStore {
    /// Stand an instruction up in a room: this caller as author, the agent to
    /// run it, the words to run, and the trigger. For a schedule, `first_at`
    /// sets the first firing; `None` means one full repeat from now — asked
    /// in advance means the first run is in the future, not this second.
    ///
    /// The caller must be a **member** of the room (an instruction posts into
    /// it on a clock, which is more than reading), and the agent must be in
    /// the room, awake, and the caller's to see — the same module gate every
    /// other agent surface asks.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] for a room the caller is not a member of or
    /// an agent that is not theirs to see or not in the room;
    /// [`StoreError::Validation`] for bad words, a bad trigger, a retired
    /// agent, or a room already holding [`INSTRUCTIONS_PER_CHANNEL`].
    pub async fn create_agent_instruction(
        &self,
        agent: &ChatAgentId,
        channel: &ChatChannelId,
        text: &str,
        trigger: &InstructionTrigger,
        first_at: Option<OffsetDateTime>,
    ) -> Result<AgentInstruction> {
        let text = validate_text(text)?;
        self.channel(channel).await?;
        if self.channel_role(channel).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let known = self.agent(agent).await?;
        if known.disabled {
            return Err(StoreError::Validation(format!(
                "@{} is retired and takes no instructions",
                known.handle
            )));
        }
        let present: Option<(i32,)> = sqlx::query_as(
            "SELECT 1 FROM chat_agent_members \
             WHERE tenant_id = $1 AND channel_id = $2 AND agent_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(agent.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if present.is_none() {
            return Err(StoreError::Validation(format!(
                "@{} is not in this room — add it first",
                known.handle
            )));
        }
        let (event_kind, repeat_minutes, next_run) = match trigger {
            InstructionTrigger::Schedule { every_minutes } => {
                if !(INSTRUCTION_MIN_MINUTES..=INSTRUCTION_MAX_MINUTES).contains(every_minutes) {
                    return Err(StoreError::Validation(format!(
                        "a schedule repeats every {INSTRUCTION_MIN_MINUTES} minutes to every \
                         {INSTRUCTION_MAX_MINUTES} (four weeks)"
                    )));
                }
                let first = first_at.unwrap_or_else(|| {
                    OffsetDateTime::now_utc() + time::Duration::minutes(i64::from(*every_minutes))
                });
                (None, Some(*every_minutes), Some(first))
            }
            InstructionTrigger::Event { kind } => {
                if !valid_event_name(kind) {
                    return Err(StoreError::Validation(
                        "an event trigger names a verb: lowercase words joined by '.' or '_'"
                            .to_owned(),
                    ));
                }
                (Some(kind.as_str()), None, None)
            }
            // A one-shot's moment may be in the past — an overdue task is
            // still assigned, and the next sweep is when it fires.
            InstructionTrigger::Once => (
                None,
                None,
                Some(first_at.unwrap_or_else(OffsetDateTime::now_utc)),
            ),
        };
        let held: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM agent_instructions WHERE tenant_id = $1 AND channel_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if held.0 >= INSTRUCTIONS_PER_CHANNEL {
            return Err(StoreError::Validation(format!(
                "this room already holds {INSTRUCTIONS_PER_CHANNEL} standing instructions — \
                 cancel one first"
            )));
        }
        let id = AgentInstructionId::generate();
        sqlx::query(
            "INSERT INTO agent_instructions \
               (tenant_id, id, agent_id, channel_id, author_id, text, \
                trigger_kind, event_kind, repeat_minutes, next_run) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(agent.as_str())
        .bind(channel.as_str())
        .bind(self.user.as_str())
        .bind(text)
        .bind(match trigger {
            InstructionTrigger::Schedule { .. } => "schedule",
            InstructionTrigger::Event { .. } => "event",
            InstructionTrigger::Once => "once",
        })
        .bind(event_kind)
        .bind(repeat_minutes)
        .bind(next_run)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        self.instruction(&id).await
    }

    /// One instruction of this tenant, joined for the card.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when there is no such row here.
    async fn instruction(&self, id: &AgentInstructionId) -> Result<AgentInstruction> {
        let row: Option<InstructionRow> = sqlx::query_as(&format!(
            "SELECT {INSTRUCTION_COLUMNS} FROM agent_instructions i {INSTRUCTION_JOINS} \
             WHERE i.tenant_id = $1 AND i.id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row_to_instruction(row.ok_or(StoreError::NotFound)?)
    }

    /// A room's standing instructions in the order they were made — readable
    /// by everyone who can read the room, exactly like the card it feeds.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's to see.
    pub async fn channel_instructions(
        &self,
        channel: &ChatChannelId,
    ) -> Result<Vec<AgentInstruction>> {
        self.channel(channel).await?;
        let rows: Vec<InstructionRow> = sqlx::query_as(&format!(
            "SELECT {INSTRUCTION_COLUMNS} FROM agent_instructions i {INSTRUCTION_JOINS} \
             WHERE i.tenant_id = $1 AND i.channel_id = $2 \
             ORDER BY i.created_at, i.id"
        ))
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(row_to_instruction).collect()
    }

    /// Cancel one instruction — the author's and the room owner's brake
    /// (either side of a direct room counts as its owner).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] where the instruction was never this caller's
    /// to see (another tenant's, an unknown id, a room they cannot read — one
    /// answer, no oracle); [`StoreError::Forbidden`] for a member who is
    /// neither the author nor an owner.
    pub async fn cancel_agent_instruction(&self, id: &AgentInstructionId) -> Result<()> {
        let held = self.instruction(id).await?;
        let room = self.channel(&held.channel).await?;
        let allowed = held.author.as_str() == self.user.as_str()
            || match self.channel_role(&held.channel).await? {
                Some(MemberRole::Owner) => true,
                Some(MemberRole::Member) => room.kind.is_direct(),
                None => false,
            };
        if !allowed {
            return Err(StoreError::Forbidden);
        }
        sqlx::query("DELETE FROM agent_instructions WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Pause everything this author stood up in this room — called by
    /// [`AccountStore::remove_member`](crate::chat) when they leave, because
    /// each firing runs on the author's access and access that walked out
    /// must not keep acting in the room.
    pub(crate) async fn pause_author_instructions(
        &self,
        channel: &ChatChannelId,
        author: &UserId,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE agent_instructions SET paused_at = now() \
             WHERE tenant_id = $1 AND channel_id = $2 AND author_id = $3 \
               AND paused_at IS NULL",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(author.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Delete a room's instructions — an archived room takes no further
    /// turns, so a card and a clock there have nothing left to be right on.
    pub(crate) async fn delete_channel_instructions(&self, channel: &ChatChannelId) -> Result<()> {
        sqlx::query("DELETE FROM agent_instructions WHERE tenant_id = $1 AND channel_id = $2")
            .bind(self.tenant.as_str())
            .bind(channel.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Delete one agent's instructions in one room — an agent taken out of a
    /// room cannot be asked to keep acting in it.
    pub(crate) async fn delete_agent_channel_instructions(
        &self,
        agent: &ChatAgentId,
        channel: &ChatChannelId,
    ) -> Result<()> {
        sqlx::query(
            "DELETE FROM agent_instructions \
             WHERE tenant_id = $1 AND channel_id = $2 AND agent_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(agent.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }
}

/// One claimed firing: what the sweeper needs to run one turn as the author.
#[derive(Debug, Clone)]
pub struct DueInstruction {
    pub tenant: TenantId,
    pub id: AgentInstructionId,
    pub agent: ChatAgentId,
    pub channel: ChatChannelId,
    pub author: UserId,
    /// The instruction's words, run verbatim as the turn's question.
    pub text: String,
}

/// Both claim queries guard the same way at fire time, belt beside the
/// hooks' braces: the room still live, the author still a member, the agent
/// still in the room and awake. A row any of these fails is left unclaimed —
/// paused or deleted by its own hook, or waiting for the state to mend.
const CLAIM_GUARDS: &str = "AND c.archived_at IS NULL \
      AND EXISTS (SELECT 1 FROM chat_members m \
                   WHERE m.tenant_id = i2.tenant_id AND m.channel_id = i2.channel_id \
                     AND m.user_id = i2.author_id) \
      AND EXISTS (SELECT 1 FROM chat_agent_members am \
                   WHERE am.tenant_id = i2.tenant_id AND am.channel_id = i2.channel_id \
                     AND am.agent_id = i2.agent_id) \
      AND EXISTS (SELECT 1 FROM chat_agents ag \
                   WHERE ag.tenant_id = i2.tenant_id AND ag.id = i2.agent_id \
                     AND ag.disabled_at IS NULL)";

type DueRow = (String, String, String, String, String, String);

fn due_of(rows: Vec<DueRow>) -> Vec<DueInstruction> {
    rows.into_iter()
        .map(
            |(tenant, id, agent, channel, author, text)| DueInstruction {
                tenant: TenantId::new(tenant),
                id: AgentInstructionId::new(id),
                agent: ChatAgentId::new(agent),
                channel: ChatChannelId::new(channel),
                author: UserId::new(author),
                text,
            },
        )
        .collect()
}

impl Store {
    /// Claim the instructions due to fire, at most `limit`, stamping each
    /// claimed row so a second sweep cannot fire it again — the same
    /// claim-then-act shape as
    /// [`Store::claim_due_sends`](crate::schedule).
    ///
    /// A schedule is due when its clock has arrived; the claim moves the
    /// clock one whole repeat from **now**, so a sweeper that was down for a
    /// day fires once and resumes, rather than replaying the backlog. An
    /// event trigger is due when the tenant's stream holds an event of its
    /// kind newer than the last firing (or than the instruction itself,
    /// before any) — and at most once an hour, coalescing everything since
    /// into one turn.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn claim_due_instructions(&self, limit: i64) -> Result<Vec<DueInstruction>> {
        let scheduled: Vec<DueRow> = sqlx::query_as(&format!(
            "UPDATE agent_instructions i \
                SET next_run = now() + make_interval(mins => i.repeat_minutes), \
                    last_fired_at = now() \
              WHERE (i.tenant_id, i.id) IN ( \
                SELECT i2.tenant_id, i2.id FROM agent_instructions i2 \
                  JOIN chat_channels c ON c.tenant_id = i2.tenant_id AND c.id = i2.channel_id \
                 WHERE i2.trigger_kind = 'schedule' AND i2.paused_at IS NULL \
                   AND i2.next_run <= now() \
                   {CLAIM_GUARDS} \
                 ORDER BY i2.next_run LIMIT $1) \
              RETURNING i.tenant_id, i.id, i.agent_id, i.channel_id, i.author_id, i.text"
        ))
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let evented: Vec<DueRow> = sqlx::query_as(&format!(
            "UPDATE agent_instructions i \
                SET last_fired_at = now() \
              WHERE (i.tenant_id, i.id) IN ( \
                SELECT i2.tenant_id, i2.id FROM agent_instructions i2 \
                  JOIN chat_channels c ON c.tenant_id = i2.tenant_id AND c.id = i2.channel_id \
                 WHERE i2.trigger_kind = 'event' AND i2.paused_at IS NULL \
                   AND (i2.last_fired_at IS NULL \
                        OR i2.last_fired_at <= now() - interval '1 hour') \
                   AND EXISTS (SELECT 1 FROM events e \
                                WHERE e.tenant_id = i2.tenant_id AND e.kind = i2.event_kind \
                                  AND e.created_at > COALESCE(i2.last_fired_at, i2.created_at)) \
                   {CLAIM_GUARDS} \
                 LIMIT $1) \
              RETURNING i.tenant_id, i.id, i.agent_id, i.channel_id, i.author_id, i.text"
        ))
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        // A one-shot is claimed by deletion: the row IS the pending firing,
        // so taking it out and firing it are one atomic step — no second
        // sweep can claim it, and no fired assignment lingers as a card.
        let once: Vec<DueRow> = sqlx::query_as(&format!(
            "DELETE FROM agent_instructions i \
              WHERE (i.tenant_id, i.id) IN ( \
                SELECT i2.tenant_id, i2.id FROM agent_instructions i2 \
                  JOIN chat_channels c ON c.tenant_id = i2.tenant_id AND c.id = i2.channel_id \
                 WHERE i2.trigger_kind = 'once' AND i2.paused_at IS NULL \
                   AND i2.next_run <= now() \
                   {CLAIM_GUARDS} \
                 ORDER BY i2.next_run LIMIT $1) \
              RETURNING i.tenant_id, i.id, i.agent_id, i.channel_id, i.author_id, i.text"
        ))
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let mut due = due_of(scheduled);
        due.extend(due_of(evented));
        due.extend(due_of(once));
        Ok(due)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_words_are_trimmed_and_bounded() {
        assert_eq!(
            validate_text("  chase overdue invoices  ").unwrap(),
            "chase overdue invoices"
        );
        assert!(validate_text("   ").is_err());
        assert!(validate_text(&"a".repeat(INSTRUCTION_TEXT_MAX)).is_ok());
        assert!(validate_text(&"a".repeat(INSTRUCTION_TEXT_MAX + 1)).is_err());
    }
}
