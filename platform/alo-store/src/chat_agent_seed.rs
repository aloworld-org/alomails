//! The agents a tenant starts with (ADR 0034 "an agent in every product";
//! queue item A1.5).
//!
//! ADR 0034 says every product has an agent. Until this module that was true
//! only of a tenant whose administrator had posted an agent per product to
//! `POST /chat/agents` by hand, handle by handle — a manual step between a new
//! tenant and the feature the product is sold on. So the set is seeded on the
//! first read of the agent list, the way Inventory's locations and Finance's
//! chart of accounts already are: an empty state that is a working state.
//!
//! # This file states the shape, not the words
//!
//! What is here is which agents exist and what each is addressed by — the
//! structural half, identical in every language. A handle is an identifier
//! people type into message text, so it never translates: the Dutch tenant's
//! website agent is `@sites` too. The **names and descriptions** are user-facing
//! strings and are supplied by the caller ([`AgentSeed`]), from the language
//! tables at the API edge, exactly as [`crate::inv_locations`] takes its
//! location names. A hardcoded English name is hardest to see down here.
//!
//! # Seeding is a first-use rule, not an every-read one
//!
//! The question asked is whether the seed has ever *run* for this tenant (the
//! `chat_agent_seeds` ledger, migration 0403), not whether the rows are still
//! there. A tenant that retired an agent we gave them is not handed it back the
//! next morning.

use crate::account::AccountStore;
use crate::agent_product::{ALL_AGENT_PRODUCTS, AgentProduct};
use crate::chat_agents::ChatAgent;
use crate::error::{Result, StoreError};
use crate::id::ChatAgentId;

/// The `chat_agent_seeds` key under which the default set is recorded.
///
/// A key rather than a bare tenant row, so a second seeded thing in chat later
/// (a starting channel, say) is another key and not another table.
pub const AGENT_SEED_KEY: &str = "default-agents";

/// Products whose agent arrived **after** [`AGENT_SEED_KEY`] first ran, and the
/// ledger key each is offered under.
///
/// The default set is written once and never revisited, which is the whole
/// point of the ledger — a tenant that retires `@meet` must not find it back.
/// But that same "once" would leave every existing tenant permanently without
/// the agent of a product built later, and A1.5's promise is that a tenant gets
/// its agents without anybody registering a handle by hand. So a product added
/// after the fact is offered **once, under its own key**: a tenant that has
/// never been offered it gets it, and a tenant that was offered it and threw it
/// away keeps it thrown away.
///
/// A tenant seeded from scratch today already receives every product's agent in
/// the first set, so the top-up finds the handle taken, writes nothing, and
/// records the key — the same end state by either route.
pub const LATER_AGENT_PRODUCTS: &[(AgentProduct, &str)] = &[
    (AgentProduct::Sheets, "default-agents:sheets"),
    (AgentProduct::Docs, "default-agents:docs"),
];

/// The words one default agent is written with, in the language of whoever
/// opened the agent list first.
#[derive(Debug, Clone)]
pub struct AgentWords {
    /// Which product's agent these words describe.
    pub product: AgentProduct,
    /// Shown in the feed beside its messages — the rail's own word for the
    /// module, so the agent and the app a person clicks are recognisably the
    /// same thing.
    pub name: String,
    /// One line: what asking it is good for. May be blank, which stores no
    /// description rather than an empty one.
    pub description: String,
}

/// The set a tenant is seeded with: one entry per product, in any order.
///
/// Every product must be present. A seed missing one is refused rather than
/// silently producing a tenant one agent short, because the missing one
/// would then be missing forever — the ledger records that the seed ran, not
/// which rows it wrote.
#[derive(Debug, Clone)]
pub struct AgentSeed {
    /// One entry per [`ALL_AGENT_PRODUCTS`].
    pub agents: Vec<AgentWords>,
}

/// What a product's default agent is addressed by, after the `@`.
///
/// The product's own word, which is also the rail's id for the module, so the
/// handle is guessable from the app it belongs to. The one exception is
/// [`AgentProduct::Workspace`]: `@workspace` is not what anybody would type at
/// the assistant that works across the whole workspace, and `@alo` is the name
/// the product already uses for it ("Ask alo").
#[must_use]
pub fn default_handle(product: AgentProduct) -> &'static str {
    match product {
        AgentProduct::Workspace => "alo",
        other => other.as_str(),
    }
}

/// One agent as the seed will write it.
#[derive(Debug)]
struct SeedRow {
    product: AgentProduct,
    handle: &'static str,
    name: String,
    description: Option<String>,
}

