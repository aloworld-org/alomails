//! Shared identity test harness over the live Postgres store.
#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::sync::Arc;

use alo_identity::{Identity, IdentityConfig};
use alo_store::{BlobStore, Store, TenantId, UserId};

pub const ISSUER: &str = "https://id.alo.test";

pub fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://alo:alo-dev-only@127.0.0.1:5432/alo".to_owned())
}

/// A migrated store plus an identity with a signing key provisioned.
pub async fn setup() -> (Arc<Store>, Identity) {
    let store = Arc::new(
        Store::connect(&database_url(), BlobStore::in_memory(25 * 1024 * 1024))
            .await
            .expect("connect to test postgres (is DATABASE_URL set / compose up?)"),
    );
    store.migrate().await.unwrap();
    let identity = Identity::new(Arc::clone(&store), IdentityConfig::new(ISSUER)).unwrap();
    identity.ensure_signing_key().await.unwrap();
    (store, identity)
}

/// A provisioned user: `(tenant, user, email, password)`. The email carries
/// the random tenant id so the global login-username index never collides
/// across reruns against the shared database.
pub struct TestUser {
    pub tenant: TenantId,
    pub user: UserId,
    pub email: String,
    pub password: String,
}

pub async fn make_user(store: &Arc<Store>, identity: &Identity, tag: &str) -> TestUser {
    let tenant = store.create_tenant(&format!("id-{tag}")).await.unwrap();
    // The random tenant id keeps both the user's email and the global login
    // username unique across reruns and across the other test users.
    let email = format!("{tag}-{tenant}@ex.test");
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&email)
        .await
        .unwrap();
    let password = format!("correct-horse-{tag}-battery");
    identity
        .set_password(&tenant, &user, &email, &password)
        .await
        .unwrap();
    TestUser {
        tenant,
        user,
        email,
        password,
    }
}
