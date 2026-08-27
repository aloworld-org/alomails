//! Web Push subscription persistence (mail M5.3). Storage only: the VAPID
//! keys, the RFC 8291 encryption and the actual sending live in the JMAP
//! service — this module holds the per-device handles a browser's push
//! service issued and nothing that could decrypt or read anything.
//!
//! Two doors, mirroring `app_passwords.rs`:
//! - **Tenant-scoped ownership on [`TenantStore`]** — subscribe, list and
//!   remove, reached only through the tenant door so a caller cannot touch
//!   another tenant's devices.
//! - **A system-handle delete on [`Store`]** — the push dispatcher drops a
//!   row the push service reported gone (HTTP 404/410), keyed by the
//!   unguessable id it just read through the tenant door.

use time::OffsetDateTime;

use crate::error::{Result, StoreError};
use crate::id::{PushSubscriptionId, UserId};
use crate::store::{Store, TenantStore};

/// The most push subscriptions one user may hold at once. A subscription is
/// a browser installation; ten live devices is beyond any real desk, and the
/// cap bounds how many external POSTs one state change can fan out to.
pub const PUSH_SUBSCRIPTIONS_MAX: i64 = 10;

/// The longest accepted endpoint URL. Push services mint long capability
/// URLs, but a URL is still a URL — kilobytes of it is not one.
pub const PUSH_ENDPOINT_MAX_CHARS: usize = 2000;

/// The longest accepted key material field (base64url text from the
/// browser's `PushSubscription.getKey`): a P-256 point is ~87 characters,
/// an auth secret ~22 — ten times that is already nonsense.
pub const PUSH_KEY_MAX_CHARS: usize = 512;

/// One subscription as its owner's settings list shows it: the record,
/// never the key material (the browser owns the keys; our copy only
/// encrypts toward it).
#[derive(Debug)]
pub struct PushSubscriptionRow {
    /// The record's id (the unsubscribe handle).
    pub id: PushSubscriptionId,
    /// The push-service endpoint URL — names the device to its owner.
    pub endpoint: String,
    /// When the device subscribed.
    pub created_at: OffsetDateTime,
}

/// Everything the dispatcher needs to deliver one encrypted push to one
/// device (RFC 8291): where to POST and the client's key material.
#[derive(Debug, Clone)]
pub struct PushDelivery {
    /// The record's id (so a dead endpoint can be dropped by handle).
    pub id: PushSubscriptionId,
    /// The push-service endpoint URL to POST to.
    pub endpoint: String,
    /// The client's P-256 ECDH public key, base64url as the browser gave it.
    pub p256dh: String,
    /// The client's 16-byte auth secret, base64url as the browser gave it.
    pub auth: String,
}

