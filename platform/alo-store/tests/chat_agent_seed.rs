//! The default agent set and the module gate (queue item A1.5).
//!
//! Two properties, and they are the item:
//!
//! 1. A tenant **gets** its agents — one per product, on the first read of the
//!    list, with nobody having registered a handle by hand — and gets them once.
//! 2. A module a person cannot open **has no agent** for them, on every surface
//!    that can reach one: the list, an id, a shared room, a one-to-one they
//!    already opened, and making one.
//!
//! The gate is the admin console's existing per-user app switch (migration
//! 0208) asked of the agent's product, so these tests also pin the two edges
//! that switch has: a tenant admin is never denied, and `mail`/`workspace` have
//! no denial row to be found by.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    ALL_AGENT_PRODUCTS, AgentProduct, AgentSeed, AgentWords, AppModule, ChatAgent, ChatAgentId,
    StoreError, default_handle,
};

fn assert_not_found<T: std::fmt::Debug>(r: Result<T, StoreError>) {
    assert!(
        matches!(r, Err(StoreError::NotFound)),
        "expected NotFound, got {r:?}"
    );
}

fn assert_validation<T: std::fmt::Debug>(r: Result<T, StoreError>) {
    assert!(
        matches!(r, Err(StoreError::Validation(_))),
        "expected Validation, got {r:?}"
    );
}

/// A seed shaped like the one the API edge builds, in a language-free form: the
/// store only ever checks that every product has a non-empty name.
fn seed() -> AgentSeed {
    AgentSeed {
        agents: ALL_AGENT_PRODUCTS
            .into_iter()
            .map(|product| AgentWords {
                product,
                name: format!("{product} agent"),
                description: format!("what asking the {product} agent is good for"),
            })
            .collect(),
    }
}

fn by_handle<'a>(agents: &'a [ChatAgent], handle: &str) -> Option<&'a ChatAgent> {
    agents.iter().find(|a| a.handle == handle)
}

/// The item's first half: nobody registers a handle, and the tenant has an
/// agent for every product it owns.
#[tokio::test]
async fn a_tenant_gets_an_agent_for_every_product_without_anyone_registering_one() {
    let store = common::test_store().await;
    let t = store.create_tenant("agentseed-first").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let u = ts.create_user("anna@agentseed.test").await.unwrap();
    let a = store.for_account(t.clone(), u.clone());

    // Nothing before it is asked for, and the ledger says so.
    assert!(a.agents().await.unwrap().is_empty());
    assert!(!a.agent_seed_ran(alo_store::AGENT_SEED_KEY).await.unwrap());

    let agents = a.agents_or_seed(&seed()).await.unwrap();
    assert_eq!(agents.len(), ALL_AGENT_PRODUCTS.len());
    for product in ALL_AGENT_PRODUCTS {
        let handle = default_handle(product);
        let agent = by_handle(&agents, handle)
            .unwrap_or_else(|| panic!("no @{handle} — {product} has no agent"));
        assert_eq!(
            agent.product, product,
            "@{handle} is scoped to the wrong product"
        );
        assert_eq!(agent.name, format!("{product} agent"));
        assert_eq!(
            agent.description.as_deref(),
            Some(format!("what asking the {product} agent is good for").as_str())
        );
        assert!(!agent.disabled);
    }
    // The workspace agent is the one whose handle is not its product word.
    assert_eq!(
        by_handle(&agents, "alo").unwrap().product,
        AgentProduct::Workspace
    );
    assert!(a.agent_seed_ran(alo_store::AGENT_SEED_KEY).await.unwrap());
}

