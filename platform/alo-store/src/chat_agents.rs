//! Agents as chat participants (ADR 0034 §chat, ADR 0038;
//! `docs/design/chat-agents.md`).
//!
//! An agent has an **identity** and no **authority**. It posts under its own
//! name, and every turn it takes runs through the account door of the person
//! who asked it — this module is reached from an [`AccountStore`], so there is
//! no other door available. There is no agent credential anywhere in this
//! type: an agent cannot authenticate, cannot be a caller, and cannot see one
//! thing more than the human who summoned it.
//!
//! That is ADR 0034's "an agent cannot widen access" made structural rather
//! than promised. It also bounds prompt injection: a hostile message in a
//! channel can only ever reach as far as the person who triggered the turn
//! could already reach.

use serde_json::Value;
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::agent_product::AgentProduct;
use crate::error::{Result, StoreError};
use crate::id::{ChatAgentId, ChatChannelId, ChatMessageId, ChatProposalId, UserId};

/// A handle is what people type after `@`; it must read like one.
const HANDLE_MAX: usize = 32;

/// An agent that can be named in a conversation.
#[derive(Debug, Clone)]
pub struct ChatAgent {
    /// Opaque id. What `chat_messages.author_id` holds for its messages.
    pub id: ChatAgentId,
    /// Typed after `@`, lowercase, unique in the tenant.
    pub handle: String,
    /// Shown in the feed beside its messages.
    pub name: String,
    /// One line: what asking it is good for.
    pub description: Option<String>,
    /// The product it is the agent **of** (ADR 0034, migration 0401).
    ///
    /// Not decoration: it decides which tools the prompt offers it and which
    /// ones the execution boundary refuses it. [`AgentProduct::Workspace`] is
    /// "Ask alo" and is the only value that gets all of them.
    pub product: AgentProduct,
    /// Retired agents keep their past messages but take no new turns.
    pub disabled: bool,
}

/// What state a proposal is in. A proposal is only ever decided once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalState {
    /// Waiting for the asker's tap.
    Pending,
    /// Approved and executed.
    Approved,
    /// Turned down.
    Discarded,
    /// Aged out without a decision.
    Expired,
}

impl ProposalState {
    /// The token stored in the `state` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Discarded => "discarded",
            Self::Expired => "expired",
        }
    }

    pub(crate) fn parse(token: &str) -> Result<Self> {
        match token {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "discarded" => Ok(Self::Discarded),
            "expired" => Ok(Self::Expired),
            other => Err(StoreError::Validation(format!(
                "unknown proposal state {other}"
            ))),
        }
    }
}

/// An action an agent has proposed, waiting for a tap.
#[derive(Debug, Clone)]
pub struct ChatProposal {
    pub id: ChatProposalId,
    pub channel: ChatChannelId,
    /// The agent's message carrying it, so it renders in place.
    pub message: ChatMessageId,
    /// The person whose words caused it — and the only person who may
    /// approve it.
    pub asked_by: UserId,
    pub tool: String,
    pub args: Value,
    pub state: ProposalState,
    pub decided_by: Option<UserId>,
    pub created_at: OffsetDateTime,
}

fn validate_handle(handle: &str) -> Result<String> {
    let handle = handle.trim().trim_start_matches('@').to_lowercase();
    if handle.is_empty() {
        return Err(StoreError::Validation("an agent needs a handle".to_owned()));
    }
    if handle.chars().count() > HANDLE_MAX {
        return Err(StoreError::Validation(format!(
            "a handle is at most {HANDLE_MAX} characters"
        )));
    }
    // The same characters `parse_handles` will accept after an '@'; a handle
    // nobody can type is not a handle.
    if !handle
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(StoreError::Validation(
            "a handle uses letters, digits, dot, dash or underscore".to_owned(),
        ));
    }
    Ok(handle)
}

type AgentRow = (
    String,
    String,
    String,
    Option<String>,
    String,
    Option<OffsetDateTime>,
);

