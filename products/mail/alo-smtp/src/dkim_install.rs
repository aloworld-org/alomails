//! `alo-smtp --install-dkim-key` — the operator door for a sending identity's
//! DKIM key.
//!
//! ADR 0014 stores one DKIM key per domain per algorithm and resolves it by the
//! `From` domain at signing time, and `/admin/domains/dkim/rotate` mints Ed25519
//! keys for hosted domains. Neither can install **RSA**: the ADR deliberately
//! refuses to generate RSA in-process (the pure-Rust `rsa` crate is forbidden,
//! ADR 0008) and says an operator supplies it out of band. This is that band.
//!
//! It exists because a campaign identity (ADR 0044 §1) dual-signs — RSA for the
//! receivers that cannot read RFC 8463, Ed25519 for the rest — and bulk mail is
//! where an unverifiable signature is charged to delivery rather than merely
//! noticed.
//!
//! It runs where the key and the database already are: inside the service
//! container, which mounts the key directory and holds `DATABASE_URL`. **The
//! private half never leaves the host, is never logged, and never appears in an
//! error message** — the only thing printed is the public record to publish.

use std::path::{Path, PathBuf};

use alo_auth_mail::dkim::keystore::{
    self, ed25519_key_from_pkcs8, generate_ed25519_key, load_pkcs8_pem,
};
use alo_auth_mail::dkim::rsa_public;
use alo_store::{Store, TenantId};
use zeroize::Zeroizing;

/// What the operator asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRequest {
    /// The tenant the key belongs to.
    pub tenant: String,
    /// The sending domain (`d=`).
    pub domain: String,
    /// The DNS selector (`s=`). Required when importing a key, because the
    /// record was published under a name the operator chose; generated keys
    /// derive their own.
    pub selector: Option<String>,
    /// A PKCS#8 PEM private key to import. `None` generates a fresh Ed25519 key.
    pub key_path: Option<PathBuf>,
}

/// What was installed, and the record that must now exist in DNS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// The sending domain.
    pub domain: String,
    /// The selector the key is published under.
    pub selector: String,
    /// `rsa` or `ed25519`, as read from the key rather than as asserted.
    pub algorithm: String,
    /// The DNS name of the TXT record.
    pub record_name: String,
    /// The TXT record's value.
    pub record_value: String,
}

/// Why an install was refused. Every variant is safe to print: none of them can
/// carry key bytes.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    /// The request itself is unusable.
    #[error("{0}")]
    Request(String),
    /// The key file could not be read as a PKCS#8 PEM private key, or its
    /// permissions are too open.
    #[error("key file unusable: {0}")]
    Key(String),
    /// The key is neither RSA nor Ed25519.
    #[error("the key is neither an RSA nor an Ed25519 private key")]
    UnknownAlgorithm,
    /// No such tenant.
    #[error("no tenant {0}")]
    UnknownTenant(String),
    /// The domain is registered to a different tenant.
    #[error(
        "{0} belongs to another tenant; installing a key here would retire theirs \
         and leave their mail unverifiable"
    )]
    ForeignDomain(String),
    /// Key generation failed (RNG).
    #[error("could not generate a key")]
    Generate,
    /// The store refused the write.
    #[error("could not store the key: {0}")]
    Store(#[from] alo_store::StoreError),
}

