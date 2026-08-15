//! The product an agent belongs to (ADR 0034, A1.2, migration 0401).
//!
//! The value everything else scopes by: the tools the prompt offers, and the
//! refusal at the execution boundary. So the properties that matter here are
//! that it is stored as given, comes back as given, and — like every other fact
//! in this store — belongs to exactly one tenant.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{ALL_AGENT_PRODUCTS, AgentProduct, ChatAgentId, StoreError};

/// Every accepted word survives a round trip through the column, including the
/// three whose agents have no tools yet. A word the CHECK refuses would fail
/// here rather than in a room.
#[tokio::test]
async fn every_product_is_stored_and_read_back_as_itself() {
    let store = common::test_store().await;
    let t = store.create_tenant("agentproduct-t").await.unwrap();
    let ua = store
        .for_tenant(t.clone())
        .create_user("anna@agentproduct.test")
        .await
        .unwrap();
    let a = store.for_account(t.clone(), ua.clone());

    for product in ALL_AGENT_PRODUCTS {
        let id = a
            .create_agent(product.as_str(), product.as_str(), None, product)
            .await
            .unwrap();
        assert_eq!(a.agent(&id).await.unwrap().product, product);
    }

    // And the listing carries it too — the surface a client reads to put an
    // agent beside its module.
    let listed = a.agents().await.unwrap();
    assert_eq!(listed.len(), ALL_AGENT_PRODUCTS.len());
    for agent in listed {
        assert_eq!(agent.product, AgentProduct::parse(&agent.handle).unwrap());
    }
}

/// A product word nobody accepts never reaches the database. The refusal
/// matters more than it looks: the alternative to refusing is defaulting, and
/// the only sensible default is `workspace` — every tool in the workspace.
#[tokio::test]
async fn a_product_nobody_declared_is_refused_before_the_insert() {
    for stranger in ["", "payroll", "Mail", "workspace ; drop", "all"] {
        assert!(
            matches!(
                AgentProduct::parse(stranger),
                Err(StoreError::Validation(_))
            ),
            "{stranger:?} must not name a product"
        );
    }
}

/// **The wrong-tenant test.** An agent is one tenant's, and so is its product:
/// tenant B holding tenant A's agent id learns nothing from it — not the
/// handle, not the product, not that the id exists at all.
#[tokio::test]
async fn an_agent_and_its_product_are_never_another_tenants() {
    let store = common::test_store().await;

    let t1 = store.create_tenant("agentproduct-iso1").await.unwrap();
    let ua = store
        .for_tenant(t1.clone())
        .create_user("anna@iso1.test")
        .await
        .unwrap();
    let a = store.for_account(t1.clone(), ua.clone());

    let t2 = store.create_tenant("agentproduct-iso2").await.unwrap();
    let uc = store
        .for_tenant(t2.clone())
        .create_user("carol@iso2.test")
        .await
        .unwrap();
    let c = store.for_account(t2.clone(), uc.clone());

    let hers = a
        .create_agent("hr", "People", None, AgentProduct::Hr)
        .await
        .unwrap();
    assert_eq!(a.agent(&hers).await.unwrap().product, AgentProduct::Hr);

    // The other tenant, holding the exact id, gets the same answer an id that
    // was never issued gets — so the refusal is not an oracle either.
    assert!(matches!(c.agent(&hers).await, Err(StoreError::NotFound)));
    assert!(matches!(
        c.agent(&ChatAgentId::new("never-issued".to_owned())).await,
        Err(StoreError::NotFound)
    ));
    assert!(c.agents().await.unwrap().is_empty());

    // The same handle in the other tenant is a different agent with its own
    // product: a handle is unique per tenant, not globally, and nothing about
    // tenant A's HR agent follows it across.
    let ours = c
        .create_agent("hr", "People", None, AgentProduct::Workspace)
        .await
        .unwrap();
    assert_ne!(ours.as_str(), hers.as_str());
    assert_eq!(
        c.agent(&ours).await.unwrap().product,
        AgentProduct::Workspace
    );
    assert_eq!(a.agent(&hers).await.unwrap().product, AgentProduct::Hr);
}
