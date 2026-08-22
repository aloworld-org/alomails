//! The operator key-install door (`alo-smtp --install-dkim-key`), driven against
//! a real store.
//!
//! The failure this file exists for is not a leak. Installing a key **retires
//! the domain's previous active key of that algorithm** — so an operator who
//! could install one for a domain belonging to another tenant would not read
//! anything of theirs, they would stop that tenant's outbound mail verifying
//! anywhere, silently, until somebody noticed the deliverability. That is the
//! wrong-tenant test here, and it asserts the neighbour's key is still the one
//! that signs afterwards rather than merely that a refusal happened.
//!
//! Needs the dev Postgres at `DATABASE_URL` / the 5433 default. Skips itself if
//! no database is reachable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use alo_auth_mail::dkim::keystore::{ed25519_signing_key_from_seed, generate_ed25519_key};
use alo_smtp::dkim_install::{self, InstallError, InstallRequest};
use alo_store::{BlobStore, Store, TenantId};
use base64::Engine;

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
fn database_url() -> String {
    alo_test_db::url()
}

/// A DNS-safe suffix from a tenant id, so parallel runs never collide.
fn suffix(tenant: &TenantId) -> String {
    tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(10)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Writes an Ed25519 PKCS#8 PEM an operator might hand us, returning its path,
/// the public key it must end up publishing, and the seed it must end up
/// signing with.
fn write_key(dir: &Path, name: &str) -> (PathBuf, Vec<u8>, Vec<u8>) {
    let generated = generate_ed25519_key().unwrap();
    let der = ed25519_signing_key_from_seed(generated.seed.as_ref())
        .unwrap()
        .pkcs8_der;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&*der);
    let pem = format!("-----BEGIN PRIVATE KEY-----\n{b64}\n-----END PRIVATE KEY-----\n");
    let path = dir.join(name);
    std::fs::write(&path, pem).unwrap();
    // The loader refuses group/world-readable private keys on unix, and
    // `fs::write` honours the umask — without this the key loads only on
    // Windows, where the permission check is a no-op.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    (
        path,
        generated.public_raw.to_vec(),
        generated.seed.as_ref().to_vec(),
    )
}

async fn store() -> Option<Store> {
    match Store::connect(&database_url(), BlobStore::in_memory(4 * 1024 * 1024)).await {
        Ok(store) => {
            store.migrate().await.unwrap();
            Some(store)
        }
        Err(_) => {
            eprintln!(
                "SKIP: no database at {} — bring up dev postgres to run",
                database_url()
            );
            None
        }
    }
}

#[tokio::test]
async fn an_imported_key_becomes_the_one_that_signs_and_names_its_own_record() {
    let Some(store) = store().await else { return };
    let tenant = store.create_tenant("dkim-install").await.unwrap();
    let domain = format!("news{}.test", suffix(&tenant));
    let dir = tempfile::tempdir().unwrap();
    let (path, public, seed) = write_key(dir.path(), "campaign.pem");

    let installed = dkim_install::run(
        &store,
        &InstallRequest {
            tenant: tenant.as_str().to_owned(),
            domain: domain.to_ascii_uppercase(), // spelled as an operator might
            selector: Some("Camp".to_owned()),
            key_path: Some(path),
        },
    )
    .await
    .expect("the key installs");

    // The record names the algorithm the key actually is, read from the key
    // rather than asserted by a flag.
    assert_eq!(installed.algorithm, "ed25519");
    assert_eq!(installed.domain, domain);
    assert_eq!(installed.selector, "camp");
    assert_eq!(installed.record_name, format!("camp._domainkey.{domain}"));
    let expected_p = base64::engine::general_purpose::STANDARD.encode(&public);
    assert_eq!(
        installed.record_value,
        format!("v=DKIM1; k=ed25519; p={expected_p}"),
        "the published record must carry the key that was installed"
    );

    // And the signer resolves it: this is the whole point of the command.
    let material = store
        .active_dkim_material(&domain)
        .await
        .unwrap()
        .expect("the signer finds an active key for the sending domain");
    assert_eq!(material.selector, "camp");
    assert_eq!(material.algorithm, "ed25519");
    // What signs must be the private half of what was published: the file's own
    // key, not a fresh one generated alongside it.
    assert_eq!(
        material.seed, seed,
        "the key that signs must be the one in the operator's file"
    );
    assert!(
        ed25519_signing_key_from_seed(&material.seed).is_some(),
        "and it must rebuild into a usable signing key"
    );
}

