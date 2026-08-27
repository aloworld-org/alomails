//! Proof of the configured dual-signing path (RFC 8463, M4.5): the *real*
//! signer (`AuthMail::sign_outbound`, the function the submission listener
//! calls) is driven with a deployment key pair — RSA + Ed25519 for one
//! domain, as the server wires it from `ALO_SMTP_DKIM_SELECTOR2`/`_KEY2` —
//! and the message must carry **two** signatures, RSA first, each of which
//! validates under the real DKIM verifier against fixture DNS keys.
//!
//! No database needed: this is the file-key fallback path, the one the
//! production sending domain uses. The path ships dark — until the second
//! pair is configured, `sign_outbound` emits exactly the one signature it
//! always has (asserted here too).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use alo_auth_mail::dkim::keystore::{
    FileKeyStore, KeyAlgorithm, ed25519_signing_key_from_seed, ed25519_txt_record,
    generate_ed25519_key, rsa_txt_record,
};
use alo_auth_mail::dkim::{self, DkimResult, Message, rsa_public};
use alo_auth_mail::resolver::fixture::FixtureResolver;
use alo_smtp::authmail::{AuthMail, SigningConfig};
use base64::Engine;

// The committed RSA-2048 test fixture — RSA is never generated in-process
// (ADR 0008), so suites share `fixture_keys` instead of each carrying a copy.
use alo_auth_mail::dkim::keystore::fixture_keys::RSA_PKCS8_B64;

const DOMAIN: &str = "dual.test";
const RSA_SELECTOR: &str = "r1";
const ED_SELECTOR: &str = "e1";

/// A minimal, well-formed RFC 5322 message (CRLF).
const RAW_MSG: &[u8] = b"From: sender@dual.test\r\nTo: rcpt@example.test\r\n\
Subject: dual signing\r\nDate: Thu, 27 Aug 2026 12:00:00 +0000\r\n\
Message-ID: <dual@alo.test>\r\n\r\nBoth families must be able to verify this.\r\n";