impl TenantStore {
    /// Records (or refreshes) a device's push subscription for a user in
    /// this tenant. One row per `(user, endpoint)`: re-subscribing the same
    /// device replaces its key material and keeps its id, so a browser that
    /// rotated keys does not leave a dead twin behind.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the user is not in this tenant;
    /// [`StoreError::Validation`] if the endpoint or keys are empty or
    /// overlong; [`StoreError::Conflict`] if the user already holds
    /// [`PUSH_SUBSCRIPTIONS_MAX`] subscriptions (and this endpoint is not
    /// one of them).
    pub async fn create_push_subscription(
        &self,
        user: &UserId,
        endpoint: &str,
        p256dh: &str,
        auth: &str,
    ) -> Result<PushSubscriptionId> {
        let endpoint = endpoint.trim();
        if endpoint.is_empty() {
            return Err(StoreError::Validation(
                "a push subscription needs an endpoint".into(),
            ));
        }
        if endpoint.chars().count() > PUSH_ENDPOINT_MAX_CHARS {
            return Err(StoreError::Validation(format!(
                "a push endpoint is at most {PUSH_ENDPOINT_MAX_CHARS} characters"
            )));
        }
        for (field, value) in [("p256dh", p256dh), ("auth", auth)] {
            if value.trim().is_empty() {
                return Err(StoreError::Validation(format!(
                    "a push subscription needs its {field} key"
                )));
            }
            if value.chars().count() > PUSH_KEY_MAX_CHARS {
                return Err(StoreError::Validation(format!(
                    "a push {field} key is at most {PUSH_KEY_MAX_CHARS} characters"
                )));
            }
        }
        self.assert_user(user).await?;
        let held: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM push_subscriptions \
             WHERE tenant_id = $1 AND user_id = $2 AND endpoint <> $3",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .bind(endpoint)
        .fetch_one(self.pool())
        .await?;
        if held >= PUSH_SUBSCRIPTIONS_MAX {
            return Err(StoreError::Conflict(format!(
                "at most {PUSH_SUBSCRIPTIONS_MAX} push subscriptions per user — remove one first"
            )));
        }
        let id = PushSubscriptionId::generate();
        let stored: String = sqlx::query_scalar(
            "INSERT INTO push_subscriptions (id, tenant_id, user_id, endpoint, p256dh, auth) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (tenant_id, user_id, endpoint) \
             DO UPDATE SET p256dh = EXCLUDED.p256dh, auth = EXCLUDED.auth \
             RETURNING id",
        )
        .bind(id.as_str())
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .bind(endpoint)
        .bind(p256dh)
        .bind(auth)
        .fetch_one(self.pool())
        .await?;
        Ok(PushSubscriptionId::new(stored))
    }

    /// A user's push subscriptions, oldest first — the settings list
    /// (endpoint + created; the key material never leaves the store this
    /// way).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the user is not in this tenant.
    pub async fn list_push_subscriptions(&self, user: &UserId) -> Result<Vec<PushSubscriptionRow>> {
        self.assert_user(user).await?;
        let rows = sqlx::query_as::<_, (String, String, OffsetDateTime)>(
            "SELECT id, endpoint, created_at FROM push_subscriptions \
             WHERE tenant_id = $1 AND user_id = $2 ORDER BY created_at",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, endpoint, created_at)| PushSubscriptionRow {
                id: PushSubscriptionId::new(id),
                endpoint,
                created_at,
            })
            .collect())
    }

    /// Everything needed to deliver to a user's devices right now — the
    /// dispatcher's read. Same tenant door as the rest: the hub message that
    /// triggers a send names `(tenant, user)`, and this read cannot widen it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the user is not in this tenant.
    pub async fn push_deliveries(&self, user: &UserId) -> Result<Vec<PushDelivery>> {
        self.assert_user(user).await?;
        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT id, endpoint, p256dh, auth FROM push_subscriptions \
             WHERE tenant_id = $1 AND user_id = $2 ORDER BY created_at",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, endpoint, p256dh, auth)| PushDelivery {
                id: PushSubscriptionId::new(id),
                endpoint,
                p256dh,
                auth,
            })
            .collect())
    }

    /// Removes one subscription: pushes to that device stop with the row.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if no such record belongs to this
    /// `(tenant, user)` — a foreign id gets the same clean denial as an
    /// absent one.
    pub async fn delete_push_subscription(
        &self,
        user: &UserId,
        id: &PushSubscriptionId,
    ) -> Result<()> {
        let done = sqlx::query(
            "DELETE FROM push_subscriptions WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .bind(id.as_str())
        .execute(self.pool())
        .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

impl Store {
    /// Drops a subscription whose endpoint the push service reported gone
    /// (HTTP 404/410 on delivery). Keyed by the unguessable id the
    /// dispatcher just read through the tenant door; silent if the owner
    /// removed it in the meantime — either way the row is gone.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn drop_dead_push_subscription(&self, id: &PushSubscriptionId) -> Result<()> {
        sqlx::query("DELETE FROM push_subscriptions WHERE id = $1")
            .bind(id.as_str())
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
