//! End-to-end proof of ARC sealing on Sieve redirects (RFC 8617): the
//! *real* forward path (`LocalDelivery::deliver` → Sieve `redirect` →
//! spool enqueue) is driven with a sealer installed, and the enqueued
//! forward must carry a complete `i=1; cv=none` ARC set that the real
//! chain validator accepts against the published key — plus the
//! original message byte-intact below it.
//!
//! Needs the dev Postgres (compose, or a throwaway container) at
//! `DATABASE_URL` / the 5433 default. Skips itself if no database is
//! reachable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use alo_auth_mail::arc::{self, ChainValidation};
use alo_auth_mail::dkim::Message;
use alo_auth_mail::dkim::keystore::{
    FileKeyStore, KeyAlgorithm, ed25519_signing_key_from_seed, ed25519_txt_record,
    generate_ed25519_key,
};
use alo_auth_mail::resolver::fixture::FixtureResolver;
use alo_smtp::authmail::{AuthMail, SigningConfig};
use alo_smtp::local_delivery::{DeliveryOutcome, LocalDelivery};
use alo_smtp::spool::Spool;
use alo_store::{BlobStore, Store};
use base64::Engine;

const HOSTNAME: &str = "mx.alo.test";

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
fn database_url() -> String {
    alo_test_db::url()
}

/// The message as it reaches local delivery on the MX: our stamped
/// `Authentication-Results` (the ingress verdicts) above the original.
fn stamped_message(to: &str) -> String {
    format!(
        "Authentication-Results: {HOSTNAME};\r\n\
         \tspf=pass smtp.mailfrom=friend@ext.test;\r\n\
         \tdmarc=pass header.from=ext.test\r\n\
         From: friend@ext.test\r\nTo: {to}\r\nSubject: forward me\r\n\
         Date: Thu, 30 Jul 2026 12:00:00 +0000\r\nMessage-ID: <arc-e2e@ext.test>\r\n\r\n\
         the forwarded body\r\n"
    )
}