/// The columns every agent query selects, in the order [`row_to_agent`] reads
/// them. One list, so a query cannot forget the product and get an agent this
/// code would then have to guess the scope of.
///
/// Written for the `a` alias every agent query gives `chat_agents`, because
/// [`AGENT_VISIBLE`] correlates on `a.product` and a query without the alias
/// could not carry the gate.
const AGENT_COLUMNS: &str = "a.id, a.handle, a.name, a.description, a.product, a.disabled_at";

/// Whether an agent is the caller's to see — **the module gate, stated once**
/// (queue item A1.5).
///
/// An agent is the agent *of* a product (migration 0401), and a product is a
/// rail module a tenant admin can switch off per person (migration 0208). So a
/// person who cannot open Inventory has no `@inventory`: not a hidden button,
/// no agent — not in the list, not by id, not in a room they share with someone
/// who does have it, and not as the counterpart of a one-to-one they opened
/// before the switch was thrown.
///
/// **The existing gate, asked of a new subject — not a second one.** The join
/// is on the agent's product because the two vocabularies are the same words by
/// construction (`AgentProduct::as_str` == `AppModule::as_str`, held by a test
/// in [`crate::agent_product`]), and the two products that are not modules —
/// `mail` and `workspace` — are exactly the two the 0208 CHECK will not store,
/// so they can never match a denial row and are always visible. That is the
/// right answer for both: mail is the account itself, and Ask alo is the
/// workspace agent whose own scope is already whatever its human can reach.
///
/// **The one product whose word is not its module's is translated here.**
/// `sheets` is a product with no rail app: a spreadsheet is a Drive node, so
/// the switch that decides whether somebody may open one is Drive's
/// ([`AgentProduct::module`]). Left untranslated, the join would compare
/// `sheets` against a column that can never hold it, and a person denied Drive
/// would keep `@sheets` — the exact failure A1.5 exists to prevent, and a
/// silent one. [`AGENT_GATE`] is the mapping, and
/// `only_the_two_drive_documents_are_gated_on_another_products_module` in
/// [`crate::agent_product`] plus the test below keep it from falling behind a
/// product added later.
///
/// `NOT u.is_admin` is [`crate::AccessFacts::may_open`]'s admin arm, spelled in
/// SQL rather than repeated as a judgement here: an administrator is never
/// denied, because an administrator who switched an app off for themselves must
/// still be able to reach the console that switches it back on.
///
/// Every query that pastes this in binds `$1` = tenant and `$2` = caller first,
/// in that order, and starts its own parameters at `$3`.
const AGENT_VISIBLE: &str = "NOT EXISTS ( \
       SELECT 1 FROM tenant_user_module_denials d \
         JOIN users u ON u.tenant_id = d.tenant_id AND u.id = d.user_id \
        WHERE d.tenant_id = $1 AND d.user_id = $2 \
          AND d.module = AGENT_GATE AND NOT u.is_admin)";

/// The rail module an agent's product is gated on, in SQL — `a.product` for
/// every product whose word *is* its module's, and the module's word for the
/// two that are not: a spreadsheet and a document are both Drive nodes, so
/// Drive's switch gates both agents (see [`AGENT_VISIBLE`]).
const AGENT_GATE: &str =
    "(CASE a.product WHEN 'sheets' THEN 'drive' WHEN 'docs' THEN 'drive' ELSE a.product END)";

/// [`AGENT_VISIBLE`] with the gate spliced in — what every query actually
/// pastes. A function of two consts rather than one written-out string so the
/// mapping appears once and the test below can read it.
fn agent_visible() -> String {
    AGENT_VISIBLE.replace("AGENT_GATE", AGENT_GATE)
}

