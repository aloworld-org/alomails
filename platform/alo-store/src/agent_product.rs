//! Which product an agent is the agent *of* (ADR 0034, migration 0401).
//!
//! ADR 0034's decision is that every product has its own agent, and what makes
//! that true is not the handle somebody typed — it is this value. It is read
//! twice, and the second reading is the one that matters:
//!
//! 1. The **prompt** offers an agent its own product's tools, so the Inventory
//!    agent is told about stock and not about payroll.
//! 2. The **execution boundary** refuses every other product's tools, whatever
//!    the model returned. A prompt that asks nicely is not a permission system:
//!    the model is the untrusted party here, and an injected turn will happily
//!    name a tool it was never offered.
//!
//! # Why the words are the rail's module ids
//!
//! [`AppModule`] already answers "which apps may this person open", per user,
//! with an admin console behind it. Sharing its vocabulary is what lets A1.5
//! gate an agent on the access that already exists rather than inventing a
//! second permission system that can disagree with the first. The two places
//! this set is wider are [`Self::Mail`] — mail has no denial row, because a
//! denial there would be a broken account rather than a missing app (see
//! [`AppModule`]) — and [`Self::Workspace`], which is not a product at all.

use crate::error::{Result, StoreError};
use crate::user_modules::AppModule;

/// The product an agent belongs to, or [`Self::Workspace`] for "Ask alo".
///
/// Closed, and deliberately the same set as the CHECK in migration 0401: a
/// stored word this enum cannot read would be an agent nothing could scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AgentProduct {
    Mail,
    Agenda,
    Tasks,
    Chat,
    Drive,
    Billing,
    Crm,
    Projects,
    Finance,
    Inventory,
    Hr,
    Insights,
    Meet,
    Sites,
    /// "Ask alo" — the top-level agent, scoped to no product because its whole
    /// job is to work across them (ADR 0034). **The only value that is offered
    /// every tool**, which is why it is a word of its own rather than the
    /// absence of one: a NULL would make "nobody said" and "deliberately
    /// everything" the same state, and the failure would be silent and wide.
    Workspace,
}

/// Every product an agent can belong to, in the order the directory shows them.
pub const ALL_AGENT_PRODUCTS: [AgentProduct; 15] = [
    AgentProduct::Mail,
    AgentProduct::Agenda,
    AgentProduct::Tasks,
    AgentProduct::Chat,
    AgentProduct::Drive,
    AgentProduct::Billing,
    AgentProduct::Crm,
    AgentProduct::Projects,
    AgentProduct::Finance,
    AgentProduct::Inventory,
    AgentProduct::Hr,
    AgentProduct::Insights,
    AgentProduct::Meet,
    AgentProduct::Sites,
    AgentProduct::Workspace,
];

impl AgentProduct {
    /// The stored word, which is also what the wire carries.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mail => "mail",
            Self::Agenda => "agenda",
            Self::Tasks => "tasks",
            Self::Chat => "chat",
            Self::Drive => "drive",
            Self::Billing => "billing",
            Self::Crm => "crm",
            Self::Projects => "projects",
            Self::Finance => "finance",
            Self::Inventory => "inventory",
            Self::Hr => "hr",
            Self::Insights => "insights",
            Self::Meet => "meet",
            Self::Sites => "sites",
            Self::Workspace => "workspace",
        }
    }

    /// Reads a product word — from a request body or from a stored row.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set. An unknown word is
    /// refused rather than quietly turned into a workspace agent: that mistake
    /// would hand every tool to an agent somebody meant to scope.
    pub fn parse(value: &str) -> Result<Self> {
        ALL_AGENT_PRODUCTS
            .into_iter()
            .find(|product| product.as_str() == value.trim())
            .ok_or_else(|| {
                StoreError::Validation(format!(
                    "product must be one of: {}",
                    ALL_AGENT_PRODUCTS
                        .iter()
                        .map(|product| product.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })
    }

    /// The rail module a person needs to be able to open for this agent to make
    /// sense to them — `None` when there is no such switch.
    ///
    /// Mail cannot be denied (see [`AppModule`]) and Workspace is not a module,
    /// so neither has one. Everything else maps one-to-one, which is the point:
    /// A1.5 gates an agent on the access an admin has already decided, and a
    /// module somebody was denied yields no agent rather than a second list to
    /// keep in step.
    #[must_use]
    pub fn module(self) -> Option<AppModule> {
        match self {
            Self::Agenda => Some(AppModule::Agenda),
            Self::Tasks => Some(AppModule::Tasks),
            Self::Chat => Some(AppModule::Chat),
            Self::Drive => Some(AppModule::Drive),
            Self::Billing => Some(AppModule::Billing),
            Self::Crm => Some(AppModule::Crm),
            Self::Projects => Some(AppModule::Projects),
            Self::Finance => Some(AppModule::Finance),
            Self::Inventory => Some(AppModule::Inventory),
            Self::Hr => Some(AppModule::Hr),
            Self::Insights => Some(AppModule::Insights),
            Self::Meet => Some(AppModule::Meet),
            Self::Sites => Some(AppModule::Sites),
            Self::Mail | Self::Workspace => None,
        }
    }
}

impl std::fmt::Display for AgentProduct {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::user_modules::ALL_MODULES;

    #[test]
    fn every_word_round_trips_and_a_stranger_is_refused() {
        for product in ALL_AGENT_PRODUCTS {
            assert_eq!(AgentProduct::parse(product.as_str()).unwrap(), product);
            // Whitespace a body might carry is trimmed, nothing else is.
            assert_eq!(
                AgentProduct::parse(&format!("  {product}  ")).unwrap(),
                product
            );
        }
        for stranger in ["", "Mail", "workspaces", "payroll", "mail agent"] {
            let err = AgentProduct::parse(stranger).unwrap_err();
            assert!(matches!(err, StoreError::Validation(_)), "{stranger:?}");
        }
    }

    /// The vocabularies must not drift: every rail module is a product, so A1.5
    /// can ask the module gate about any agent it is about to create.
    #[test]
    fn every_rail_module_is_a_product_and_maps_back_to_itself() {
        for module in ALL_MODULES {
            let product = AgentProduct::parse(module.as_str())
                .unwrap_or_else(|_| panic!("{module} has no agent product"));
            assert_eq!(product.module(), Some(module));
        }
        // The two that are deliberately not modules.
        assert_eq!(AgentProduct::Mail.module(), None);
        assert_eq!(AgentProduct::Workspace.module(), None);
        // ...and that is the whole difference between the two sets.
        assert_eq!(ALL_AGENT_PRODUCTS.len(), ALL_MODULES.len() + 2);
    }
}
