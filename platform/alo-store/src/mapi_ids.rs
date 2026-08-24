//! Stable MAPI identifiers (ADR 0051) — what a classic Outlook calls a folder
//! or a message, recorded rather than recomputed.
//!
//! A MAPI id is a replica and a 48-bit counter. alo used to derive that counter
//! by hashing the store's opaque id, which is enough while a client only reads:
//! the server holds the folder's list and matches an incoming id by scanning.
//!
//! Cached-mode synchronisation cannot use a hash. The client keeps its replica
//! for years and hands back sets of ids; the server answers with what changed
//! outside the set. That needs two guarantees a 48-bit hash does not give:
//!
//! * **Unique.** Hashes collide by the birthday bound long before the space
//!   fills. Two messages sharing an id raises no error anywhere — the client
//!   simply never sees the second one. Silent mail loss.
//! * **Permanent.** An id handed out must never name a different object later.
//!
//! So a counter is allocated once, per account, and kept. See migration
//! `0802_mapi_object_ids.sql` for why one counter space covers both kinds and
//! why the allocator is its own row.
//!
//! These hang off [`AccountStore`] rather than taking a tenant and a user,
//! because that handle already carries both privately and bakes them into every
//! statement. An id is only ever meaningful inside the mailbox that issued it,
//! so a free function taking the ids as arguments would make a cross-account
//! lookup something a caller could write by accident.

use std::collections::HashMap;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};

/// The object kinds a MAPI id can name.
///
/// Both draw from one counter space, so an id is unambiguous across them; the
/// kind records which table `store_id` points into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapiKind {
    /// A mailbox, which MAPI calls a folder.
    Folder,
    /// A message.
    Message,
}

impl MapiKind {
    /// The value stored in the `kind` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Message => "message",
        }
    }
}

/// The first counter the allocator will hand out.
///
/// Below this sits the band the special folders occupy at fixed values a client
/// expects to find, so an allocated id can never take one of theirs.
pub const FIRST_ALLOCATABLE: i64 = 1024;

/// A store GUID is sixteen bytes.
pub const REPLGUID_LEN: usize = 16;

impl AccountStore {
    /// The counter a MAPI client knows this object by, allocating one if this
    /// is the first time the object has been named.
    ///
    /// Idempotent: calling it twice for the same object returns the same
    /// counter, which is the entire point — a client holds it for years.
    ///
    /// # Errors
    ///
    /// Returns a store error if a query fails.
    pub async fn mapi_counter_for(&self, kind: MapiKind, store_id: &str) -> Result<i64> {
        if let Some(existing) = self.mapi_lookup(kind, store_id).await? {
            return Ok(existing);
        }

        // Advance the account's allocator and take the value, atomically: the
        // row lock Postgres holds for the UPDATE is what stops two concurrent
        // deliveries being handed the same number.
        let counter = sqlx::query!(
            "INSERT INTO mapi_id_counter (tenant_id, user_id, last_counter) VALUES ($1, $2, $3) \
             ON CONFLICT (tenant_id, user_id) \
             DO UPDATE SET last_counter = mapi_id_counter.last_counter + 1 \
             RETURNING last_counter",
            self.tenant.as_str(),
            self.user.as_str(),
            FIRST_ALLOCATABLE
        )
        .fetch_one(&self.pool)
        .await?
        .last_counter;

        // Two callers racing on the *same* object each allocated a counter;
        // only one row can land. The loser's number is simply never used — ids
        // are not required to be dense — and both callers end up with the
        // winner's, which is why this re-reads rather than trusting `counter`.
        sqlx::query!(
            "INSERT INTO mapi_object_id (tenant_id, user_id, kind, store_id, counter) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (tenant_id, user_id, kind, store_id) DO NOTHING",
            self.tenant.as_str(),
            self.user.as_str(),
            kind.as_str(),
            store_id,
            counter
        )
        .execute(&self.pool)
        .await?;

        Ok(self.mapi_lookup(kind, store_id).await?.unwrap_or(counter))
    }