/// Writes a PKCS#8 PEM, owner-readable only (the keystore refuses wider
/// permissions on Unix; the mode is a no-op on Windows).
fn write_key_pem(path: &std::path::Path, der: &[u8]) {
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let pem = format!("-----BEGIN PRIVATE KEY-----\n{b64}\n-----END PRIVATE KEY-----\n");
    std::fs::write(path, pem).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

/// The RSA fixture on disk plus the TXT record that publishes its public half.
fn rsa_key(dir: &std::path::Path) -> (std::path::PathBuf, String) {
    let der = base64::engine::general_purpose::STANDARD
        .decode(RSA_PKCS8_B64)
        .unwrap();
    let path = dir.join("rsa.pem");
    write_key_pem(&path, &der);
    let spki = rsa_public::spki_from_pkcs8(&der).expect("an RSA key");
    (path, rsa_txt_record(&spki))
}

/// A fresh Ed25519 key on disk plus its TXT record.
fn ed25519_key(dir: &std::path::Path) -> (std::path::PathBuf, String) {
    let generated = generate_ed25519_key().expect("keygen");
    let der = ed25519_signing_key_from_seed(generated.seed.as_ref())
        .expect("from seed")
        .pkcs8_der;
    let path = dir.join("ed25519.pem");
    write_key_pem(&path, &der);
    (path, ed25519_txt_record(&generated.public_raw))
}

#[tokio::test]
async fn dual_signing_produces_two_signatures_and_both_validate() {
    let dir = tempfile::tempdir().unwrap();
    let (rsa_path, rsa_txt) = rsa_key(dir.path());
    let (ed_path, ed_txt) = ed25519_key(dir.path());

    // Exactly what the server wires from a dual config: both keys in the
    // store, the RSA selector first.
    let keys = FileKeyStore::new()
        .with_key(DOMAIN, RSA_SELECTOR, &rsa_path, KeyAlgorithm::RsaSha256)
        .with_key(DOMAIN, ED_SELECTOR, &ed_path, KeyAlgorithm::Ed25519Sha256);
    let auth = AuthMail::disabled("mail.alo.test").with_signing(SigningConfig {
        keys: Arc::new(keys),
        domain: DOMAIN.to_owned(),
        selector: RSA_SELECTOR.to_owned(),
        second_selector: Some(ED_SELECTOR.to_owned()),
    });

    let sig = auth
        .sign_outbound(RAW_MSG)
        .await
        .expect("a dual-keyed domain must be signed");
    eprintln!(
        "\n===== CAPTURED DKIM-Signature block — dual-signing =====\n{}\n",
        sig.trim_end()
    );

    // Two signatures, RSA first (the family every verifier reads leads).
    let lines: Vec<&str> = sig
        .lines()
        .filter(|l| l.starts_with("DKIM-Signature:"))
        .collect();
    assert_eq!(lines.len(), 2, "exactly two signatures: {sig}");
    assert!(
        lines[0].contains("a=rsa-sha256") && lines[0].contains(&format!("s={RSA_SELECTOR}")),
        "the RSA signature must come first: {}",
        lines[0]
    );
    assert!(
        lines[1].contains("a=ed25519-sha256") && lines[1].contains(&format!("s={ED_SELECTOR}")),
        "the Ed25519 signature must come second: {}",
        lines[1]
    );

    // Both must validate under the real verifier, resolving each published
    // key from fixture DNS — this is the item's gate: two VALID signatures,
    // not two plausible-looking headers.
    let signed = [sig.as_bytes(), RAW_MSG].concat();
    let resolver = FixtureResolver::default()
        .with_txt(&format!("{RSA_SELECTOR}._domainkey.{DOMAIN}"), &[&rsa_txt])
        .with_txt(&format!("{ED_SELECTOR}._domainkey.{DOMAIN}"), &[&ed_txt]);
    let verdicts = dkim::verify(&resolver, &Message::parse(&signed)).await;
    assert_eq!(verdicts.len(), 2, "{verdicts:?}");
    for verdict in &verdicts {
        assert_eq!(
            verdict.result,
            DkimResult::Pass,
            "every signature must validate: {verdicts:?}"
        );
        assert_eq!(verdict.domain, DOMAIN);
    }
    let selectors: Vec<&str> = verdicts.iter().map(|v| v.selector.as_str()).collect();
    assert_eq!(selectors, [RSA_SELECTOR, ED_SELECTOR]);
    eprintln!("DKIM verification of both captured signatures: PASS ({verdicts:?})\n");
}

#[tokio::test]
async fn without_a_second_key_exactly_one_signature_is_emitted() {
    // The dark half of "ships dark": a single-key config signs once,
    // byte-shape-identical to before M4.5 existed.
    let dir = tempfile::tempdir().unwrap();
    let (rsa_path, rsa_txt) = rsa_key(dir.path());
    let keys =
        FileKeyStore::new().with_key(DOMAIN, RSA_SELECTOR, &rsa_path, KeyAlgorithm::RsaSha256);
    let auth = AuthMail::disabled("mail.alo.test").with_signing(SigningConfig {
        keys: Arc::new(keys),
        domain: DOMAIN.to_owned(),
        selector: RSA_SELECTOR.to_owned(),
        second_selector: None,
    });
    let sig = auth.sign_outbound(RAW_MSG).await.expect("signed");
    assert_eq!(
        sig.lines()
            .filter(|l| l.starts_with("DKIM-Signature:"))
            .count(),
        1,
        "a single-key deployment signs exactly once: {sig}"
    );
    let signed = [sig.as_bytes(), RAW_MSG].concat();
    let resolver = FixtureResolver::default()
        .with_txt(&format!("{RSA_SELECTOR}._domainkey.{DOMAIN}"), &[&rsa_txt]);
    let verdicts = dkim::verify(&resolver, &Message::parse(&signed)).await;
    assert_eq!(verdicts.len(), 1);
    assert_eq!(verdicts[0].result, DkimResult::Pass, "{verdicts:?}");
}

#[tokio::test]
async fn one_unusable_key_does_not_cost_the_other_its_signature() {
    // A second key whose file has gone missing at signing time degrades to
    // one signature — never to an unsigned message, never to no message.
    let dir = tempfile::tempdir().unwrap();
    let (rsa_path, rsa_txt) = rsa_key(dir.path());
    let keys = FileKeyStore::new()
        .with_key(DOMAIN, RSA_SELECTOR, &rsa_path, KeyAlgorithm::RsaSha256)
        .with_key(
            DOMAIN,
            ED_SELECTOR,
            dir.path().join("vanished.pem"),
            KeyAlgorithm::Ed25519Sha256,
        );
    let auth = AuthMail::disabled("mail.alo.test").with_signing(SigningConfig {
        keys: Arc::new(keys),
        domain: DOMAIN.to_owned(),
        selector: RSA_SELECTOR.to_owned(),
        second_selector: Some(ED_SELECTOR.to_owned()),
    });
    let sig = auth
        .sign_outbound(RAW_MSG)
        .await
        .expect("the usable key must still sign");
    let lines: Vec<&str> = sig
        .lines()
        .filter(|l| l.starts_with("DKIM-Signature:"))
        .collect();
    assert_eq!(lines.len(), 1, "only the usable key signs: {sig}");
    assert!(lines[0].contains("a=rsa-sha256"), "{sig}");
    let signed = [sig.as_bytes(), RAW_MSG].concat();
    let resolver = FixtureResolver::default()
        .with_txt(&format!("{RSA_SELECTOR}._domainkey.{DOMAIN}"), &[&rsa_txt]);
    let verdicts = dkim::verify(&resolver, &Message::parse(&signed)).await;
    assert_eq!(verdicts.len(), 1);
    assert_eq!(verdicts[0].result, DkimResult::Pass, "{verdicts:?}");
}
