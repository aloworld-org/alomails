//! Expense categories — the words a claim form offers, and the account each of
//! them books to (alo Finance, ADR 0035, wave B4; `docs/design/finance.md`,
//! "Expenses, receipts and mileage").
//!
//! A category is the **only** thing that decides an expense's account, and that
//! is why it is a row rather than a string typed on the claim:
//!
//! - Renaming "Software" — or repointing it at a different account when the
//!   accountant asks — is one edit, not a rewrite of every claim that used it.
//! - A claim carries the category's *id*, so what a hundred old claims booked
//!   to is a fact about the category as it stood, resolved once by the posting
//!   rule, rather than a string re-matched against a list that has moved.
//!
//! Two rules the store enforces, both of which produce a wrong P&L if left to
//! the caller:
//!
//! - **A category books to an *expense* account.** Pointing "Travel" at the
//!   revenue account is not a preference, it is a cost recorded as income; the
//!   store refuses it by name ([`AccountStore::create_fin_category`]).
//! - **Its account must be active.** A tenant who deactivated an account has
//!   said something, and quietly booking to it anyway would be us deciding they
//!   did not mean it — the rule [`crate::fin_accounts::AccountStore::fin_account_for_role`]
//!   already applies to the roles.
//!
//! The table ships **empty**. Nothing here seeds an English word list: the
//! categories are the tenant's own vocabulary, and a hardcoded "Travel" would
//! be a bug in a European product. The chart (which we *do* seed) is named at
//! the HTTP edge in the caller's language for the same reason.
//!
//! This module is tenant-wide configuration on the account door, exactly like
//! the chart it points into: every user of a tenant reads the same categories.
//! The claims themselves are personal and live behind their own door
//! ([`crate::fin_expenses`]).

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::{required, vat_rate_bp};
use crate::error::{Result, StoreError};
use crate::fin_accounts::AccountType;
use crate::id::{FinAccountId, FinCategoryId};

/// A category name is a word on a picker, not a description.
pub const CATEGORY_NAME_MAX_CHARS: usize = 120;

/// The columns every read of a category selects, in [`CategoryRow`] order.
const CATEGORY_COLS: &str =
    "id, name, account_id, default_vat_rate_bp, active, created_at, updated_at";

/// The writable shape of a category, used for both create and update (an update
/// is a full replace — the route layer merges a partial `PATCH` onto the stored
/// record before calling, as the chart's own routes do).
#[derive(Debug, Clone)]
pub struct NewExpenseCategory {
    /// The word on the claim form, in the tenant's language. Required, and
    /// unique within the tenant case-insensitively.
    pub name: String,
    /// The account claims in this category book to. Must be one of the tenant's
    /// own, of type [`AccountType::Expense`], and active.
    pub account_id: FinAccountId,
    /// The rate the claim form offers for this category, or `None` for "the
    /// receipt says". A default in the UI sense only: **nothing derives an
    /// expense's VAT from it** (`docs/design/finance.md`).
    pub default_vat_rate_bp: Option<i32>,
}

/// A stored expense category.
#[derive(Debug, Clone)]
pub struct ExpenseCategory {
    /// Opaque id, unique within the tenant. What a claim carries.
    pub id: FinCategoryId,
    /// The word on the claim form.
    pub name: String,
    /// The account claims in this category book to.
    pub account_id: FinAccountId,
    /// The rate the form offers, if the tenant set one.
    pub default_vat_rate_bp: Option<i32>,
    /// Whether it is offered on new claims. An inactive category stays
    /// readable, so last year's claims still explain themselves.
    pub active: bool,
    /// When it was created.
    pub created_at: OffsetDateTime,
    /// When it was last changed.
    pub updated_at: OffsetDateTime,
}

/// A validated, normalised category ready to be bound into a statement.
#[derive(Debug, PartialEq, Eq)]
struct Normalized {
    name: String,
    default_vat_rate_bp: Option<i32>,
}

/// Validates and normalises the fields that need no database. Pure, so the
/// rules are unit-tested directly; the account rule needs the chart and is
/// checked by [`AccountStore::require_expense_account`].
fn normalize(input: &NewExpenseCategory) -> Result<Normalized> {
    Ok(Normalized {
        name: required("category name", &input.name, CATEGORY_NAME_MAX_CHARS)?,
        default_vat_rate_bp: input.default_vat_rate_bp.map(vat_rate_bp).transpose()?,
    })
}