/// Seeding is a first-use rule, not an every-read one: the ids do not move, and
/// a tenant that has been given its set is not given a second one.
#[tokio::test]
async fn the_set_is_given_once_and_a_later_read_returns_the_same_agents() {
    let store = common::test_store().await;
    let t = store.create_tenant("agentseed-once").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let u = ts.create_user("anna@agentseed-once.test").await.unwrap();
    let a = store.for_account(t.clone(), u.clone());

    let first: Vec<String> = a
        .agents_or_seed(&seed())
        .await
        .unwrap()
        .iter()
        .map(|agent| agent.id.as_str().to_owned())
        .collect();
    let second: Vec<String> = a
        .agents_or_seed(&seed())
        .await
        .unwrap()
        .iter()
        .map(|agent| agent.id.as_str().to_owned())
        .collect();
    assert_eq!(first, second, "a second read made a second set");

    // A colleague's first read is not a first read for the tenant either.
    let ub = ts.create_user("ben@agentseed-once.test").await.unwrap();
    let b = store.for_account(t.clone(), ub.clone());
    let bens: Vec<String> = b
        .agents_or_seed(&seed())
        .await
        .unwrap()
        .iter()
        .map(|agent| agent.id.as_str().to_owned())
        .collect();
    assert_eq!(first, bens, "a colleague's first read seeded a second set");
}

/// Two first reads at the same instant produce exactly one set — the ledger's
/// primary key decides, and the loser reads back what the winner wrote.
#[tokio::test]
async fn two_simultaneous_first_reads_produce_one_set() {
    let store = common::test_store().await;
    let t = store.create_tenant("agentseed-race").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let u = ts.create_user("anna@agentseed-race.test").await.unwrap();
    let one = store.for_account(t.clone(), u.clone());
    let two = one.clone();

    let (left_seed, right_seed) = (seed(), seed());
    let (left, right) = tokio::join!(
        one.agents_or_seed(&left_seed),
        two.agents_or_seed(&right_seed)
    );
    let left = left.unwrap();
    let right = right.unwrap();
    assert_eq!(left.len(), ALL_AGENT_PRODUCTS.len());
    assert_eq!(right.len(), ALL_AGENT_PRODUCTS.len());
    let ids: Vec<&str> = left.iter().map(|a| a.id.as_str()).collect();
    let other: Vec<&str> = right.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, other, "the race produced two sets");
}

/// A tenant whose administrator already registered `@mail` keeps theirs, name
/// and all, and is given the fourteen they were missing.
#[tokio::test]
async fn an_agent_a_tenant_already_had_is_kept_rather_than_replaced() {
    let store = common::test_store().await;
    let t = store.create_tenant("agentseed-kept").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let u = ts.create_user("anna@agentseed-kept.test").await.unwrap();
    let a = store.for_account(t.clone(), u.clone());

    let theirs = a
        .create_agent("mail", "Our post room", Some("ours"), AgentProduct::Mail)
        .await
        .unwrap();

    let agents = a.agents_or_seed(&seed()).await.unwrap();
    assert_eq!(agents.len(), ALL_AGENT_PRODUCTS.len());
    let mail = by_handle(&agents, "mail").unwrap();
    assert_eq!(
        mail.id.as_str(),
        theirs.as_str(),
        "their agent was replaced"
    );
    assert_eq!(mail.name, "Our post room", "their name was overwritten");
}

/// A malformed seed is refused before anything is written — and refused
/// **without** claiming the ledger, so the next well-formed read still seeds.
#[tokio::test]
async fn a_seed_short_a_product_is_refused_and_the_tenant_stays_unseeded() {
    let store = common::test_store().await;
    let t = store.create_tenant("agentseed-short").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let u = ts.create_user("anna@agentseed-short.test").await.unwrap();
    let a = store.for_account(t.clone(), u.clone());

    let mut short = seed();
    short.agents.retain(|w| w.product != AgentProduct::Sites);
    assert_validation(a.agents_or_seed(&short).await);
    assert!(!a.agent_seed_ran(alo_store::AGENT_SEED_KEY).await.unwrap());
    assert!(a.agents().await.unwrap().is_empty());

    assert_eq!(
        a.agents_or_seed(&seed()).await.unwrap().len(),
        ALL_AGENT_PRODUCTS.len()
    );
}

