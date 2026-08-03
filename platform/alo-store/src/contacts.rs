//! Recent correspondents, for compose recipient autocomplete. The address
//! fields on `messages` (`from_addr`, `to_addrs`, `cc_addrs`, `bcc_addrs`) hold
//! raw RFC 5322 header strings; this returns the most recent of them for the
//! account and leaves parsing + ranking to the caller (alo-jmap already owns
//! address-header parsing). Tenant/user-scoped like every other read.

use crate::account::AccountStore;
use crate::changes::{self, Change, TYPE_CONTACT};
use crate::error::{Result, StoreError};
use crate::id::ContactId;
use crate::model::{Contact, ContactField};

/// The raw address headers of one message, newest-first, for contact mining.
#[derive(Debug, Clone)]
pub struct AddressHeaders {
    pub from: String,
    pub to: String,
    pub cc: String,
    pub bcc: String,
}

impl AccountStore {
    /// The address headers of this account's most recent `limit` messages
    /// (newest first). The caller extracts individual addresses and ranks them;
    /// recency order is preserved so a caller can break ties by "seen most
    /// recently". Scoped to this (tenant, user).
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn recent_address_headers(&self, limit: i64) -> Result<Vec<AddressHeaders>> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT from_addr, to_addrs, cc_addrs, bcc_addrs FROM messages \
             WHERE tenant_id = $1 AND user_id = $2 \
             ORDER BY received_at DESC LIMIT $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(from, to, cc, bcc)| AddressHeaders { from, to, cc, bcc })
            .collect())
    }

    /// This account's saved contacts, ordered by display name. Runtime
    /// query (the `contacts` table is newer than some builds' offline cache).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn contacts(&self) -> Result<Vec<Contact>> {
        let rows = sqlx::query_as::<_, ContactRow>(
            "SELECT id, display_name, first_name, last_name, emails, phones, \
                    organization, job_title, notes \
             FROM contacts WHERE tenant_id = $1 AND user_id = $2 \
             ORDER BY display_name, id",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(ContactRow::into_contact).collect())
    }

    /// One saved contact by id, or `None` when it is not this account's.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn contact(&self, id: &ContactId) -> Result<Option<Contact>> {
        let row = sqlx::query_as::<_, ContactRow>(
            "SELECT id, display_name, first_name, last_name, emails, phones, \
                    organization, job_title, notes \
             FROM contacts WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(ContactRow::into_contact))
    }

    /// Creates a contact and returns its id. The account's modseq advances
    /// so JMAP/CardDAV clients see the change. The caller validates the
    /// fields (non-empty display name, plausible addresses); the store
    /// persists what it is given.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn create_contact(&self, contact: &Contact) -> Result<ContactId> {
        let id = ContactId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO contacts \
             (tenant_id, user_id, id, display_name, first_name, last_name, \
              emails, phones, organization, job_title, notes) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(&contact.display_name)
        .bind(&contact.first_name)
        .bind(&contact.last_name)
        .bind(sqlx::types::Json(&contact.emails))
        .bind(sqlx::types::Json(&contact.phones))
        .bind(&contact.organization)
        .bind(&contact.job_title)
        .bind(&contact.notes)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        changes::bump_and_record(
            &mut tx,
            self.tenant.as_str(),
            self.user.as_str(),
            &[Change::created(TYPE_CONTACT, id.as_str())],
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Replaces a contact's fields. Advances the account modseq.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the contact is not this account's;
    /// [`StoreError::Db`] on failure.
    pub async fn update_contact(&self, id: &ContactId, contact: &Contact) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let done = sqlx::query(
            "UPDATE contacts SET display_name = $4, first_name = $5, last_name = $6, \
                    emails = $7, phones = $8, organization = $9, job_title = $10, \
                    notes = $11, updated_at = now() \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(&contact.display_name)
        .bind(&contact.first_name)
        .bind(&contact.last_name)
        .bind(sqlx::types::Json(&contact.emails))
        .bind(sqlx::types::Json(&contact.phones))
        .bind(&contact.organization)
        .bind(&contact.job_title)
        .bind(&contact.notes)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        changes::bump_and_record(
            &mut tx,
            self.tenant.as_str(),
            self.user.as_str(),
            &[Change::updated(TYPE_CONTACT, id.as_str())],
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Creates or replaces a contact at a **caller-chosen** id (CardDAV
    /// PUT: the client owns the resource href). Returns whether it was a
    /// create (`true`) or a replace. Advances the account modseq.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn put_contact(&self, id: &ContactId, contact: &Contact) -> Result<bool> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let existed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM contacts WHERE tenant_id = $1 AND user_id = $2 AND id = $3)",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let done = sqlx::query(
            "INSERT INTO contacts \
             (tenant_id, user_id, id, display_name, first_name, last_name, \
              emails, phones, organization, job_title, notes) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
             ON CONFLICT (id) DO UPDATE SET \
              display_name = EXCLUDED.display_name, first_name = EXCLUDED.first_name, \
              last_name = EXCLUDED.last_name, emails = EXCLUDED.emails, \
              phones = EXCLUDED.phones, organization = EXCLUDED.organization, \
              job_title = EXCLUDED.job_title, notes = EXCLUDED.notes, updated_at = now() \
             WHERE contacts.tenant_id = $1 AND contacts.user_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(&contact.display_name)
        .bind(&contact.first_name)
        .bind(&contact.last_name)
        .bind(sqlx::types::Json(&contact.emails))
        .bind(sqlx::types::Json(&contact.phones))
        .bind(&contact.organization)
        .bind(&contact.job_title)
        .bind(&contact.notes)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        // The `id` column is globally unique. If the row belongs to another
        // account, the guarded upsert touches nothing — surface that as a
        // conflict rather than a false success (a client-chosen href that
        // collides across tenants; astronomically rare with UUID hrefs).
        if done.rows_affected() == 0 {
            return Err(StoreError::Conflict(
                "contact id already exists in another account".to_owned(),
            ));
        }
        let change = if existed {
            Change::updated(TYPE_CONTACT, id.as_str())
        } else {
            Change::created(TYPE_CONTACT, id.as_str())
        };
        changes::bump_and_record(&mut tx, self.tenant.as_str(), self.user.as_str(), &[change])
            .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(!existed)
    }

    /// Deletes a contact. Advances the account modseq.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the contact is not this account's;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_contact(&self, id: &ContactId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let done =
            sqlx::query("DELETE FROM contacts WHERE tenant_id = $1 AND user_id = $2 AND id = $3")
                .bind(self.tenant.as_str())
                .bind(self.user.as_str())
                .bind(id.as_str())
                .execute(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        changes::bump_and_record(
            &mut tx,
            self.tenant.as_str(),
            self.user.as_str(),
            &[Change::destroyed(TYPE_CONTACT, id.as_str())],
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }
}

/// A raw `contacts` row, with the JSONB multi-value columns decoded.
#[derive(sqlx::FromRow)]
struct ContactRow {
    id: String,
    display_name: String,
    first_name: Option<String>,
    last_name: Option<String>,
    emails: sqlx::types::Json<Vec<ContactField>>,
    phones: sqlx::types::Json<Vec<ContactField>>,
    organization: Option<String>,
    job_title: Option<String>,
    notes: Option<String>,
}

impl ContactRow {
    fn into_contact(self) -> Contact {
        Contact {
            id: ContactId::new(self.id),
            display_name: self.display_name,
            first_name: self.first_name,
            last_name: self.last_name,
            emails: self.emails.0,
            phones: self.phones.0,
            organization: self.organization,
            job_title: self.job_title,
            notes: self.notes,
        }
    }
}