#[tokio::test]
async fn a_key_cannot_be_installed_for_a_domain_another_tenant_owns() {
    let Some(store) = store().await else { return };
    let neighbour = store.create_tenant("dkim-install-neighbour").await.unwrap();
    let us = store.create_tenant("dkim-install-us").await.unwrap();
    let domain = format!("theirs{}.test", suffix(&neighbour));

    // The neighbour registers the domain and has a working signing key.
    store.create_domain(&neighbour, &domain).await.unwrap();
    let theirs = generate_ed25519_key().unwrap();
    store
        .install_active_dkim_key(
            &neighbour,
            &domain,
            &theirs.selector,
            "ed25519",
            theirs.seed.as_ref(),
            &theirs.public_raw,
        )
        .await
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let (path, _, _) = write_key(dir.path(), "ours.pem");
    let error = dkim_install::run(
        &store,
        &InstallRequest {
            tenant: us.as_str().to_owned(),
            domain: domain.clone(),
            selector: Some("camp".to_owned()),
            key_path: Some(path),
        },
    )
    .await
    .expect_err("installing over a neighbour's domain must be refused");
    assert!(
        matches!(error, InstallError::ForeignDomain(ref d) if d == &domain),
        "expected a foreign-domain refusal, got {error:?}"
    );

    // The refusal is only worth anything if their mail still signs with their
    // key — a half-applied install would have retired it.
    let material = store
        .active_dkim_material(&domain)
        .await
        .unwrap()
        .expect("the neighbour still has an active key");
    assert_eq!(material.selector, theirs.selector);
    assert_eq!(material.seed, theirs.seed.to_vec());
    // And nothing of ours was written for it.
    let ours = store
        .for_tenant(us.clone())
        .list_dkim_keys(&domain)
        .await
        .unwrap();
    assert!(
        ours.is_empty(),
        "no key of ours may exist for another tenant's domain"
    );
}

#[tokio::test]
async fn an_unknown_tenant_and_an_unusable_domain_are_refused_before_anything_is_written() {
    let Some(store) = store().await else { return };
    let tenant = store.create_tenant("dkim-install-guards").await.unwrap();
    let domain = format!("news{}.test", suffix(&tenant));
    let dir = tempfile::tempdir().unwrap();
    let (path, _, _) = write_key(dir.path(), "campaign.pem");

    let request = |tenant: &str, domain: &str, selector: &str| InstallRequest {
        tenant: tenant.to_owned(),
        domain: domain.to_owned(),
        selector: Some(selector.to_owned()),
        key_path: Some(path.clone()),
    };

    // A tenant that does not exist.
    assert!(matches!(
        dkim_install::run(&store, &request("no-such-tenant", &domain, "camp")).await,
        Err(InstallError::UnknownTenant(_))
    ));
    // Things that are not sending domains.
    for bad in ["", "  ", "localhost", "news example.test", "."] {
        assert!(
            matches!(
                dkim_install::run(&store, &request(tenant.as_str(), bad, "camp")).await,
                Err(InstallError::Request(_))
            ),
            "domain {bad:?} must be refused"
        );
    }
    // A selector that could never be published.
    for bad in ["", "camp._domainkey", "camp key"] {
        assert!(
            matches!(
                dkim_install::run(&store, &request(tenant.as_str(), &domain, bad)).await,
                Err(InstallError::Request(_))
            ),
            "selector {bad:?} must be refused"
        );
    }
    // After every refusal the domain still has no key: nothing half-wrote.
    assert!(store.active_dkim_material(&domain).await.unwrap().is_none());
}

#[tokio::test]
async fn a_generated_key_needs_no_file_and_publishes_what_it_generated() {
    let Some(store) = store().await else { return };
    let tenant = store.create_tenant("dkim-install-generate").await.unwrap();
    let domain = format!("gen{}.test", suffix(&tenant));

    let installed = dkim_install::run(
        &store,
        &InstallRequest {
            tenant: tenant.as_str().to_owned(),
            domain: domain.clone(),
            selector: None,
            key_path: None,
        },
    )
    .await
    .expect("a key is generated");

    assert_eq!(installed.algorithm, "ed25519");
    assert!(
        installed.selector.starts_with("fic"),
        "a generated selector is derived from the key: {}",
        installed.selector
    );
    assert!(installed.record_value.starts_with("v=DKIM1; k=ed25519; p="));

    // Installing a second key for the same algorithm rotates rather than
    // duplicates: one active key per algorithm is the store's invariant, and an
    // operator running the command twice must not end up signing with a key
    // whose record was never published.
    let again = dkim_install::run(
        &store,
        &InstallRequest {
            tenant: tenant.as_str().to_owned(),
            domain: domain.clone(),
            selector: None,
            key_path: None,
        },
    )
    .await
    .expect("a second key is generated");
    assert_ne!(again.selector, installed.selector);
    let active = store.active_dkim_materials(&domain).await.unwrap();
    assert_eq!(active.len(), 1, "one active key per algorithm");
    assert_eq!(active[0].selector, again.selector);
}