/// Checks the caller gave words for every product, exactly once, and that each
/// has a name.
///
/// Refused rather than repaired: the two available repairs are both wrong in a
/// way that lasts. Dropping the product without words leaves a tenant
/// permanently short one agent, and inventing a name for it puts an English
/// word in a French tenant's sidebar with nothing to trace it back to.
fn normalize_seed(seed: &AgentSeed) -> Result<Vec<SeedRow>> {
    if seed.agents.len() != ALL_AGENT_PRODUCTS.len() {
        return Err(StoreError::Validation(format!(
            "the default agent set needs one entry per product ({}), and has {}",
            ALL_AGENT_PRODUCTS.len(),
            seed.agents.len()
        )));
    }
    let mut rows = Vec::with_capacity(ALL_AGENT_PRODUCTS.len());
    for product in ALL_AGENT_PRODUCTS {
        // Exactly one, so a duplicate entry for one product — which would leave
        // another with none while the length still matched — is caught here.
        let mut found = seed.agents.iter().filter(|w| w.product == product);
        let words = found.next().ok_or_else(|| {
            StoreError::Validation(format!("the default agent set has no {product} agent"))
        })?;
        if found.next().is_some() {
            return Err(StoreError::Validation(format!(
                "the default agent set names the {product} agent twice"
            )));
        }
        let name = words.name.trim();
        if name.is_empty() {
            return Err(StoreError::Validation(format!(
                "the {product} agent has no name"
            )));
        }
        let description = words.description.trim();
        rows.push(SeedRow {
            product,
            handle: default_handle(product),
            name: name.to_owned(),
            description: (!description.is_empty()).then(|| description.to_owned()),
        });
    }
    Ok(rows)
}

impl AccountStore {
    /// The tenant's agents, **seeding the default set on first use**.
    ///
    /// The agent list's read. A tenant that has never opened it is given an
    /// agent for every product first, so the first thing a person sees is a
    /// working set rather than an empty list and a form asking for a handle.
    ///
    /// What comes back has been through the same module gate every other agent
    /// read is: an agent of a product this caller may not open is seeded (it is
    /// the tenant's, not theirs) and then not returned to them. See
    /// [`AccountStore::agents`].
    ///
    /// Two first reads at the same instant produce exactly one set: the loser
    /// of the race on the ledger's primary key writes nothing and reads back
    /// what the winner wrote.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the seed itself is malformed (a missing,
    /// duplicated or unnamed product); [`StoreError::Db`] on failure.
    pub async fn agents_or_seed(&self, seed: &AgentSeed) -> Result<Vec<ChatAgent>> {
        let rows = normalize_seed(seed)?;
        if !self.agent_seed_ran(AGENT_SEED_KEY).await? {
            match self.seed_agents(&rows).await {
                // A concurrent first read won the ledger: its agents are the
                // tenant's, and reading them back is the whole of the loser's
                // job.
                Ok(()) | Err(StoreError::Conflict(_)) => {}
                Err(other) => return Err(other),
            }
        }
        self.offer_later_agents(&rows).await?;
        self.agents().await
    }

