//! Self-service **personal** account provisioning (ADR 0018).
//!
//! An individual claims an address such as `johnsmith@alomails.com` on a
//! platform-operated domain. Unlike [`crate::provision`] (operator-run tenant
//! bootstrap), this is the primitive behind a public, verification-gated
//! signup surface — so it is defensive about the address and race-safe about
//! uniqueness.
//!
//! **One tenant per person** keeps Law #1 intact: a personal user is isolated
//! exactly as a company is. **Global address uniqueness** rides on the
//! existing global `credentials_username` unique index rather than a new
//! constraint: provisioning sets `username = email`, and a lost race surfaces
//! as [`StoreError::Conflict`] at password-set time, at which point the
//! half-built tenant is deleted so no dangling user row can ever make
//! [`Store::account_by_email`](alo_store::Store::account_by_email) ambiguous.
//!
//! The caller (the signup HTTP surface) is responsible for confirming the
//! domain is one of the configured personal domains and for the verification
//! gate; this primitive only provisions.

use alo_store::{StoreError, TenantId, UserId};

use crate::{Identity, IdentityError};

/// A newly provisioned personal account.
#[derive(Debug, Clone)]
pub struct PersonalAccount {
    pub tenant: TenantId,
    pub user: UserId,
    /// The full address, lowercased (`localpart@domain`).
    pub email: String,
}

/// Why a personal signup was refused. Maps cleanly to HTTP at the edge:
/// `InvalidAddress`/`Reserved` → 400/422, `AddressTaken` → 409, `Internal` →
/// 500. The underlying store/crypto detail is logged, never surfaced.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SignupError {
    #[error("the address is not a valid personal address")]
    InvalidAddress,
    #[error("that address is reserved")]
    Reserved,
    #[error("that address is already taken")]
    AddressTaken,
    #[error("could not create the account")]
    Internal,
}

/// Minimum localpart length — short names are reserved for the operator and
/// resist enumeration/impersonation of role accounts.
const MIN_LOCALPART: usize = 3;
/// Maximum localpart length (well under the RFC 5321 limit of 64).
const MAX_LOCALPART: usize = 64;

/// Localparts an individual may never self-claim: RFC 2142 role mailboxes
/// plus common sensitive/impersonation-prone names. Compared lowercased.
const RESERVED_LOCALPARTS: &[&str] = &[
    // RFC 2142 (business + network operations)
    "postmaster",
    "abuse",
    "hostmaster",
    "webmaster",
    "usenet",
    "news",
    "www",
    "uucp",
    "ftp",
    "noc",
    "security",
    "info",
    "marketing",
    "sales",
    "support",
    // Impersonation / operational
    "admin",
    "administrator",
    "root",
    "sysadmin",
    "mailer-daemon",
    "daemon",
    "noreply",
    "no-reply",
    "donotreply",
    "do-not-reply",
    "help",
    "contact",
    "billing",
    "accounts",
    "account",
    "alo",
    "alomail",
    "alomails",
    "team",
    "staff",
    "spam",
    "null",
];

/// Validates and normalises a personal localpart. Returns the lowercased
/// localpart or a [`SignupError`]. Rules: `[a-z0-9._-]` only, length
/// `MIN_LOCALPART..=MAX_LOCALPART`, no leading/trailing `.`/`-`/`_`, no `..`,
/// and not a reserved name.
pub fn normalize_localpart(input: &str) -> Result<String, SignupError> {
    let local = input.trim().to_ascii_lowercase();
    if local.len() < MIN_LOCALPART || local.len() > MAX_LOCALPART {
        return Err(SignupError::InvalidAddress);
    }
    if !local
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_'))
    {
        return Err(SignupError::InvalidAddress);
    }
    let edge = |c: char| c == '.' || c == '-' || c == '_';
    if local.starts_with(edge) || local.ends_with(edge) || local.contains("..") {
        return Err(SignupError::InvalidAddress);
    }
    if RESERVED_LOCALPARTS.contains(&local.as_str()) {
        return Err(SignupError::Reserved);
    }
    Ok(local)
}