/// The item's second half, and the one that matters: a module switched off for
/// one person yields no agent on any surface that can reach one — while the
/// colleague who still has the module is unaffected.
#[tokio::test]
async fn a_module_a_person_cannot_open_has_no_agent_on_any_surface() {
    let store = common::test_store().await;
    let t = store.create_tenant("agentseed-denied").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let admin = ts.create_user("admin@agentseed-denied.test").await.unwrap();
    ts.set_admin(&admin, true).await.unwrap();
    let ua = ts.create_user("anna@agentseed-denied.test").await.unwrap();
    let ub = ts.create_user("ben@agentseed-denied.test").await.unwrap();
    let a = store.for_account(t.clone(), ua.clone());
    let b = store.for_account(t.clone(), ub.clone());

    let agents = a.agents_or_seed(&seed()).await.unwrap();
    let inventory = by_handle(&agents, "inventory").unwrap().id.clone();
    let mail = by_handle(&agents, "mail").unwrap().id.clone();

    // Anna opens her one-to-one with @inventory *before* the switch is thrown,
    // and shares a room with Ben that @inventory is a member of.
    let dm = a.open_agent_dm(&inventory).await.unwrap();
    let room = a
        .create_channel("stock", Some("Stock"), alo_store::ChannelVisibility::Public)
        .await
        .unwrap();
    a.add_agent_to_channel(&room, &inventory).await.unwrap();
    b.join_channel(&room).await.unwrap();

    ts.set_module_access(&ua, AppModule::Inventory, false, &admin)
        .await
        .unwrap();

    // 1. The list: absent for her, present for him.
    let hers = a.agents().await.unwrap();
    assert!(
        by_handle(&hers, "inventory").is_none(),
        "a denied module still has an agent in the list"
    );
    assert_eq!(hers.len(), ALL_AGENT_PRODUCTS.len() - 1);
    assert!(by_handle(&b.agents().await.unwrap(), "inventory").is_some());

    // 2. By id: the same answer an id that was never issued gets.
    assert_not_found(a.agent(&inventory).await);
    assert!(b.agent(&inventory).await.is_ok());

    // 3. In a shared room: no such member to name, so her `@inventory`
    //    resolves to nobody — while his still does.
    let in_room = a.channel_agents(&room).await.unwrap();
    assert!(by_handle(&in_room, "inventory").is_none());
    assert!(by_handle(&b.channel_agents(&room).await.unwrap(), "inventory").is_some());

    // 4. Her existing one-to-one stays readable and has no counterpart to
    //    answer in it — every message there is now an ordinary message.
    assert!(a.channel(&dm).await.is_ok());
    assert_eq!(a.channel_agent_counterpart(&dm).await.unwrap(), None);

    // 5. Opening one, and putting one in a room, are both refused.
    assert_not_found(a.open_agent_dm(&inventory).await);
    let hers_alone = a
        .create_channel("hers", Some("Hers"), alo_store::ChannelVisibility::Private)
        .await
        .unwrap();
    assert_not_found(a.add_agent_to_channel(&hers_alone, &inventory).await);

    // 6. Making a new one of that product is refused rather than made and
    //    hidden — but any other product is untouched.
    assert_validation(
        a.create_agent("stockroom", "Stockroom", None, AgentProduct::Inventory)
            .await,
    );
    a.create_agent("post", "Post", None, AgentProduct::Mail)
        .await
        .unwrap();

    // 7. Nothing else moved: the other fourteen are hers as before.
    assert!(a.agent(&mail).await.is_ok());
}

/// The two products that are not rail modules can never be denied, because the
/// 0208 CHECK will not store a denial for them. Mail is the account itself, and
/// Ask alo's scope is already whatever its human can reach.
#[tokio::test]
async fn mail_and_the_workspace_agent_have_no_switch_to_be_denied_by() {
    let store = common::test_store().await;
    let t = store.create_tenant("agentseed-nomodule").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let admin = ts
        .create_user("admin@agentseed-nomodule.test")
        .await
        .unwrap();
    ts.set_admin(&admin, true).await.unwrap();
    let ua = ts
        .create_user("anna@agentseed-nomodule.test")
        .await
        .unwrap();
    let a = store.for_account(t.clone(), ua.clone());

    let agents = a.agents_or_seed(&seed()).await.unwrap();
    assert!(AgentProduct::Mail.module().is_none());
    assert!(AgentProduct::Workspace.module().is_none());

    // Every module there *is* switched off, all thirteen of them.
    for module in alo_store::ALL_MODULES {
        ts.set_module_access(&ua, module, false, &admin)
            .await
            .unwrap();
    }
    let left = a.agents().await.unwrap();
    let handles: Vec<&str> = left.iter().map(|agent| agent.handle.as_str()).collect();
    assert_eq!(handles, vec!["alo", "mail"], "left with {handles:?}");
    for agent in &agents {
        if agent.handle == "alo" || agent.handle == "mail" {
            assert!(
                a.agent(&agent.id).await.is_ok(),
                "@{} vanished",
                agent.handle
            );
        } else {
            assert_not_found(a.agent(&agent.id).await);
        }
    }
}