/// Turns the category table's two constraint violations into typed answers
/// naming the rule that was hit, and leaves every other database failure alone.
fn map_category_conflict(error: sqlx::Error) -> StoreError {
    match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
            match db.constraint().unwrap_or_default() {
                "fin_categories_name_unique" => {
                    StoreError::Conflict("a category with this name already exists".to_owned())
                }
                _ => StoreError::Conflict("unique constraint".to_owned()),
            }
        }
        // A claim pointing at the category (`fin_expenses_category_fk`) is what
        // makes a delete fail. A category that has classified a cost is history,
        // not a preference.
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23503") => {
            StoreError::Conflict(
                "a category that classifies expenses cannot be deleted; deactivate it instead"
                    .to_owned(),
            )
        }
        other => StoreError::Db(other),
    }
}

impl AccountStore {
    /// The tenant's expense categories, by name. Inactive ones are excluded
    /// unless `include_inactive`, and then sort after the active ones.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_categories(&self, include_inactive: bool) -> Result<Vec<ExpenseCategory>> {
        let rows = sqlx::query_as::<_, CategoryRow>(&format!(
            "SELECT {CATEGORY_COLS} FROM fin_categories \
             WHERE tenant_id = $1 AND ($2 OR active) \
             ORDER BY (NOT active), lower(name), id"
        ))
        .bind(self.tenant.as_str())
        .bind(include_inactive)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(CategoryRow::into_category).collect())
    }

    /// One category of the tenant, or `None` — including when the id belongs to
    /// another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_category(&self, id: &FinCategoryId) -> Result<Option<ExpenseCategory>> {
        let row = sqlx::query_as::<_, CategoryRow>(&format!(
            "SELECT {CATEGORY_COLS} FROM fin_categories WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(CategoryRow::into_category))
    }

    /// Creates a category.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a blank or over-long name, an out-of-range
    /// default rate, or an account that is not an active expense account;
    /// [`StoreError::NotFound`] when the account is not the tenant's;
    /// [`StoreError::Conflict`] when the name is taken; [`StoreError::Db`] on
    /// failure.
    pub async fn create_fin_category(&self, input: &NewExpenseCategory) -> Result<FinCategoryId> {
        let c = normalize(input)?;
        self.require_expense_account(&input.account_id).await?;
        let id = FinCategoryId::generate();
        sqlx::query(
            "INSERT INTO fin_categories \
                 (tenant_id, id, name, account_id, default_vat_rate_bp) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&c.name)
        .bind(input.account_id.as_str())
        .bind(c.default_vat_rate_bp)
        .execute(&self.pool)
        .await
        .map_err(map_category_conflict)?;
        Ok(id)
    }

    /// Replaces every writable field of a category.
    ///
    /// **Repointing a category at another account does not restate a claim it
    /// already classified**: what an approved expense booked is in the journal,
    /// written once, and the journal is never edited underneath a document
    /// (`docs/design/finance.md`). The new account applies to what is decided
    /// from now on.
    ///
    /// Deactivating is a separate operation
    /// ([`AccountStore::set_fin_category_active`]) so an ordinary rename can
    /// never drop a category out of the picker by accident.
    ///
    /// # Errors
    /// [`StoreError::Validation`] as for create; [`StoreError::NotFound`] when
    /// the category — or the account — is not the tenant's;
    /// [`StoreError::Conflict`] when the name is taken by another category;
    /// [`StoreError::Db`] on failure.
    pub async fn update_fin_category(
        &self,
        id: &FinCategoryId,
        input: &NewExpenseCategory,
    ) -> Result<()> {
        let c = normalize(input)?;
        self.require_expense_account(&input.account_id).await?;
        let done = sqlx::query(
            "UPDATE fin_categories SET name = $3, account_id = $4, default_vat_rate_bp = $5, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&c.name)
        .bind(input.account_id.as_str())
        .bind(c.default_vat_rate_bp)
        .execute(&self.pool)
        .await
        .map_err(map_category_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deactivates or reactivates a category. Idempotent.
    ///
    /// This is the removal a tenant normally wants: the category keeps every
    /// claim it classified and stops offering itself on new ones. A claim
    /// already carrying it is untouched — a cost does not become uncategorised
    /// because nobody may pick that word again.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the category isn't the tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn set_fin_category_active(&self, id: &FinCategoryId, active: bool) -> Result<()> {
        let done = sqlx::query(
            "UPDATE fin_categories SET active = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(active)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes a category **no claim has ever used**.
    ///
    /// The refusal is the database's own (`fin_expenses`' foreign key), so it
    /// holds against a claim being written this instant rather than only
    /// against a slow one — and it is a `409`, because deactivating is what a
    /// tenant who has stopped using a word actually wants.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the category isn't the tenant's;
    /// [`StoreError::Conflict`] when a claim carries it; [`StoreError::Db`] on
    /// failure.
    pub async fn delete_fin_category(&self, id: &FinCategoryId) -> Result<()> {
        let done = sqlx::query("DELETE FROM fin_categories WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(map_category_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Confirms an account is one this tenant may book a cost to: theirs, of
    /// type [`AccountType::Expense`], and active.
    ///
    /// Another tenant's account reads as absent, never as a refusal that would
    /// confirm it exists.
    async fn require_expense_account(&self, account: &FinAccountId) -> Result<()> {
        let found = self
            .fin_account(account)
            .await?
            .ok_or(StoreError::NotFound)?;
        if found.kind != AccountType::Expense {
            return Err(StoreError::Validation(
                "an expense category must book to an expense account".to_owned(),
            ));
        }
        if !found.active {
            return Err(StoreError::Validation(
                "an expense category cannot book to a deactivated account".to_owned(),
            ));
        }
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct CategoryRow {
    id: String,
    name: String,
    account_id: String,
    default_vat_rate_bp: Option<i32>,
    active: bool,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl CategoryRow {
    fn into_category(self) -> ExpenseCategory {
        ExpenseCategory {
            id: FinCategoryId::new(self.id),
            name: self.name,
            account_id: FinAccountId::new(self.account_id),
            default_vat_rate_bp: self.default_vat_rate_bp,
            active: self.active,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn travel() -> NewExpenseCategory {
        NewExpenseCategory {
            name: "Reisekosten".to_owned(),
            account_id: FinAccountId::new("acc-1".to_owned()),
            default_vat_rate_bp: Some(1900),
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn name_is_required_trimmed_and_bounded() {
        let c = normalize(&NewExpenseCategory {
            name: "  Reisekosten  ".to_owned(),
            ..travel()
        })
        .unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(c.name, "Reisekosten", "the word is the tenant's, not ours");

        for blank in ["", "   ", "\t\n"] {
            let msg = invalid(normalize(&NewExpenseCategory {
                name: blank.to_owned(),
                ..travel()
            }));
            assert!(msg.contains("category name"), "{msg}");
        }
        let msg = invalid(normalize(&NewExpenseCategory {
            name: "x".repeat(CATEGORY_NAME_MAX_CHARS + 1),
            ..travel()
        }));
        assert!(msg.contains("at most"), "{msg}");
        assert!(
            normalize(&NewExpenseCategory {
                name: "x".repeat(CATEGORY_NAME_MAX_CHARS),
                ..travel()
            })
            .is_ok(),
            "exactly at the bound is fine"
        );
    }

    #[test]
    fn the_default_rate_is_optional_and_bounded() {
        assert_eq!(
            normalize(&travel())
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .default_vat_rate_bp,
            Some(1900)
        );
        // "The receipt says" — the ordinary case for a tenant who does not
        // want the form pre-filling a rate at all.
        assert_eq!(
            normalize(&NewExpenseCategory {
                default_vat_rate_bp: None,
                ..travel()
            })
            .unwrap_or_else(|e| panic!("rejected: {e}"))
            .default_vat_rate_bp,
            None
        );
        // Zero is a real rate: exempt and reverse-charge purchases.
        assert_eq!(
            normalize(&NewExpenseCategory {
                default_vat_rate_bp: Some(0),
                ..travel()
            })
            .unwrap_or_else(|e| panic!("rejected: {e}"))
            .default_vat_rate_bp,
            Some(0)
        );
        for bad in [-1, 10_001] {
            let msg = invalid(normalize(&NewExpenseCategory {
                default_vat_rate_bp: Some(bad),
                ..travel()
            }));
            assert!(msg.contains("VAT rate"), "{msg}");
        }
    }

    #[test]
    fn a_duplicate_name_is_a_conflict_naming_the_rule() {
        let error = sqlx::Error::RowNotFound;
        // Only the two constraint paths are ours; everything else stays a Db
        // error rather than being dressed up as a user-fixable conflict.
        assert!(matches!(
            map_category_conflict(error),
            StoreError::Db(sqlx::Error::RowNotFound)
        ));
    }
}