/// Parses the operator's arguments, or `None` when this is not an install run.
///
/// Accepts `--flag value` and `--flag=value` alike, because an operator typing a
/// one-off command should not have to remember which this is.
///
/// # Errors
/// [`InstallError::Request`] when the command is recognised but malformed —
/// a missing value, an unknown flag, or a required field left out.
pub fn from_args<I>(args: I) -> Result<Option<InstallRequest>, InstallError>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    if !args.iter().any(|a| a == "--install-dkim-key") {
        return Ok(None);
    }
    let mut tenant = None;
    let mut domain = None;
    let mut selector = None;
    let mut key_path = None;
    let mut i = 0;
    while i < args.len() {
        let (flag, inline) = match args[i].split_once('=') {
            Some((flag, value)) => (flag.to_owned(), Some(value.to_owned())),
            None => (args[i].clone(), None),
        };
        // `--flag value` consumes the next argument; `--flag=value` does not.
        // A value that is itself a flag was never a value — the operator left
        // one out, and taking it would silently mean something else.
        let mut value = |name: &str| -> Result<String, InstallError> {
            if let Some(value) = inline.clone() {
                return Ok(value);
            }
            i += 1;
            args.get(i)
                .filter(|v| !v.starts_with("--"))
                .cloned()
                .ok_or_else(|| InstallError::Request(format!("{name} needs a value")))
        };
        match flag.as_str() {
            "--install-dkim-key" => {}
            "--tenant" => tenant = Some(value("--tenant")?),
            "--domain" => domain = Some(value("--domain")?),
            "--selector" => selector = Some(value("--selector")?),
            "--key" => key_path = Some(PathBuf::from(value("--key")?)),
            other => {
                return Err(InstallError::Request(format!("unknown flag {other}")));
            }
        }
        i += 1;
    }
    let tenant = tenant.ok_or_else(|| InstallError::Request("--tenant is required".to_owned()))?;
    let domain = domain.ok_or_else(|| InstallError::Request("--domain is required".to_owned()))?;
    if key_path.is_some() && selector.is_none() {
        // The record for an imported key is already published under a name we
        // did not choose; guessing one would install a key nothing looks up.
        return Err(InstallError::Request(
            "--selector is required with --key (the name the record is published under)".to_owned(),
        ));
    }
    Ok(Some(InstallRequest {
        tenant,
        domain,
        selector,
        key_path,
    }))
}

/// Installs the key and returns the record to publish.
///
/// # Errors
/// [`InstallError`] — see its variants. Nothing is written unless every check
/// passed, so a refusal never leaves a half-installed identity behind.
pub async fn run(store: &Store, request: &InstallRequest) -> Result<Installed, InstallError> {
    let domain = request.domain.trim().trim_matches('.').to_ascii_lowercase();
    if domain.is_empty() || !domain.contains('.') || domain.contains(char::is_whitespace) {
        return Err(InstallError::Request(format!(
            "{:?} is not a sending domain",
            request.domain
        )));
    }
    let tenant = TenantId::from(request.tenant.trim().to_owned());
    if !store.tenant_exists(&tenant).await? {
        return Err(InstallError::UnknownTenant(request.tenant.clone()));
    }
    // Tenancy: installing a key retires the domain's previous active key of the
    // same algorithm. Doing that to a neighbour would not read anything of
    // theirs — it would stop their outbound mail verifying until they noticed.
    // A domain with no registration at all is fine: a sending subdomain is an
    // egress identity, not a hosted domain.
    if let Some(record) = store.domain_record(&domain).await?
        && record.tenant_id != tenant.as_str()
    {
        return Err(InstallError::ForeignDomain(domain));
    }

    let (selector, algorithm, seed, public_raw) = match &request.key_path {
        Some(path) => {
            let selector = request.selector.clone().unwrap_or_default();
            let (algorithm, seed, public_raw) = read_key(path)?;
            (selector, algorithm, seed, public_raw)
        }
        None => {
            let key = generate_ed25519_key().ok_or(InstallError::Generate)?;
            let selector = request.selector.clone().unwrap_or(key.selector);
            (
                selector,
                "ed25519".to_owned(),
                Zeroizing::new(key.seed.to_vec()),
                key.public_raw.to_vec(),
            )
        }
    };
    let selector = validate_selector(&selector)?;

    let record_value =
        keystore::txt_record_for(&algorithm, &public_raw).ok_or(InstallError::UnknownAlgorithm)?;

    store
        .install_active_dkim_key(&tenant, &domain, &selector, &algorithm, &seed, &public_raw)
        .await?;

    Ok(Installed {
        record_name: format!("{selector}._domainkey.{domain}"),
        record_value,
        domain,
        selector,
        algorithm,
    })
}