/// A tenant admin is never denied — `AccessFacts::may_open`'s own rule, and
/// the reason it exists: an administrator who switched an app off for
/// themselves must still reach the console that switches it back on.
#[tokio::test]
async fn an_administrator_keeps_every_agent_even_with_a_denial_row() {
    let store = common::test_store().await;
    let t = store.create_tenant("agentseed-admin").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let admin = ts.create_user("admin@agentseed-admin.test").await.unwrap();
    ts.set_admin(&admin, true).await.unwrap();
    let a = store.for_account(t.clone(), admin.clone());

    let agents = a.agents_or_seed(&seed()).await.unwrap();
    let inventory = by_handle(&agents, "inventory").unwrap().id.clone();
    ts.set_module_access(&admin, AppModule::Inventory, false, &admin)
        .await
        .unwrap();

    assert_eq!(a.agents().await.unwrap().len(), ALL_AGENT_PRODUCTS.len());
    assert!(a.agent(&inventory).await.is_ok());
    a.create_agent("stockroom", "Stockroom", None, AgentProduct::Inventory)
        .await
        .unwrap();

    // And the moment they stop being an admin, the denial that was there all
    // along starts applying.
    ts.set_admin(&admin, false).await.unwrap();
    assert_not_found(a.agent(&inventory).await);
}

/// The mandatory wrong-tenant half. A seed is a tenant's own, a denial is a
/// tenant's own, and an agent id from one is not a shortcut into the other.
#[tokio::test]
async fn a_seed_and_a_denial_are_never_another_tenants() {
    let store = common::test_store().await;

    let ta = store.create_tenant("agentseed-a").await.unwrap();
    let tsa = store.for_tenant(ta.clone());
    let admin_a = tsa.create_user("admin@a.test").await.unwrap();
    tsa.set_admin(&admin_a, true).await.unwrap();
    let ua = tsa.create_user("anna@a.test").await.unwrap();
    let a = store.for_account(ta.clone(), ua.clone());

    let tb = store.create_tenant("agentseed-b").await.unwrap();
    let tsb = store.for_tenant(tb.clone());
    let ub = tsb.create_user("ben@b.test").await.unwrap();
    let b = store.for_account(tb.clone(), ub.clone());

    let theirs = a.agents_or_seed(&seed()).await.unwrap();
    let their_inventory = by_handle(&theirs, "inventory").unwrap().id.clone();

    // Seeding A did not seed B, and B's ledger is its own.
    assert!(!b.agent_seed_ran(alo_store::AGENT_SEED_KEY).await.unwrap());
    assert!(b.agents().await.unwrap().is_empty());
    // Nor is A's agent reachable from B by id, before or after B has its own.
    assert_not_found(b.agent(&their_inventory).await);
    assert_not_found(b.open_agent_dm(&their_inventory).await);

    let bens = b.agents_or_seed(&seed()).await.unwrap();
    assert_eq!(bens.len(), ALL_AGENT_PRODUCTS.len());
    let ben_inventory = by_handle(&bens, "inventory").unwrap().id.clone();
    assert_ne!(
        ben_inventory.as_str(),
        their_inventory.as_str(),
        "two tenants share an agent row"
    );
    assert_not_found(b.agent(&their_inventory).await);
    assert_not_found(a.agent(&ben_inventory).await);

    // A denial in A says nothing about B: same module, same handle, other
    // tenant, still there.
    tsa.set_module_access(&ua, AppModule::Inventory, false, &admin_a)
        .await
        .unwrap();
    assert_not_found(a.agent(&their_inventory).await);
    assert!(b.agent(&ben_inventory).await.is_ok());
    assert!(by_handle(&b.agents().await.unwrap(), "inventory").is_some());

    // And an id nobody ever issued is the same answer as a foreign one.
    assert_not_found(b.agent(&ChatAgentId::new("agent-that-never-was")).await);
}