/// Read one row.
///
/// **Fails rather than guesses at an unreadable product.** A word the CHECK
/// allowed but this binary cannot read means the database is ahead of it — a
/// rolling deploy — and the two available guesses are both wrong in a way that
/// matters: `workspace` would hand every tool to an agent somebody deliberately
/// scoped, and a narrower guess would silently take tools away from a working
/// room. That is the opposite direction from
/// [`crate::user_modules::modules_from_words`], which drops an unknown module
/// and so fails open, and the difference is the stakes: there an unknown word
/// withholds an app for a few minutes, here it would decide a permission.
fn row_to_agent(row: AgentRow) -> Result<ChatAgent> {
    Ok(ChatAgent {
        id: ChatAgentId::new(row.0),
        handle: row.1,
        name: row.2,
        description: row.3,
        product: AgentProduct::parse(&row.4)?,
        disabled: row.5.is_some(),
    })
}

fn rows_to_agents(rows: Vec<AgentRow>) -> Result<Vec<ChatAgent>> {
    rows.into_iter().map(row_to_agent).collect()
}

impl AccountStore {
    /// Define an agent for this tenant, as the agent of one product.
    ///
    /// `product` is what scopes it (ADR 0034): the tools its prompt offers and
    /// the ones the execution boundary refuses it. It is a required argument
    /// and has no default, because the only sensible default would be
    /// [`AgentProduct::Workspace`] — every tool — and a caller who forgot to
    /// say would silently get the widest agent there is.
    ///
    /// # Errors
    /// [`StoreError::Validation`] for a bad handle, an empty name, or a product
    /// this caller may not open; [`StoreError::Conflict`] if the handle is
    /// taken.
    pub async fn create_agent(
        &self,
        handle: &str,
        name: &str,
        description: Option<&str>,
        product: AgentProduct,
    ) -> Result<ChatAgentId> {
        let handle = validate_handle(handle)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Validation("an agent needs a name".to_owned()));
        }
        // Said plainly rather than answered with the agent's id, because
        // [`AccountStore::agent`] would then refuse to hand back the thing this
        // call just made: an agent of a module this person cannot open is not
        // theirs to see, and a 201 followed by a 404 is worse than a refusal
        // that says why. Not a second gate — the same one, asked before the
        // write instead of after it.
        if let Some(module) = product.module()
            && !self.access_facts().await?.may_open(module)
        {
            return Err(StoreError::Validation(format!(
                "@{handle} would be the {product} agent, and this account cannot open {product}"
            )));
        }
        let id = ChatAgentId::generate();
        let done = sqlx::query(
            "INSERT INTO chat_agents (tenant_id, id, handle, name, description, product) \
             VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&handle)
        .bind(name)
        .bind(description.map(str::trim).filter(|d| !d.is_empty()))
        .bind(product.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::Conflict(format!("@{handle} is already taken")));
        }
        Ok(id)
    }

    /// Every agent this tenant has that this caller may see, retired ones
    /// included — the composer's `@` list filters, the member sheet shows.
    ///
    /// Gated by [`AGENT_VISIBLE`]: an agent of a module an admin has switched
    /// off for this person is simply not here. To seed the tenant's default set
    /// on a first read, call [`AccountStore::agents_or_seed`] instead.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn agents(&self) -> Result<Vec<ChatAgent>> {
        let visible = agent_visible();
        let rows: Vec<AgentRow> = sqlx::query_as(&format!(
            "SELECT {AGENT_COLUMNS} FROM chat_agents a \
             WHERE a.tenant_id = $1 AND {visible} ORDER BY a.handle"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows_to_agents(rows)
    }

    /// One agent of this tenant, if it is this caller's to see.
    ///
    /// The chokepoint for the module gate on every path that reaches an agent
    /// by id: [`AccountStore::add_agent_to_channel`] and
    /// [`AccountStore::open_agent_dm`] both come through here, so neither
    /// repeats the rule.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if there is no such agent here — including one
    /// that exists but belongs to a module this caller may not open, which is
    /// the same answer an id that was never issued gets, so the refusal is
    /// never an oracle either.
    pub async fn agent(&self, id: &ChatAgentId) -> Result<ChatAgent> {
        let visible = agent_visible();
        let row: Option<AgentRow> = sqlx::query_as(&format!(
            "SELECT {AGENT_COLUMNS} FROM chat_agents a \
             WHERE a.tenant_id = $1 AND a.id = $3 AND {visible}"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row_to_agent(row.ok_or(StoreError::NotFound)?)
    }

    /// The agents in a room this caller may see, so a mention can be resolved
    /// against them.
    ///
    /// Gated by [`AGENT_VISIBLE`] as well as by the room, which is what stops
    /// a shared room becoming a way round the module switch: a colleague who
    /// still has Inventory can put `@inventory` in a channel and be answered
    /// there, and to the person who was denied it the same room simply has no
    /// such member to name. Their `@inventory` resolves to nobody and no turn
    /// is taken.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's to see.
    pub async fn channel_agents(&self, channel: &ChatChannelId) -> Result<Vec<ChatAgent>> {
        self.channel(channel).await?;
        let visible = agent_visible();
        let rows: Vec<AgentRow> = sqlx::query_as(&format!(
            "SELECT {AGENT_COLUMNS} \
             FROM chat_agents a \
             JOIN chat_agent_members m \
               ON m.tenant_id = a.tenant_id AND m.agent_id = a.id \
             WHERE a.tenant_id = $1 AND m.channel_id = $3 AND {visible} \
             ORDER BY a.handle"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(channel.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows_to_agents(rows)
    }

    /// Put an agent in a room. Membership is a member's business, the same as
    /// inviting a person.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room or agent is not the caller's to
    /// see, or they are not a member of it.
    pub async fn add_agent_to_channel(
        &self,
        channel: &ChatChannelId,
        agent: &ChatAgentId,
    ) -> Result<()> {
        self.channel(channel).await?;
        if self.channel_role(channel).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let known = self.agent(agent).await?;
        if known.disabled {
            return Err(StoreError::Validation(format!(
                "@{} is retired and takes no new turns",
                known.handle
            )));
        }
        sqlx::query(
            "INSERT INTO chat_agent_members (tenant_id, channel_id, agent_id, added_by) \
             VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(agent.as_str())
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Take an agent out of a room. Its past messages stay: a room's history
    /// does not change because somebody left.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's, or they are
    /// not a member.
    pub async fn remove_agent_from_channel(
        &self,
        channel: &ChatChannelId,
        agent: &ChatAgentId,
    ) -> Result<()> {
        self.channel(channel).await?;
        if self.channel_role(channel).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "DELETE FROM chat_agent_members \
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

impl AccountStore {
    /// Colleagues whose address matches `query` — for choosing someone to
    /// start a conversation with.
    ///
    /// A search and not a listing: see
    /// [`find_people`](crate::identity::find_people). Never includes the
    /// caller, and never reaches outside their tenant.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn find_people(&self, query: &str, limit: i64) -> Result<Vec<(UserId, String)>> {
        crate::identity::find_people(
            &self.pool,
            self.tenant.as_str(),
            query,
            self.user.as_str(),
            limit,
        )
        .await
    }
}

/// What an agent has actually done — the difference between a feature and a
/// colleague.
#[derive(Debug, Clone, Default)]
pub struct AgentRecord {
    /// Times it has answered.
    pub answers: i64,
    /// Actions it proposed that someone approved and that therefore ran.
    pub actions: i64,
    /// Reading tools it ran **for the caller**, inside a turn and with nobody's
    /// approval (ADR 0047 §4).
    ///
    /// Counted from `agent_tool_runs` rather than from proposals, because a
    /// read no longer makes one. Without this, eleven of the thirty-three
    /// tools would run leaving nothing behind and this record — the one
    /// surface that says what an agent has done — would quietly under-report a
    /// third of its work.
    pub reads: i64,
    /// When it last said anything.
    pub last_at: Option<OffsetDateTime>,
}

impl AccountStore {
    /// What each agent has done, keyed by agent id.
    ///
    /// **Counted only within rooms the caller can see** — a member of one
    /// channel must not learn from a tally that an agent is busy in a private
    /// room they were never in. Aggregates leak too, just more slowly, and the
    /// rule everywhere else in chat is that you see what you could already
    /// read.
    ///
    /// One query for every agent rather than one each: this is drawn beside a
    /// list.
    ///
    /// The reads are a second query rather than another join, because they are
    /// counted over a different population: what an agent *said* is scoped by
    /// the rooms the caller can see, while what it *ran* is scoped to the
    /// caller's own runs — a read happens through one person's access and a
    /// colleague must not learn from a tally which diaries were opened for
    /// them. Folding both into one `GROUP BY` would multiply the two counts
    /// together.
    ///
    /// An agent that has only ever read still appears here: its row comes from
    /// the reads alone, so eleven tools' worth of work is not invisible until
    /// the agent happens to also speak.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn agent_records(&self) -> Result<std::collections::HashMap<String, AgentRecord>> {
        let rows: Vec<(String, i64, i64, Option<OffsetDateTime>)> = sqlx::query_as(
            "SELECT m.author_id, \
                    count(*) FILTER (WHERE m.deleted_at IS NULL) AS answers, \
                    count(p.id) FILTER (WHERE p.state = 'approved') AS actions, \
                    max(m.created_at) AS last_at \
             FROM chat_messages m \
             JOIN chat_channels c \
               ON c.tenant_id = m.tenant_id AND c.id = m.channel_id \
             LEFT JOIN chat_proposals p \
               ON p.tenant_id = m.tenant_id AND p.message_id = m.id \
             WHERE m.tenant_id = $1 AND m.author_kind = 'agent' \
               AND ( \
                 EXISTS (SELECT 1 FROM chat_members mm \
                         WHERE mm.tenant_id = c.tenant_id AND mm.channel_id = c.id \
                           AND mm.user_id = $2) \
                 OR (c.visibility = 'public' AND c.archived_at IS NULL)) \
             GROUP BY m.author_id",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut records: std::collections::HashMap<String, AgentRecord> = rows
            .into_iter()
            .map(|(agent, answers, actions, last_at)| {
                (
                    agent,
                    AgentRecord {
                        answers,
                        actions,
                        reads: 0,
                        last_at,
                    },
                )
            })
            .collect();
        for (agent, reads) in self.agent_read_counts().await? {
            records.entry(agent).or_default().reads = reads;
        }
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_product::ALL_AGENT_PRODUCTS;

    /// The module gate lives in two places — [`AgentProduct::module`] in Rust
    /// and [`AGENT_GATE`] in SQL — and only the SQL one runs. This holds them
    /// in step *by reading the Rust one*: every product whose word is not its
    /// module's must be translated in the CASE, and a product whose word is its
    /// module's must not appear there at all (a stray arm would gate an agent
    /// on somebody else's switch).
    ///
    /// A product added later with a borrowed module fails here rather than in
    /// production, where the symptom would be an agent a denial no longer
    /// hides — silent, and on the permission side.
    #[test]
    fn the_sql_gate_translates_exactly_the_products_whose_word_is_not_their_module() {
        for product in ALL_AGENT_PRODUCTS {
            let arm = format!("WHEN '{}' THEN", product.as_str());
            match product.module() {
                Some(module) if module.as_str() != product.as_str() => {
                    assert!(
                        AGENT_GATE.contains(&format!("{arm} '{}'", module.as_str())),
                        "{product} is gated on {module} in Rust and on nothing in SQL"
                    );
                }
                _ => assert!(
                    !AGENT_GATE.contains(&arm),
                    "{product} is translated in SQL but is its own module in Rust"
                ),
            }
        }
        // …and the spliced predicate is what the queries paste: no placeholder
        // survives into a statement, and the bound parameters are untouched.
        let visible = agent_visible();
        assert!(!visible.contains("AGENT_GATE"), "{visible}");
        assert!(visible.contains(AGENT_GATE), "{visible}");
        assert!(
            visible.contains("d.tenant_id = $1 AND d.user_id = $2"),
            "{visible}"
        );
        assert!(visible.contains("NOT u.is_admin"), "{visible}");
    }
}
