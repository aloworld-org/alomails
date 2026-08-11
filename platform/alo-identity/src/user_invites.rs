//! Credential setup for a one-time workspace invitation (migration 0209).
//!
//! Hashing stays in the identity authority; spending the token, installing the
//! credential and recording the recovery address stay one store transaction.
//! A crash can therefore leave neither a reusable link nor an account that can
//! be signed into but never recovered.

use alo_store::UserInviteTarget;

use crate::secret;
use crate::{Identity, IdentityError, Result};

impl Identity {
    /// Sets the password an invited person chose, records the recovery address
    /// they named, and spends their link.
    ///
    /// `None` when the token is unknown, expired or already spent — one answer
    /// for all three, so the acceptance page is not an oracle for which
    /// invitations exist.
    ///
    /// # Errors
    /// [`IdentityError::Crypto`] when password hashing fails, or
    /// [`IdentityError::Store`] when persistence fails.
    pub async fn accept_user_invite(
        &self,
        token: &str,
        password: &str,
        recovery_email: &str,
    ) -> Result<Option<UserInviteTarget>> {
        let hash = {
            let _permit = self.argon2_slots.acquire().await;
            self.passwords
                .hash(password)
                .map_err(|_| IdentityError::Crypto)?
        };
        let token_hash = secret::hash_at_rest(token);
        self.store
            .invites()
            .accept(&token_hash, &hash, recovery_email)
            .await
            .map_err(IdentityError::Store)
    }
}