impl Identity {
    /// Provisions a personal account for `localpart@domain`: its own
    /// single-user tenant, a login (`username = email`), and the standard
    /// mailboxes. Idempotent-safe against races — a duplicate address is
    /// refused as [`SignupError::AddressTaken`] with no partial account left
    /// behind.
    ///
    /// The caller must have already confirmed `domain` is a configured
    /// personal domain and that the signup was verified.
    ///
    /// # Errors
    /// [`SignupError::InvalidAddress`]/[`SignupError::Reserved`] on a bad or
    /// reserved localpart; [`SignupError::AddressTaken`] if the address
    /// exists; [`SignupError::Internal`] on a store/crypto failure.
    pub async fn provision_personal(
        &self,
        domain: &str,
        localpart: &str,
        password: &str,
        recovery_email: &str,
    ) -> Result<PersonalAccount, SignupError> {
        let localpart = normalize_localpart(localpart)?;
        let domain = domain.trim().to_ascii_lowercase();
        let email = format!("{localpart}@{domain}");

        // Friendly fast path — the authoritative guard is the unique username
        // index below, which also closes the check-then-act race.
        match self.store().account_by_email(&email).await {
            Ok(Some(_)) => return Err(SignupError::AddressTaken),
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%error, "personal signup: availability check failed");
                return Err(SignupError::Internal);
            }
        }

        // The personal tenant is named after the address (one tenant, one
        // person); it carries no owned domain.
        let tenant = self.store().create_tenant(&email).await.map_err(|error| {
            tracing::warn!(%error, "personal signup: tenant create failed");
            SignupError::Internal
        })?;

        let user = match self
            .store()
            .for_tenant(tenant.clone())
            .create_user(&email)
            .await
        {
            Ok(user) => user,
            Err(error) => {
                self.cleanup(&tenant).await;
                tracing::warn!(%error, "personal signup: user create failed");
                return Err(SignupError::Internal);
            }
        };

        // Whoever signs up **is** the tenant: they just created it, they are its
        // only member, and there is nobody above them to grant this later. A
        // tenant whose creator is not its admin therefore has no admin at all,
        // and every admin surface in it is dark forever.
        //
        // That is what shipped. The operator-provisioned path does this one line
        // (`provision.rs`) and self-service signup never did, so every account
        // made this way since the feature landed administers nothing — including
        // the deployment's own owner, whose flag had to be set by hand in psql.
        //
        // It cannot promote an *invited* member: an invite adds a user to an
        // existing tenant and does not come through here. Only the creation of a
        // brand-new tenant reaches this line.
        if let Err(error) = self
            .store()
            .for_tenant(tenant.clone())
            .set_admin(&user, true)
            .await
        {
            self.cleanup(&tenant).await;
            tracing::warn!(%error, "personal signup: set_admin failed");
            return Err(SignupError::Internal);
        }

        // set_password writes the credential; the global username unique index
        // is the real uniqueness guard. A lost race → Conflict → address taken.
        if let Err(error) = self.set_password(&tenant, &user, &email, password).await {
            self.cleanup(&tenant).await;
            return match error {
                IdentityError::Store(StoreError::Conflict(_)) => Err(SignupError::AddressTaken),
                other => {
                    tracing::warn!(error = %other, "personal signup: set_password failed");
                    Err(SignupError::Internal)
                }
            };
        }

        // A ready-to-use mailbox: Inbox plus the standard folders.
        let acc = self.store().for_account(tenant.clone(), user.clone());
        let setup = async {
            acc.inbox().await?;
            for (name, role) in [
                ("Sent", "sent"),
                ("Drafts", "drafts"),
                ("Junk", "junk"),
                ("Trash", "trash"),
                ("Archive", "archive"),
            ] {
                acc.create_mailbox(None, name, Some(role)).await?;
            }
            Ok::<(), StoreError>(())
        }
        .await;
        if let Err(error) = setup {
            self.cleanup(&tenant).await;
            tracing::warn!(%error, "personal signup: mailbox setup failed");
            return Err(SignupError::Internal);
        }

        // Persist the recovery mailbox so a forgotten password can later be
        // reset by mailing a code to it. Best-effort: the account is already
        // usable, so a failure here is logged, not surfaced — reset simply
        // won't be available for this account until the value is re-captured.
        if let Err(error) = self
            .store()
            .set_account_recovery(&email, tenant.as_str(), user.as_str(), recovery_email)
            .await
        {
            tracing::warn!(%error, "personal signup: recovery-email capture failed");
        }

        Ok(PersonalAccount {
            tenant,
            user,
            email,
        })
    }

    /// Sets a new password for an existing account identified by its address
    /// (its login username). The primitive behind the verified password-reset
    /// surface; the caller owns the verification gate (a code mailed to the
    /// recovery address). Cross-tenant by address — a personal account lives in
    /// its own tenant, resolved here from the global username index.
    ///
    /// # Errors
    /// [`IdentityError::Store`] if no account has that address or on a store
    /// failure; [`IdentityError::Crypto`] on a hashing failure.
    pub async fn reset_password(&self, address: &str, password: &str) -> Result<(), IdentityError> {
        let address = address.trim().to_ascii_lowercase();
        let cred = self
            .store()
            .credentials_by_username(&address)
            .await?
            .ok_or(IdentityError::Store(StoreError::NotFound))?;
        self.set_password(&cred.tenant, &cred.user, &address, password)
            .await
    }

    /// Compensating rollback: delete a half-provisioned personal tenant so no
    /// dangling user/address survives. Best-effort — a failure here is logged,
    /// not surfaced (the signup already failed).
    async fn cleanup(&self, tenant: &TenantId) {
        if let Err(error) = self.store().delete_tenant(tenant).await {
            tracing::warn!(%error, tenant = tenant.as_str(), "personal signup: rollback failed");
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_plain_localpart() {
        assert_eq!(normalize_localpart("JohnSmith").unwrap(), "johnsmith");
        assert_eq!(normalize_localpart("  jane.doe ").unwrap(), "jane.doe");
        assert_eq!(normalize_localpart("a_b-2").unwrap(), "a_b-2");
    }

    #[test]
    fn rejects_bad_shapes() {
        for bad in [
            "ab",     // too short
            ".john",  // leading dot
            "john.",  // trailing dot
            "-john",  // leading dash
            "jo..hn", // double dot
            "jo hn",  // space
            "john@x", // '@' not allowed in a localpart
            "josé",   // non-ascii
            "a+b",    // '+' (plus-addressing) not self-claimable
        ] {
            assert_eq!(
                normalize_localpart(bad),
                Err(SignupError::InvalidAddress),
                "{bad}"
            );
        }
    }

    #[test]
    fn rejects_reserved_names_case_insensitively() {
        for name in [
            "postmaster",
            "ABUSE",
            "Admin",
            "no-reply",
            "Root",
            "alomails",
        ] {
            assert_eq!(
                normalize_localpart(name),
                Err(SignupError::Reserved),
                "{name}"
            );
        }
    }
}
