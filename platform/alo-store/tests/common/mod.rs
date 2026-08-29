//! Shared test harness. Each test gets its **own** small pool (a
//! `PgPool` must not be shared across separate `#[tokio::test]`
//! runtimes — the pool's tasks die with the runtime that made it).
//! Tests use fresh random tenants, so they never collide in the shared
//! Postgres from compose.
#![allow(clippy::unwrap_used, clippy::expect_used)]
// Each test binary includes this module and uses only a subset of the
// helpers; the rest are legitimately unused there.
#![allow(dead_code)]

use alo_store::{AccountStore, BlobStore, MailboxId, MessageId, Store, UserId};
use sqlx::postgres::PgPoolOptions;

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
pub fn database_url() -> String {
    alo_test_db::url()
}

/// A migrated store on a small, test-local pool.
pub async fn test_store() -> Store {
    test_store_with_blobs().await.0
}

/// A migrated store plus a clone of its blob handle (so a test can plant
/// bytes directly — used by the crash-safety suite).
pub async fn test_store_with_blobs() -> (Store, BlobStore) {
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url())
        .await
        .expect("connect to test postgres (is compose up? is DATABASE_URL set?)");
    let blobs = BlobStore::in_memory(25 * 1024 * 1024);
    let store = Store::new(pool, blobs.clone());
    store.migrate().await.expect("run migrations");
    (store, blobs)
}

/// Creates a fresh tenant with one user and their inbox (no messages),
/// returning the **account door** for that user.
pub async fn fresh_account(store: &Store, tag: &str) -> (AccountStore, UserId, MailboxId) {
    let tenant = store.create_tenant(&format!("t-{tag}")).await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let user = ts
        .create_user(&format!("u-{tag}@example.test"))
        .await
        .unwrap();
    let acc = store.for_account(tenant, user.clone());
    let inbox = acc.inbox().await.unwrap();
    (acc, user, inbox)
}

/// Delivers a synthetic message with a chosen `Message-ID`, references,
/// and subject into `inbox`, through the account door.
pub async fn deliver(
    acc: &AccountStore,
    inbox: &MailboxId,
    message_id: &str,
    references: &[&str],
    subject: &str,
) -> MessageId {
    let refs_hdr = if references.is_empty() {
        String::new()
    } else {
        format!("References: {}\r\n", references.join(" "))
    };
    let raw = format!(
        "From: sender@example.test\r\nSubject: {subject}\r\nMessage-ID: {message_id}\r\n\
         {refs_hdr}\r\nbody for {message_id}\r\n"
    );
    acc.ingest(inbox, raw.as_bytes()).await.unwrap()
}

/// A fully set-up tenant: a user, their inbox, and one ingested message,
/// reachable through the account door.
pub struct Fixture {
    pub acc: AccountStore,
    pub user: UserId,
    pub inbox: MailboxId,
    pub message: MessageId,
}

/// Builds a fresh tenant with one delivered message. `tag` disambiguates
/// the message-id/subject across fixtures.
pub async fn tenant_fixture(store: &Store, tag: &str) -> Fixture {
    let tenant = store.create_tenant(&format!("tenant-{tag}")).await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let email = format!("user-{tag}@example.test");
    let user = ts.create_user(&email).await.unwrap();
    let acc = store.for_account(tenant, user.clone());
    let inbox = acc.inbox().await.unwrap();
    let raw = format!(
        "From: sender@example.test\r\nTo: {email}\r\nSubject: hello {tag}\r\n\
         Message-ID: <{tag}@example.test>\r\n\r\nbody of {tag}\r\n"
    );
    let message = acc.ingest(&inbox, raw.as_bytes()).await.unwrap();
    Fixture {
        acc,
        user,
        inbox,
        message,
    }
}

/// Seeds the default chart of accounts for `account`'s tenant, with plain
/// per-code test names.
///
/// Issuing a document **books it** in the same transaction (B7.01), so any
/// test that issues an invoice, records a payment or confirms a bank match
/// needs a chart the booking can resolve its roles against — exactly the
/// setup step a real tenant performs by opening the Accounts screen once.
pub fn default_chart_seed() -> alo_store::ChartSeed {
    alo_store::ChartSeed {
        names: alo_store::CHART
            .iter()
            .map(|entry| alo_store::ChartName {
                code: entry.code.to_owned(),
                name: format!("Account {}", entry.code),
            })
            .collect(),
    }
}

pub async fn seed_default_chart(account: &AccountStore) {
    let seed = default_chart_seed();
    account
        .fin_accounts_or_seed(&seed, false)
        .await
        .expect("seed the default chart");
}