#[tokio::test]
async fn sieve_redirect_is_arc_sealed_and_validates() {
    let Ok(store) = Store::connect(&database_url(), BlobStore::in_memory(25 * 1024 * 1024)).await
    else {
        eprintln!("SKIP: no database at {}", database_url());
        return;
    };
    let store = Arc::new(store);
    store.migrate().await.unwrap();

    // A user whose active Sieve script redirects everything off-host.
    let tenant = store.create_tenant("arc-e2e").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let email = format!("fwd-{tenant}@alo.test");
    let user = ts.create_user(&email).await.unwrap();
    let acc = store.for_account(tenant.clone(), user);
    acc.put_sieve_script("fwd", "redirect \"external@ext.test\";")
        .await
        .unwrap();
    acc.activate_sieve_script(Some("fwd")).await.unwrap();

    // The sealer: the deployment signing key (Ed25519 file key), as the
    // fallback path — the redirecting domain has no stored tenant key.
    let key = generate_ed25519_key().unwrap();
    let der = ed25519_signing_key_from_seed(key.seed.as_ref())
        .unwrap()
        .pkcs8_der;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&*der);
    let pem = format!("-----BEGIN PRIVATE KEY-----\n{b64}\n-----END PRIVATE KEY-----\n");
    let dir = tempfile::tempdir().unwrap();
    let key_path = dir.path().join("seal.pem");
    std::fs::write(&key_path, pem).unwrap();
    // The keystore refuses group/world-readable private keys on unix, and
    // `fs::write` honours the umask (0o644 by default) — without this the seal
    // key loads only on Windows, where the permission check is a no-op.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let keys = FileKeyStore::new().with_key(
        "sealer.test",
        &key.selector,
        &key_path,
        KeyAlgorithm::Ed25519Sha256,
    );
    let auth = AuthMail::disabled(HOSTNAME).with_signing(SigningConfig {
        keys: Arc::new(keys),
        domain: "sealer.test".to_owned(),
        selector: key.selector.clone(),
    });

    let spooldir = tempfile::tempdir().unwrap();
    let spool = Arc::new(Spool::new(spooldir.path()).unwrap());
    let ld = LocalDelivery::from_store(store.clone(), spool.clone(), HOSTNAME.to_owned())
        .with_arc_sealer(Arc::new(auth));

    let message = stamped_message(&email);
    assert_eq!(
        ld.deliver(
            message.as_bytes(),
            Some("friend@ext.test"),
            std::slice::from_ref(&email),
        )
        .await,
        DeliveryOutcome::Delivered
    );

    // The forward sits in the outbound spool, sealed.
    let ids = spool.list().unwrap();
    let mut sealed_body: Option<Vec<u8>> = None;
    for id in ids {
        let (env, body) = spool.read(&id).unwrap();
        if env.rcpt_to.iter().any(|r| r == "external@ext.test") {
            assert_eq!(
                env.mail_from.as_deref(),
                Some("friend@ext.test"),
                "redirect keeps the original return-path"
            );
            sealed_body = Some(body);
        }
    }
    let body = sealed_body.expect("redirect not enqueued");
    let text = String::from_utf8_lossy(&body);
    assert!(text.starts_with("ARC-Seal: i=1;"), "{text}");
    assert!(text.contains("cv=none"), "{text}");
    assert!(text.contains("ARC-Message-Signature: i=1;"), "{text}");
    assert!(
        text.contains(&format!("ARC-Authentication-Results: i=1; {HOSTNAME};")),
        "{text}"
    );
    // The original message is intact below the ARC set.
    assert!(text.ends_with(&message), "original bytes must be intact");

    // The chain validates against the published key.
    let txt = ed25519_txt_record(&key.public_raw);
    let resolver = FixtureResolver::default()
        .with_txt(&format!("{}._domainkey.sealer.test", key.selector), &[&txt]);
    assert_eq!(
        arc::verify(&resolver, &Message::parse(&body)).await,
        ChainValidation::Pass,
        "the enqueued forward must carry a valid ARC chain"
    );

    store.delete_tenant(&tenant).await.unwrap();
}

#[tokio::test]
async fn redirect_without_sealer_is_unsealed_and_still_flows() {
    // No sealer configured (dev/test): the forward must still be
    // enqueued, just without ARC headers — sealing never gates mail.
    let Ok(store) = Store::connect(&database_url(), BlobStore::in_memory(25 * 1024 * 1024)).await
    else {
        eprintln!("SKIP: no database at {}", database_url());
        return;
    };
    let store = Arc::new(store);
    store.migrate().await.unwrap();
    let tenant = store.create_tenant("arc-e2e-off").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let email = format!("fwdoff-{tenant}@alo.test");
    let user = ts.create_user(&email).await.unwrap();
    let acc = store.for_account(tenant.clone(), user);
    acc.put_sieve_script("fwd", "redirect \"external@ext.test\";")
        .await
        .unwrap();
    acc.activate_sieve_script(Some("fwd")).await.unwrap();

    let spooldir = tempfile::tempdir().unwrap();
    let spool = Arc::new(Spool::new(spooldir.path()).unwrap());
    let ld = LocalDelivery::from_store(store.clone(), spool.clone(), HOSTNAME.to_owned());

    let message = stamped_message(&email);
    assert_eq!(
        ld.deliver(
            message.as_bytes(),
            Some("friend@ext.test"),
            std::slice::from_ref(&email),
        )
        .await,
        DeliveryOutcome::Delivered
    );
    let ids = spool.list().unwrap();
    let mut found = false;
    for id in ids {
        let (env, body) = spool.read(&id).unwrap();
        if env.rcpt_to.iter().any(|r| r == "external@ext.test") {
            let text = String::from_utf8_lossy(&body);
            assert!(!text.contains("ARC-Seal"), "no sealer → no ARC set");
            assert_eq!(body, message.as_bytes(), "forwarded verbatim");
            found = true;
        }
    }
    assert!(found, "redirect not enqueued");
    store.delete_tenant(&tenant).await.unwrap();
}