/// What reading a key file yields: the algorithm tag as the store spells it,
/// the bytes to persist, and the public bytes to publish.
type KeyMaterial = (String, Zeroizing<Vec<u8>>, Vec<u8>);

/// Reads a PKCS#8 PEM key and works out what it actually is.
///
/// **The algorithm is read from the key, never taken from a flag.** Signing an
/// RSA key as though its bytes were an Ed25519 seed produces a signature that
/// looks fine here and fails at every receiver, which surfaces as lost delivery
/// weeks later rather than as an error now.
///
/// What is stored differs by algorithm, following the store's existing
/// convention: Ed25519 keeps its 32-byte seed (the key is rebuilt from it),
/// while RSA has no such compact form and keeps the PKCS#8 DER itself.
fn read_key(path: &Path) -> Result<KeyMaterial, InstallError> {
    let der = load_pkcs8_pem(path).map_err(InstallError::Key)?;
    if let Some(spki) = rsa_public::spki_from_pkcs8(&der) {
        return Ok(("rsa".to_owned(), der, spki));
    }
    if let Some((seed, public)) = ed25519_key_from_pkcs8(&der) {
        return Ok(("ed25519".to_owned(), seed, public));
    }
    Err(InstallError::UnknownAlgorithm)
}

/// A selector must be a DNS label: it becomes `<selector>._domainkey.<domain>`,
/// and a name that cannot be published is a key nothing will ever look up.
fn validate_selector(selector: &str) -> Result<String, InstallError> {
    let selector = selector.trim().to_ascii_lowercase();
    let usable = !selector.is_empty()
        && selector.len() <= 63
        && selector
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !selector.starts_with('-')
        && !selector.ends_with('-');
    if !usable {
        return Err(InstallError::Request(format!(
            "{selector:?} is not a DNS label; a selector is letters, digits and hyphens"
        )));
    }
    Ok(selector)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn an_ordinary_run_is_not_an_install_run() {
        assert_eq!(from_args(args(&[])).unwrap(), None);
        assert_eq!(from_args(args(&["--healthcheck"])).unwrap(), None);
    }

    #[test]
    fn both_spellings_of_a_flag_mean_the_same_thing() {
        let spaced = from_args(args(&[
            "--install-dkim-key",
            "--tenant",
            "t1",
            "--domain",
            "news.example.com",
            "--selector",
            "camp",
            "--key",
            "/dkim/campaign.key",
        ]))
        .unwrap();
        let inline = from_args(args(&[
            "--install-dkim-key",
            "--tenant=t1",
            "--domain=news.example.com",
            "--selector=camp",
            "--key=/dkim/campaign.key",
        ]))
        .unwrap();
        assert_eq!(spaced, inline);
        let request = spaced.expect("an install request");
        assert_eq!(request.tenant, "t1");
        assert_eq!(request.domain, "news.example.com");
        assert_eq!(request.selector.as_deref(), Some("camp"));
        assert_eq!(request.key_path, Some(PathBuf::from("/dkim/campaign.key")));
    }

    #[test]
    fn generating_a_key_needs_no_selector_but_importing_one_does() {
        let generated = from_args(args(&[
            "--install-dkim-key",
            "--tenant=t1",
            "--domain=news.example.com",
        ]))
        .unwrap()
        .expect("an install request");
        assert!(generated.selector.is_none());
        assert!(generated.key_path.is_none());

        let imported = from_args(args(&[
            "--install-dkim-key",
            "--tenant=t1",
            "--domain=news.example.com",
            "--key=/dkim/campaign.key",
        ]));
        assert!(
            imported.is_err(),
            "an imported key without its published selector must be refused"
        );
    }

    #[test]
    fn a_command_that_could_be_read_two_ways_is_refused() {
        for command in [
            args(&["--install-dkim-key"]),                         // nothing named
            args(&["--install-dkim-key", "--tenant=t1"]),          // no domain
            args(&["--install-dkim-key", "--domain=news.x.test"]), // no tenant
            args(&["--install-dkim-key", "--tenant=t1", "--domain"]), // no value
            args(&[
                "--install-dkim-key",
                "--tenant=t1",
                "--domain",
                "--selector=camp",
            ]), // value eaten by the next flag
            args(&["--install-dkim-key", "--tenant=t1", "--dommain=x.test"]), // typo
        ] {
            assert!(
                from_args(command.clone()).is_err(),
                "{command:?} must be refused rather than half-understood"
            );
        }
    }

    #[test]
    fn a_selector_that_could_not_be_published_is_refused() {
        assert_eq!(validate_selector(" Camp ").unwrap(), "camp");
        assert_eq!(
            validate_selector("fic0a1b2c3d4e5").unwrap(),
            "fic0a1b2c3d4e5"
        );
        for bad in [
            "",
            "  ",
            "camp._domainkey",
            "camp.news",
            "camp key",
            "-camp",
            "camp-",
            "camp!",
        ] {
            assert!(
                validate_selector(bad).is_err(),
                "selector {bad:?} must be refused"
            );
        }
        assert!(validate_selector(&"a".repeat(63)).is_ok());
        assert!(validate_selector(&"a".repeat(64)).is_err());
    }

    /// Writes a key file the way an operator is expected to leave one: readable
    /// by its owner and by nobody else.
    ///
    /// [`load_pkcs8_pem`] refuses a group- or world-readable key, which is the
    /// behaviour we want. `fs::write` creates `0644` on Unix, so a test that
    /// wrote a key and read it straight back was refused on **Linux — the
    /// platform this ships on — while passing on Windows**, which has no Unix
    /// mode for the check to read. A test that only holds on the machine it was
    /// written on is worse than no test.
    fn write_key_file(path: &Path, contents: &[u8]) {
        std::fs::write(path, contents).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }

    #[test]
    fn a_key_file_that_is_not_a_key_is_refused_by_name() {
        let dir = std::env::temp_dir().join(format!("alo-dkim-install-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("not-a-key.pem");
        // Owner-only, so the refusal asserted below is for the reason this test
        // names rather than for the file's permissions.
        write_key_file(&path, b"-----BEGIN CERTIFICATE-----\nnope\n");
        let error = read_key(&path).expect_err("a certificate is not a signing key");
        assert!(
            matches!(error, InstallError::Key(_)),
            "expected a key-file error, got {error:?}"
        );
        // Nothing secret can appear in what an operator sees.
        assert!(!error.to_string().contains("nope"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn an_ed25519_key_is_read_back_from_the_file_an_operator_supplies() {
        // The whole import path for the algorithm we can generate: PEM on disk
        // → the stored seed and the published public key. The generated key is
        // the yardstick, so a wrong-algorithm read cannot pass this.
        use alo_auth_mail::dkim::keystore::ed25519_signing_key_from_seed;
        use base64::Engine;

        let generated = generate_ed25519_key().expect("keygen");
        let der = ed25519_signing_key_from_seed(generated.seed.as_ref())
            .expect("from seed")
            .pkcs8_der;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&*der);
        let pem = format!("-----BEGIN PRIVATE KEY-----\n{b64}\n-----END PRIVATE KEY-----\n");

        let dir = std::env::temp_dir().join(format!("alo-dkim-ed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ed25519.pem");
        write_key_file(&path, pem.as_bytes());

        let (algorithm, seed, public) = read_key(&path).expect("an Ed25519 key");
        assert_eq!(algorithm, "ed25519");
        assert_eq!(seed.as_slice(), generated.seed.as_ref());
        assert_eq!(public, generated.public_raw.to_vec());
        // And the record it would publish names the algorithm it really is.
        let record = keystore::txt_record_for(&algorithm, &public).expect("a record");
        assert!(record.contains("k=ed25519"));
        std::fs::remove_file(&path).ok();
    }
}