    /// Offers, once each, the agents of products added after the default set
    /// (see [`LATER_AGENT_PRODUCTS`]).
    ///
    /// One transaction per product, each claiming its own ledger key first, so
    /// the race and the retirement rules are the ones
    /// [`AccountStore::seed_agents`] already establishes rather than a second
    /// set of them.
    async fn offer_later_agents(&self, rows: &[SeedRow]) -> Result<()> {
        for (product, key) in LATER_AGENT_PRODUCTS {
            if self.agent_seed_ran(key).await? {
                continue;
            }
            // The caller's own words for it, from the same table as the first
            // set — never a name composed here, which would be one language's
            // word written into every tenant's sidebar.
            let Some(row) = rows.iter().find(|row| row.product == *product) else {
                continue;
            };
            let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
            let claimed = sqlx::query(
                "INSERT INTO chat_agent_seeds (tenant_id, system_key, seeded_by) \
                 VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
            )
            .bind(self.tenant.as_str())
            .bind(*key)
            .bind(self.user.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            if claimed.rows_affected() == 0 {
                // Another read is offering it right now; theirs is this
                // tenant's, and rolling back leaves nothing half-written.
                tx.rollback().await.map_err(StoreError::Db)?;
                continue;
            }
            sqlx::query(
                "INSERT INTO chat_agents (tenant_id, id, handle, name, description, product) \
                 VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING",
            )
            .bind(self.tenant.as_str())
            .bind(ChatAgentId::generate().as_str())
            .bind(row.handle)
            .bind(&row.name)
            .bind(row.description.as_deref())
            .bind(row.product.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            tx.commit().await.map_err(StoreError::Db)?;
        }
        Ok(())
    }

    /// Whether the seed named by `system_key` has ever run for this tenant —
    /// the ledger's question, which survives the rows it wrote.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn agent_seed_ran(&self, system_key: &str) -> Result<bool> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM chat_agent_seeds \
              WHERE tenant_id = $1 AND system_key = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(system_key)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    /// Writes the ledger row and every seeded agent in **one transaction**: a
    /// tenant is never left holding nine of the set, and never left with a
    /// ledger row and no agents.
    ///
    /// Each insert is `ON CONFLICT DO NOTHING` on top of that, so a tenant whose
    /// administrator had already registered `@mail` by hand keeps theirs — their
    /// agent, their name for it, their description — and is given the rest.
    async fn seed_agents(&self, rows: &[SeedRow]) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let claimed = sqlx::query(
            "INSERT INTO chat_agent_seeds (tenant_id, system_key, seeded_by) \
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(AGENT_SEED_KEY)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if claimed.rows_affected() == 0 {
            return Err(StoreError::Conflict(
                "this tenant's agents have already been seeded".to_owned(),
            ));
        }
        for row in rows {
            sqlx::query(
                "INSERT INTO chat_agents (tenant_id, id, handle, name, description, product) \
                 VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING",
            )
            .bind(self.tenant.as_str())
            .bind(ChatAgentId::generate().as_str())
            .bind(row.handle)
            .bind(&row.name)
            .bind(row.description.as_deref())
            .bind(row.product.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn full_seed() -> AgentSeed {
        AgentSeed {
            agents: ALL_AGENT_PRODUCTS
                .into_iter()
                .map(|product| AgentWords {
                    product,
                    name: format!("The {product} one"),
                    description: String::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn every_product_gets_a_handle_and_only_the_workspace_one_is_renamed() {
        for product in ALL_AGENT_PRODUCTS {
            let handle = default_handle(product);
            assert!(!handle.is_empty(), "{product} has no handle");
            if product == AgentProduct::Workspace {
                assert_eq!(handle, "alo");
            } else {
                assert_eq!(handle, product.as_str(), "{product}");
            }
        }
    }

    #[test]
    fn two_products_never_share_a_handle() {
        let mut handles: Vec<&str> = ALL_AGENT_PRODUCTS.into_iter().map(default_handle).collect();
        let total = handles.len();
        handles.sort_unstable();
        handles.dedup();
        assert_eq!(handles.len(), total, "two default agents share a handle");
    }

    #[test]
    fn a_full_seed_normalizes_to_one_row_per_product_in_a_fixed_order() {
        let rows = normalize_seed(&full_seed()).unwrap();
        assert_eq!(rows.len(), ALL_AGENT_PRODUCTS.len());
        for (row, product) in rows.iter().zip(ALL_AGENT_PRODUCTS) {
            assert_eq!(row.product, product);
            assert_eq!(row.handle, default_handle(product));
            // Blank descriptions are stored as no description, not as "".
            assert!(row.description.is_none());
        }
    }

    #[test]
    fn a_seed_that_is_short_a_product_is_refused() {
        let mut seed = full_seed();
        seed.agents.retain(|w| w.product != AgentProduct::Sites);
        assert!(matches!(
            normalize_seed(&seed),
            Err(StoreError::Validation(_))
        ));
    }

    #[test]
    fn a_seed_that_names_one_product_twice_is_refused() {
        let mut seed = full_seed();
        // Same length, so only the per-product check can catch this. The
        // duplicated product is the **first** one and the entry spent on it the
        // last, so the walk reaches the duplicate before it reaches the product
        // that is now missing — otherwise this would read as the missing-product
        // refusal, which its own test already covers.
        let last = seed
            .agents
            .iter_mut()
            .find(|w| w.product == AgentProduct::Workspace)
            .unwrap();
        last.product = AgentProduct::Mail;
        let message = match normalize_seed(&seed) {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected Validation, got {other:?}"),
        };
        assert!(message.contains("twice"), "{message}");
    }

    #[test]
    fn a_seed_with_an_unnamed_product_is_refused() {
        let mut seed = full_seed();
        seed.agents[3].name = "   ".to_owned();
        assert!(matches!(
            normalize_seed(&seed),
            Err(StoreError::Validation(_))
        ));
    }

    #[test]
    fn a_description_survives_but_is_trimmed() {
        let mut seed = full_seed();
        seed.agents[0].description = "  what asking it is good for  ".to_owned();
        let rows = normalize_seed(&seed).unwrap();
        let row = rows
            .iter()
            .find(|r| r.product == seed.agents[0].product)
            .unwrap();
        assert_eq!(
            row.description.as_deref(),
            Some("what asking it is good for")
        );
    }
}