    /// The counter already recorded for this object, if any.
    async fn mapi_lookup(&self, kind: MapiKind, store_id: &str) -> Result<Option<i64>> {
        let row = sqlx::query!(
            "SELECT counter FROM mapi_object_id \
             WHERE tenant_id = $1 AND user_id = $2 AND kind = $3 AND store_id = $4",
            self.tenant.as_str(),
            self.user.as_str(),
            kind.as_str(),
            store_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| row.counter))
    }

    /// Counters for many objects at once, allocating for any not yet named.
    ///
    /// A folder listing needs every message's id, and one query per message
    /// would make drawing a mailbox cost a round trip per row.
    ///
    /// # Errors
    ///
    /// Returns a store error if a query fails.
    pub async fn mapi_counters_for(
        &self,
        kind: MapiKind,
        store_ids: &[String],
    ) -> Result<HashMap<String, i64>> {
        let mut found: HashMap<String, i64> = HashMap::with_capacity(store_ids.len());
        if store_ids.is_empty() {
            return Ok(found);
        }

        let rows = sqlx::query!(
            "SELECT store_id, counter FROM mapi_object_id \
             WHERE tenant_id = $1 AND user_id = $2 AND kind = $3 AND store_id = ANY($4)",
            self.tenant.as_str(),
            self.user.as_str(),
            kind.as_str(),
            store_ids
        )
        .fetch_all(&self.pool)
        .await?;
        for row in rows {
            found.insert(row.store_id, row.counter);
        }

        // Whatever the batch did not cover is new, and allocation is one at a
        // time so each gets its own atomic bump. New objects are rare next to
        // reads, so the loop is not the hot path the batch above is.
        for store_id in store_ids {
            if !found.contains_key(store_id) {
                let counter = self.mapi_counter_for(kind, store_id).await?;
                found.insert(store_id.clone(), counter);
            }
        }

        Ok(found)
    }

    /// The object a counter names, or `None` if this account never issued it.
    ///
    /// # Errors
    ///
    /// Returns a store error if the query fails.
    pub async fn mapi_store_id_for(&self, kind: MapiKind, counter: i64) -> Result<Option<String>> {
        let row = sqlx::query!(
            "SELECT store_id FROM mapi_object_id \
             WHERE tenant_id = $1 AND user_id = $2 AND kind = $3 AND counter = $4",
            self.tenant.as_str(),
            self.user.as_str(),
            kind.as_str(),
            counter
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| row.store_id))
    }

    /// Resolves many counters at once — the shape a synchronising client sends.
    ///
    /// Counters this account never issued are simply absent from the result,
    /// which is the honest answer to "which of these do I know": a client that
    /// names one is told nothing about it, rather than something about somebody
    /// else's mail.
    ///
    /// # Errors
    ///
    /// Returns a store error if the query fails.
    pub async fn mapi_store_ids_for(
        &self,
        kind: MapiKind,
        counters: &[i64],
    ) -> Result<HashMap<i64, String>> {
        if counters.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query!(
            "SELECT counter, store_id FROM mapi_object_id \
             WHERE tenant_id = $1 AND user_id = $2 AND kind = $3 AND counter = ANY($4)",
            self.tenant.as_str(),
            self.user.as_str(),
            kind.as_str(),
            counters
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.counter, row.store_id))
            .collect())
    }

    /// The store GUID this mailbox presents to MAPI clients.
    ///
    /// Allocated on first use and stable thereafter. A client caches it inside
    /// every identifier it holds — `PidTagSourceKey`, `PidTagChangeKey`, the
    /// `REPLGUID` of every set it sends back — so changing it invalidates that
    /// client's entire replica and it re-downloads the mailbox.
    ///
    /// The sixteen bytes are the GUID's canonical (RFC 4122) order, taken from
    /// its text form rather than from a driver's native type. [MS-OXCDATA]
    /// §2.11.1 describes a GUID as `Data1`/`Data2`/`Data3` in little-endian
    /// order, but nothing on either side parses those fields: the value is
    /// emitted, compared and echoed as one unit, so what matters is that the
    /// same bytes come out every time — which reading a fixed textual form
    /// guarantees, and a driver's byte order would not necessarily.
    ///
    /// # Errors
    ///
    /// Returns a store error if the query fails, or if the stored value is not
    /// a GUID — which would mean the column had been written by something other
    /// than this code.
    pub async fn mapi_replica_guid(&self) -> Result<[u8; REPLGUID_LEN]> {
        // The allocator row may not exist yet — a client can log on before it
        // has been given a single id — so this creates it. `DO NOTHING` then a
        // read, rather than `DO UPDATE ... RETURNING`, because there is nothing
        // to update: the point is to observe the value, not to advance it.
        sqlx::query!(
            "INSERT INTO mapi_id_counter (tenant_id, user_id) VALUES ($1, $2) \
             ON CONFLICT (tenant_id, user_id) DO NOTHING",
            self.tenant.as_str(),
            self.user.as_str()
        )
        .execute(&self.pool)
        .await?;

        // Read as text: the alternative is sqlx's `uuid` feature and the crate
        // behind it, for one column whose bytes we never interpret.
        let row = sqlx::query!(
            "SELECT replica_guid::text AS \"replica_guid!: String\" \
             FROM mapi_id_counter WHERE tenant_id = $1 AND user_id = $2",
            self.tenant.as_str(),
            self.user.as_str()
        )
        .fetch_one(&self.pool)
        .await?;

        guid_bytes(&row.replica_guid).ok_or_else(|| {
            StoreError::Validation(format!(
                "mapi_id_counter.replica_guid is not a GUID: {} characters",
                row.replica_guid.len()
            ))
        })
    }

    /// Forgets an object's id, for when the object itself is gone.
    ///
    /// The counter is **not** returned to the allocator: a client may still be
    /// holding it, and reissuing it to a different message is precisely the
    /// permanence failure this table exists to prevent.
    ///
    /// # Errors
    ///
    /// Returns a store error if the query fails.
    pub async fn mapi_forget(&self, kind: MapiKind, store_id: &str) -> Result<()> {
        sqlx::query!(
            "DELETE FROM mapi_object_id \
             WHERE tenant_id = $1 AND user_id = $2 AND kind = $3 AND store_id = $4",
            self.tenant.as_str(),
            self.user.as_str(),
            kind.as_str(),
            store_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// The sixteen bytes of a GUID written in its canonical text form.
///
/// Accepts the 36-character hyphenated form Postgres renders, and takes the
/// hex digits in the order they are written — so the same stored value always
/// yields the same bytes, which is the only property anything here depends on.
/// Returns `None` for anything that is not that form, rather than padding or
/// truncating into a plausible-looking identifier.
fn guid_bytes(text: &str) -> Option<[u8; REPLGUID_LEN]> {
    let mut out = [0_u8; REPLGUID_LEN];
    let mut digits = text.chars().filter(|c| *c != '-');
    for byte in &mut out {
        let high = digits.next()?.to_digit(16)?;
        let low = digits.next()?.to_digit(16)?;
        // Both digits are one hex character, so the value cannot exceed 0xFF.
        *byte = u8::try_from(high * 16 + low).ok()?;
    }
    // A longer string would have filled the array and still had digits left,
    // which is a different value wearing the right prefix.
    if digits.next().is_some() {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::MapiKind;

    /// The kind strings are what the migration's CHECK constraint allows; a
    /// mismatch would only surface as a failed insert at runtime.
    #[test]
    fn kind_names_are_the_values_the_check_constraint_allows() {
        assert_eq!(MapiKind::Folder.as_str(), "folder");
        assert_eq!(MapiKind::Message.as_str(), "message");
    }
}
