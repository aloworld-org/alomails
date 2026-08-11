//! Credential setup for a one-time alo Sites collaborator invitation.
//!
//! Hashing stays in the identity authority; spending the setup token and
//! installing the credential stay one store transaction. A crash can therefore
//! leave neither a reusable password-reset link nor an unusable invited user.

use alo_store::SiteEditorInviteTarget;

use crate::secret;
use crate::{Identity, IdentityError, Result};

impl Identity {
    /// Accepts a live Sites invitation and sets its first password atomically.
    /// `None` deliberately covers unknown, expired and already-used tokens.
    ///
    /// # Errors
    /// [`IdentityError::Crypto`] when password hashing fails, or
    /// [`IdentityError::Store`] when persistence fails.
    pub async fn accept_site_editor_invite(
        &self,
        token: &str,
        password: &str,
    ) -> Result<Option<SiteEditorInviteTarget>> {
        let hash = {
            let _permit = self.argon2_slots.acquire().await;
            self.passwords
                .hash(password)
                .map_err(|_| IdentityError::Crypto)?
        };
        let token_hash = secret::hash_at_rest(token);
        self.store
            .accept_site_editor_invite(&token_hash, &hash)
            .await
            .map_err(IdentityError::Store)
    }
}
