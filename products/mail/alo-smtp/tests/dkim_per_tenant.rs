//! End-to-end proof of per-tenant DKIM signing (ADR 0014): the *real* signer
//! (`AuthMail::sign_outbound`, the function the submission listener calls) is
//! driven with a real per-tenant key installed in the store **and** a competing
//! file key for a different domain. It proves the message is signed with the
//! per-tenant DB key (not the file key) and that the signature validates under
//! the real DKIM verifier, then that a domain without a stored key still falls
//! back to the file key (no regression).
//!
//! Needs the dev Postgres (compose, or a throwaway container) at
//! `DATABASE_URL` / the 5433 default. Skips itself if no database is reachable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use alo_auth_mail::dkim::keystore::{
    FileKeyStore, KeyAlgorithm, ed25519_signing_key_from_seed, ed25519_txt_record,
    generate_ed25519_key,
};
use alo_auth_mail::dkim::{self, DkimResult, Message};
use alo_auth_mail::resolver::fixture::FixtureResolver;
use alo_smtp::authmail::{AuthMail, SigningConfig};
use alo_store::{BlobStore, Store};
use base64::Engine;

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
fn database_url() -> String {
    alo_test_db::url()
}

/// A minimal, well-formed RFC 5322 message (CRLF) with a single-domain `From`.
fn raw_message(from: &str, subject: &str) -> String {
    format!(
        "From: {from}\r\nTo: recipient@example.test\r\nSubject: {subject}\r\n\
         Date: Wed, 30 Jul 2026 12:00:00 +0000\r\nMessage-ID: <e2e@alo.test>\r\n\r\n\
         This message proves which DKIM key signed it.\r\n"
    )
}

/// Writes a PKCS#8 PEM for `seed` (Ed25519) to a temp file for the FileKeyStore.
fn write_file_key(dir: &std::path::Path, seed: &[u8]) -> std::path::PathBuf {
    let der = ed25519_signing_key_from_seed(seed).unwrap().pkcs8_der;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&*der);
    let pem = format!("-----BEGIN PRIVATE KEY-----\n{b64}\n-----END PRIVATE KEY-----\n");
    let path = dir.join("filekey.pem");
    std::fs::write(&path, pem).unwrap();
    // The keystore refuses group/world-readable private keys on unix, and
    // `fs::write` honours the umask (0o644 by default) — without this the key
    // loads only on Windows, where the permission check is a no-op.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    path
}

#[tokio::test]
async fn per_tenant_key_signs_and_validates_not_the_file_key() {
    // Skip cleanly when no database is available (keeps CI/non-DB runs green).
    let Ok(store) = Store::connect(&database_url(), BlobStore::in_memory(25 * 1024 * 1024)).await
    else {
        eprintln!(
            "SKIP: no database at {} — bring up dev postgres to run",
            database_url()
        );
        return;
    };
    let store = Arc::new(store);
    store.migrate().await.unwrap();

    // A tenant owns `db_domain`; install its per-tenant DKIM key via the real
    // store path (fresh tenant name keeps runs from colliding).
    let tenant = store.create_tenant("dkim-e2e").await.unwrap();
    // A clean DNS label (real domains pass `is_plausible_domain`, so no `_`).
    let suffix: String = tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(10)
        .collect::<String>()
        .to_ascii_lowercase();
    let db_domain = format!("dbkey{suffix}.test");
    let key = generate_ed25519_key().unwrap();
    store
        .install_active_dkim_key(
            &tenant,
            &db_domain,
            &key.selector,
            "ed25519",
            key.seed.as_ref(),
            &key.public_raw,
        )
        .await
        .unwrap();

    // A competing FILE key for a DIFFERENT domain — so "signed with the DB key"
    // is proven against a real alternative, not the absence of one.
    let dir = tempfile::tempdir().unwrap();
    let file_gen = generate_ed25519_key().unwrap();
    let file_path = write_file_key(dir.path(), file_gen.seed.as_ref());
    let file_domain = "filekey.test";
    let file_selector = "filesel";
    let file_keys = FileKeyStore::new().with_key(
        file_domain,
        file_selector,
        &file_path,
        KeyAlgorithm::Ed25519Sha256,
    );

    let auth = AuthMail::disabled("mail.alo.test")
        .with_signing(SigningConfig {
            keys: Arc::new(file_keys),
            domain: file_domain.to_owned(),
            selector: file_selector.to_owned(),
            second_selector: None,
        })
        .with_dkim_store(Arc::clone(&store));

    // 1) Sign FROM the DB-keyed domain — must use the per-tenant key.
    let msg = raw_message(&format!("alice@{db_domain}"), "From the DB-keyed domain");
    let sig = auth
        .sign_outbound(msg.as_bytes())
        .await
        .expect("the DB-keyed domain must be signed");
    eprintln!(
        "\n===== CAPTURED DKIM-Signature — per-tenant DB key =====\n{}\n",
        sig.trim_end()
    );

    assert!(
        sig.contains(&format!("d={db_domain}")),
        "d= must be the DB domain: {sig}"
    );
    assert!(
        sig.contains(&format!("s={}", key.selector)),
        "s= must be the DB selector: {sig}"
    );
    assert!(sig.contains("a=ed25519-sha256"), "must be Ed25519: {sig}");
    assert!(
        !sig.contains(file_domain),
        "must NOT sign as the file-key domain: {sig}"
    );

    // Cryptographically verify the captured signature with the REAL DKIM
    // verifier, resolving the published key from the fixture DNS.
    let signed = format!("{sig}{msg}"); // DKIM-Signature prepended to the message
    let txt = ed25519_txt_record(&key.public_raw);
    let resolver = FixtureResolver::default().with_txt(
        &format!("{}._domainkey.{}", key.selector, db_domain),
        &[&txt],
    );
    let verdicts = dkim::verify(&resolver, &Message::parse(signed.as_bytes())).await;
    assert!(
        verdicts
            .iter()
            .any(|v| matches!(v.result, DkimResult::Pass)),
        "the captured signature must validate (Pass): {verdicts:?}"
    );
    eprintln!("DKIM verification of the captured signature: PASS ({verdicts:?})\n");

    // 2) Sign FROM the file-key domain (no stored key) — falls back to the file
    // key, unchanged behaviour (the single-tenant path).
    let msg2 = raw_message(&format!("carol@{file_domain}"), "From the file-key domain");
    let sig2 = auth
        .sign_outbound(msg2.as_bytes())
        .await
        .expect("the file-key domain must be signed");
    eprintln!(
        "===== CAPTURED DKIM-Signature — file-key fallback =====\n{}\n",
        sig2.trim_end()
    );
    assert!(
        sig2.contains(&format!("d={file_domain}")),
        "fallback must sign as the file domain"
    );
    assert!(
        sig2.contains(&format!("s={file_selector}")),
        "fallback must use the file selector"
    );

    store.delete_tenant(&tenant).await.unwrap();
}
