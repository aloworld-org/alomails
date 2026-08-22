//! `identityctl` — the non-public provisioning CLI for `alo-identity`.
//! Creates a tenant's first admin, registers first-party OAuth clients, and
//! manages ID-token signing keys. This is an operator tool, never an HTTP
//! surface: the admin password arrives from `ALO_ADMIN_PASSWORD` or
//! stdin, never a command-line argument (which would leak to the process
//! table).
//!
//! Env: `DATABASE_URL`, `ALO_IDENTITY_ISSUER`, and optionally
//! `ALO_IDENTITY_BLOB_DIR` (default `./blobs`; unused by identity ops but
//! required to construct the store handle).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use alo_identity::{Identity, IdentityConfig};
use alo_store::{BlobStore, Store};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        return usage();
    };

    let identity = match connect().await {
        Ok(id) => id,
        Err(msg) => {
            eprintln!("identityctl: {msg}");
            return ExitCode::FAILURE;
        }
    };

    let result = match command.as_str() {
        "bootstrap-admin" => bootstrap_admin(&identity, &args[1..]).await,
        "bootstrap-operator" => bootstrap_operator(&identity, &args[1..]).await,
        "reset-password" => reset_password(&identity, &args[1..]).await,
        "register-client" => register_client(&identity, &args[1..]).await,
        "ensure-signing-key" => ensure_signing_key(&identity).await,
        "rotate-signing-key" => rotate_signing_key(&identity).await,
        "help" | "--help" | "-h" => return usage(),
        other => Err(format!("unknown command: {other}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("identityctl: {msg}");
            ExitCode::FAILURE
        }
    }
}

async fn connect() -> Result<Identity, String> {
    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is required".to_owned())?;
    let issuer = std::env::var("ALO_IDENTITY_ISSUER")
        .map_err(|_| "ALO_IDENTITY_ISSUER is required".to_owned())?;
    // Prefer the shared ALO_BLOB_DIR the services use, so identityctl
    // runs against the same blob backend inside a deployment container.
    let blob_dir: PathBuf = std::env::var("ALO_BLOB_DIR")
        .or_else(|_| std::env::var("ALO_IDENTITY_BLOB_DIR"))
        .unwrap_or_else(|_| "./blobs".to_owned())
        .into();
    let blobs = BlobStore::local(&blob_dir, 1024 * 1024)
        .map_err(|_| "could not open blob directory".to_owned())?;
    let store = Store::connect(&database_url, blobs)
        .await
        .map_err(|_| "could not connect to the database".to_owned())?;
    store
        .migrate()
        .await
        .map_err(|_| "could not run migrations".to_owned())?;
    Identity::new(Arc::new(store), IdentityConfig::new(issuer))
        .map_err(|_| "could not initialise identity".to_owned())
}

async fn bootstrap_admin(identity: &Identity, args: &[String]) -> Result<(), String> {
    let [tenant, email] = args else {
        return Err("usage: bootstrap-admin <tenant-name> <email>".to_owned());
    };
    let password = read_password()?;
    if password.len() < 12 {
        return Err("admin password must be at least 12 characters".to_owned());
    }
    identity.ensure_signing_key().await.map_err(fail)?;
    let account = identity
        .bootstrap_admin(tenant, email, &password)
        .await
        .map_err(fail)?;
    println!(
        "created tenant {} with admin {} ({})",
        account.tenant.as_str(),
        email,
        account.user.as_str()
    );
    Ok(())
}

async fn bootstrap_operator(identity: &Identity, args: &[String]) -> Result<(), String> {
    let [email] = args else {
        return Err("usage: bootstrap-operator <email>".to_owned());
    };
    let password = read_password()?;
    if password.len() < 12 {
        return Err("operator password must be at least 12 characters".to_owned());
    }
    identity.ensure_signing_key().await.map_err(fail)?;
    let account = identity
        .bootstrap_operator(email, &password)
        .await
        .map_err(fail)?;
    println!(
        "created platform operator {} ({}) in the system tenant {}",
        email,
        account.user.as_str(),
        account.tenant.as_str()
    );
    Ok(())
}

async fn reset_password(identity: &Identity, args: &[String]) -> Result<(), String> {
    let [email] = args else {
        return Err("usage: reset-password <email>".to_owned());
    };
    let password = read_password()?;
    if password.len() < 12 {
        return Err("password must be at least 12 characters".to_owned());
    }
    let credential = identity
        .store()
        .credentials_by_username(email)
        .await
        .map_err(|error| format!("could not find account: {error}"))?
        .ok_or_else(|| format!("no account exists for {email}"))?;
    identity
        .set_password(&credential.tenant, &credential.user, email, &password)
        .await
        .map_err(fail)?;
    println!("reset password for {email}");
    Ok(())
}

async fn register_client(identity: &Identity, args: &[String]) -> Result<(), String> {
    let [client_id, name, redirect_uris @ ..] = args else {
        return Err("usage: register-client <client-id> <name> <redirect-uri>...".to_owned());
    };
    if redirect_uris.is_empty() {
        return Err("at least one redirect-uri is required".to_owned());
    }
    identity
        .register_public_client(client_id, name, redirect_uris)
        .await
        .map_err(fail)?;
    println!(
        "registered public client {client_id} with {} redirect URI(s)",
        redirect_uris.len()
    );
    Ok(())
}

async fn ensure_signing_key(identity: &Identity) -> Result<(), String> {
    identity.ensure_signing_key().await.map_err(fail)?;
    println!("signing key present");
    Ok(())
}

async fn rotate_signing_key(identity: &Identity) -> Result<(), String> {
    let kid = identity.rotate_signing_key().await.map_err(fail)?;
    println!("rotated in new signing key {kid}");
    Ok(())
}

/// Reads the admin password from `ALO_ADMIN_PASSWORD`, else prompts on
/// stdin. Never taken from argv.
fn read_password() -> Result<String, String> {
    if let Ok(p) = std::env::var("ALO_ADMIN_PASSWORD") {
        return Ok(p);
    }
    print!("admin password (input is visible): ");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|_| "could not read password".to_owned())?;
    Ok(line.trim_end_matches(['\n', '\r']).to_owned())
}

fn fail(err: alo_identity::IdentityError) -> String {
    // The Display impls never carry a secret or internal detail.
    format!("operation failed: {err}")
}

fn usage() -> ExitCode {
    eprintln!(
        "identityctl — alo identity provisioning\n\
         \n\
         commands:\n\
         \x20 bootstrap-admin <tenant-name> <email>       create a tenant + first admin\n\
         \x20 bootstrap-operator <email>                  create a platform operator (control plane)\n\
         \x20 reset-password <email>                      replace an existing account password\n\
         \x20 register-client <client-id> <name> <uri>... register a public OAuth client\n\
         \x20 ensure-signing-key                          create an ID-token key if none\n\
         \x20 rotate-signing-key                          add a new signing key\n\
         \n\
         password: set ALO_ADMIN_PASSWORD or type it when prompted (never argv)."
    );
    ExitCode::FAILURE
}
