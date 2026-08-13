//! alo Sites catalogs — the tenant's own list of what a site offers.
//!
//! A catalog is a container (a name and one currency) holding categories and
//! [items](crate::site_catalog_items). It is deliberately *not* a
//! [collection](crate::site_collections): a collection is a live binding to a
//! table in alo Base that is re-read on every publish, while a catalog IS the
//! record — edited here, and optionally seeded once from Base through the
//! [import seam](crate::site_catalog_import).
//!
//! Money is integer minor units of `currency` (cents for EUR, whole yen for
//! JPY); no float ever touches this path. Rendering formats those units with
//! the currency's own exponent.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{BlobId, SiteCatalogCategoryId, SiteCatalogId, SiteId};

/// Maximum length of a catalog, category, or item display name.
pub const SITE_CATALOG_NAME_MAX_CHARS: usize = 120;
/// Maximum length of a public handle (category and item slugs). Bounded by the
/// section schema's id-token cap, because a catalog section names a category by
/// its handle: a handle a section could not reference would be a trap.
pub const SITE_CATALOG_SLUG_MAX_CHARS: usize = 64;
/// Maximum length of an item description.
pub const SITE_CATALOG_DESCRIPTION_MAX_CHARS: usize = 2_000;
/// Maximum length of the short note beside a price ("per night", "from").
pub const SITE_CATALOG_PRICE_NOTE_MAX_CHARS: usize = 60;
/// Maximum length of what an item's photograph shows, in words. The same cap a
/// section image's alt text carries ([`crate::site_model::MAX_SHORT_TEXT_CHARS`]),
/// because it is the same sentence in a different place.
pub const SITE_CATALOG_IMAGE_ALT_MAX_CHARS: usize = crate::site_model::MAX_SHORT_TEXT_CHARS;
/// Maximum number of categories one catalog may hold.
pub const SITE_CATALOG_MAX_CATEGORIES: usize = 100;
/// Maximum number of items one catalog may hold — the same order of magnitude
/// as a published collection, and the ceiling a publish freezes.
pub const SITE_CATALOG_MAX_ITEMS: usize = 500;
/// Largest price a catalog item may carry, in minor units (10 million major
/// units). A typo of ten extra digits is a validation error, not a price.
pub const SITE_CATALOG_MAX_PRICE_CENTS: i64 = 1_000_000_000;

/// Whether an item is on offer, temporarily unavailable, or withheld from the
/// public site entirely.
///
/// `Hidden` is the only value publishing removes: a hidden item is absent from
/// every snapshot, so it can never be read off a published page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteCatalogAvailability {
    /// Shown and orderable.
    Available,
    /// Shown, marked as unavailable.
    SoldOut,
    /// Not published at all.
    Hidden,
}

impl SiteCatalogAvailability {
    /// The exact string stored in the `availability` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SiteCatalogAvailability::Available => "available",
            SiteCatalogAvailability::SoldOut => "sold_out",
            SiteCatalogAvailability::Hidden => "hidden",
        }
    }

    /// Parses a stored value; an unknown string is a stored-data conflict
    /// rather than a silent default.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when the stored value is not one of the three.
    pub fn parse(stored: &str) -> Result<Self> {
        match stored {
            "available" => Ok(SiteCatalogAvailability::Available),
            "sold_out" => Ok(SiteCatalogAvailability::SoldOut),
            "hidden" => Ok(SiteCatalogAvailability::Hidden),
            other => Err(StoreError::Conflict(format!(
                "catalog item has an unknown availability {other}"
            ))),
        }
    }
}

/// One tenant/site-owned catalog.
#[derive(Debug, Clone)]
pub struct SiteCatalog {
    pub id: SiteCatalogId,
    pub name: String,
    /// ISO 4217 alphabetic code, uppercase.
    pub currency: String,
    /// Whether the published pages of this catalog carry an order form
    /// ([`crate::site_orders`]). Frozen into each publish, so switching it
    /// takes effect at the next publish — exactly like a price change.
    pub orders_enabled: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Complete input for a catalog.
pub struct SiteCatalogInput<'a> {
    pub name: &'a str,
    pub currency: &'a str,
    /// Offer an order form on the published pages that show this catalog.
    pub orders_enabled: bool,
}

/// One grouping inside a catalog.
#[derive(Debug, Clone)]
pub struct SiteCatalogCategory {
    pub id: SiteCatalogCategoryId,
    pub name: String,
    pub slug: String,
    pub position: i32,
}

/// Complete input for a category.
pub struct SiteCatalogCategoryInput<'a> {
    pub name: &'a str,
    pub slug: &'a str,
    pub position: i32,
}

/// One item of a catalog, as the editor sees it.
#[derive(Debug, Clone)]
pub struct SiteCatalogItem {
    pub id: crate::id::SiteCatalogItemId,
    pub category_id: Option<SiteCatalogCategoryId>,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    /// Price in minor units of the catalog's currency; `None` means "no price
    /// shown" (an enquiry-only service), which is not the same as zero.
    pub price_cents: Option<i64>,
    pub price_note: Option<String>,
    pub image: Option<BlobId>,
    /// What the image shows, in words. `None` means the owner has not written
    /// one and the published card falls back to the item name.
    pub image_alt: Option<String>,
    pub availability: SiteCatalogAvailability,
    pub position: i32,
    /// The Base record this row was imported from, when it was imported.
    pub source_key: Option<String>,
}

impl AccountStore {
    /// Creates a catalog on one of the tenant's sites. A missing or foreign
    /// site is indistinguishable from a site that does not exist.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a bad name or currency;
    /// [`StoreError::NotFound`] when the site is not the tenant's.
    pub async fn create_site_catalog(
        &self,
        site: &SiteId,
        input: &SiteCatalogInput<'_>,
    ) -> Result<SiteCatalogId> {
        let name = validate_catalog_name(input.name, "catalog name")?;
        let currency = validate_currency(input.currency)?;
        let id = SiteCatalogId::generate();
        let done = sqlx::query(
            "INSERT INTO site_catalogs (tenant_id, site_id, id, name, currency, orders_enabled) \
             SELECT $1, $2, $3, $4, $5, $6 FROM sites WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(&name)
        .bind(&currency)
        .bind(input.orders_enabled)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(id)
    }

    /// Lists a site's catalogs in stable creation order.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn site_catalogs(&self, site: &SiteId) -> Result<Vec<SiteCatalog>> {
        let rows = sqlx::query_as::<_, SiteCatalogRow>(
            "SELECT id, name, currency, orders_enabled, created_at, updated_at \
             FROM site_catalogs \
             WHERE tenant_id = $1 AND site_id = $2 ORDER BY created_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(SiteCatalogRow::into_catalog).collect())
    }

    /// Returns one tenant/site-scoped catalog.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn site_catalog(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
    ) -> Result<Option<SiteCatalog>> {
        let row = sqlx::query_as::<_, SiteCatalogRow>(
            "SELECT id, name, currency, orders_enabled, created_at, updated_at \
             FROM site_catalogs \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(catalog.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(SiteCatalogRow::into_catalog))
    }

    /// Renames a catalog or changes its currency. Prices are stored in minor
    /// units and are *not* converted: changing currency reinterprets them, so
    /// the editor asks before offering it.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a bad name or currency;
    /// [`StoreError::NotFound`] when the catalog is not the tenant's.
    pub async fn update_site_catalog(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
        input: &SiteCatalogInput<'_>,
    ) -> Result<()> {
        let name = validate_catalog_name(input.name, "catalog name")?;
        let currency = validate_currency(input.currency)?;
        let done = sqlx::query(
            "UPDATE site_catalogs \
                SET name = $4, currency = $5, orders_enabled = $6, updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(catalog.as_str())
        .bind(&name)
        .bind(&currency)
        .bind(input.orders_enabled)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes a catalog with its categories and items. Already-published
    /// snapshots keep serving: history does not depend on the editable row.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the catalog is not the tenant's.
    pub async fn delete_site_catalog(&self, site: &SiteId, catalog: &SiteCatalogId) -> Result<()> {
        let done = sqlx::query(
            "DELETE FROM site_catalogs WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(catalog.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Adds a category to a catalog.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a bad name/slug or a full catalog;
    /// [`StoreError::Conflict`] when the slug is already used in this catalog;
    /// [`StoreError::NotFound`] when the catalog is not the tenant's.
    pub async fn create_site_catalog_category(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
        input: &SiteCatalogCategoryInput<'_>,
    ) -> Result<SiteCatalogCategoryId> {
        self.require_site_catalog(site, catalog).await?;
        let name = validate_catalog_name(input.name, "category name")?;
        let slug = validate_catalog_slug(input.slug, "category slug")?;
        let existing = self.count_catalog_categories(catalog).await?;
        if existing >= SITE_CATALOG_MAX_CATEGORIES {
            return Err(StoreError::Validation(format!(
                "a catalog may hold at most {SITE_CATALOG_MAX_CATEGORIES} categories"
            )));
        }
        let id = SiteCatalogCategoryId::generate();
        sqlx::query(
            "INSERT INTO site_catalog_categories \
                (tenant_id, catalog_id, id, name, slug, position) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .bind(id.as_str())
        .bind(&name)
        .bind(&slug)
        .bind(input.position)
        .execute(&self.pool)
        .await
        .map_err(catalog_slug_conflict)?;
        Ok(id)
    }

    /// Lists a catalog's categories in display order.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the catalog is not the tenant's.
    pub async fn site_catalog_categories(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
    ) -> Result<Vec<SiteCatalogCategory>> {
        self.require_site_catalog(site, catalog).await?;
        let rows = sqlx::query_as::<_, SiteCatalogCategoryRow>(
            "SELECT id, name, slug, position FROM site_catalog_categories \
             WHERE tenant_id = $1 AND catalog_id = $2 ORDER BY position, created_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(SiteCatalogCategoryRow::into_category)
            .collect())
    }

    /// Replaces a category's complete editable shape.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a bad name/slug; [`StoreError::Conflict`]
    /// on a duplicate slug; [`StoreError::NotFound`] when the category is not
    /// the tenant's.
    pub async fn update_site_catalog_category(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
        category: &SiteCatalogCategoryId,
        input: &SiteCatalogCategoryInput<'_>,
    ) -> Result<()> {
        self.require_site_catalog(site, catalog).await?;
        let name = validate_catalog_name(input.name, "category name")?;
        let slug = validate_catalog_slug(input.slug, "category slug")?;
        let done = sqlx::query(
            "UPDATE site_catalog_categories SET name = $4, slug = $5, position = $6, \
                    updated_at = now() \
             WHERE tenant_id = $1 AND catalog_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .bind(category.as_str())
        .bind(&name)
        .bind(&slug)
        .bind(input.position)
        .execute(&self.pool)
        .await
        .map_err(catalog_slug_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes a category. Its items stay, uncategorised — deleting a grouping
    /// must never delete the things grouped by it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the category is not the tenant's.
    pub async fn delete_site_catalog_category(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
        category: &SiteCatalogCategoryId,
    ) -> Result<()> {
        self.require_site_catalog(site, catalog).await?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let done = sqlx::query(
            "DELETE FROM site_catalog_categories \
             WHERE tenant_id = $1 AND catalog_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .bind(category.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "UPDATE site_catalog_items SET category_id = NULL, updated_at = now() \
             WHERE tenant_id = $1 AND catalog_id = $2 AND category_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .bind(category.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Resolves a catalog the tenant owns on this site, or [`StoreError::NotFound`].
    pub(crate) async fn require_site_catalog(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
    ) -> Result<SiteCatalog> {
        self.site_catalog(site, catalog)
            .await?
            .ok_or(StoreError::NotFound)
    }

    pub(crate) async fn count_catalog_categories(&self, catalog: &SiteCatalogId) -> Result<usize> {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM site_catalog_categories \
             WHERE tenant_id = $1 AND catalog_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    }
}

/// Turns the per-catalog slug uniqueness violation into a field-level message
/// the editor can show, leaving every other database error alone.
pub(crate) fn catalog_slug_conflict(error: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(db) = &error
        && db.constraint().is_some_and(|name| name.contains("slug"))
    {
        return StoreError::Conflict("that handle is already used in this catalog".to_owned());
    }
    StoreError::Db(error)
}

/// Validates and trims a display name.
///
/// # Errors
/// [`StoreError::Validation`] naming the violated rule.
pub fn validate_catalog_name(name: &str, role: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::Validation(format!("{role} must not be empty")));
    }
    if name.chars().count() > SITE_CATALOG_NAME_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "{role} must be at most {SITE_CATALOG_NAME_MAX_CHARS} characters"
        )));
    }
    Ok(name.to_owned())
}

/// Validates a public handle: lowercase letters, digits, and inner hyphens.
///
/// Deliberately not [`crate::site_pages::validate_page_slug`]: a catalog handle
/// is a fragment and a filter, never a path, so the reserved public *paths* do
/// not apply to it — a category may legitimately be called `blog`.
///
/// # Errors
/// [`StoreError::Validation`] naming the violated rule.
pub fn validate_catalog_slug(slug: &str, role: &str) -> Result<String> {
    let slug = slug.trim();
    if slug.is_empty() || slug.chars().count() > SITE_CATALOG_SLUG_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "{role} must be 1-{SITE_CATALOG_SLUG_MAX_CHARS} characters"
        )));
    }
    if !slug
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(StoreError::Validation(format!(
            "{role} may only contain lowercase letters, digits, and hyphens"
        )));
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err(StoreError::Validation(format!(
            "{role} may not start or end with a hyphen"
        )));
    }
    Ok(slug.to_owned())
}

/// Validates an ISO 4217 alphabetic currency code, uppercasing it.
///
/// # Errors
/// [`StoreError::Validation`] when the code is not three ASCII letters.
pub fn validate_currency(currency: &str) -> Result<String> {
    let currency = currency.trim().to_ascii_uppercase();
    if currency.len() != 3 || !currency.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(StoreError::Validation(
            "currency must be a three-letter ISO 4217 code, for example EUR".to_owned(),
        ));
    }
    Ok(currency)
}

/// Derives a public handle from a display name — the suggestion the editor and
/// the Base import both start from. Non-ASCII letters are dropped rather than
/// transliterated (a guess about a language we do not know is worse than an
/// honest fallback); an empty result is the caller's cue to ask for a handle.
#[must_use]
pub fn catalog_slug_from_name(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    for ch in name.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    slug.chars().take(SITE_CATALOG_SLUG_MAX_CHARS).collect()
}

/// How many decimal places a currency's minor unit has (ISO 4217 exponent).
///
/// The exceptions are listed because getting them wrong is a factor-of-100
/// error in a published price: yen and franc CFA have no minor unit, dinars
/// have three. Everything else — and any code we do not know — is two, which
/// is the right default and is what the editor shows.
#[must_use]
pub fn currency_exponent(currency: &str) -> u32 {
    const ZERO: [&str; 15] = [
        "BIF", "CLP", "DJF", "GNF", "ISK", "JPY", "KMF", "KRW", "PYG", "RWF", "UGX", "VND", "VUV",
        "XAF", "XOF",
    ];
    const THREE: [&str; 7] = ["BHD", "IQD", "JOD", "KWD", "LYD", "OMR", "TND"];
    let code = currency.trim().to_ascii_uppercase();
    if ZERO.contains(&code.as_str()) {
        0
    } else if THREE.contains(&code.as_str()) {
        3
    } else {
        2
    }
}

/// Parses a human-written price into minor units of `currency`.
///
/// Accepts `12`, `12.50`, `12,50`, `1.234,50` and `1,234.50`; rejects anything
/// it cannot read *unambiguously*. A single separator followed by exactly three
/// digits (`1,234`) is refused rather than guessed: it is 1234 in one country
/// and 1.234 in another, and a silent guess is a wrong price on a public page.
/// More decimals than the currency has is likewise refused rather than rounded.
///
/// # Errors
/// [`StoreError::Validation`] naming what could not be read.
pub fn parse_price_minor_units(value: &str, currency: &str) -> Result<i64> {
    let exponent = currency_exponent(currency);
    let raw: String = value
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '\u{a0}' && *ch != '\u{202f}')
        .collect();
    let raw = raw.trim_matches(['€', '$', '£']).to_owned();
    let invalid =
        || StoreError::Validation(format!("{value} is not a price this catalog can read"));
    if raw.is_empty() {
        return Err(invalid());
    }
    let dot = raw.rfind('.');
    let comma = raw.rfind(',');
    let (whole, fraction) = match dot.max(comma) {
        None => (raw.as_str(), ""),
        Some(at) => {
            let (whole, rest) = raw.split_at(at);
            let fraction = &rest[1..];
            // One separator with exactly three digits behind it: grouping or
            // decimals? For a currency with two decimals that is unknowable —
            // `1,234` is 1234 in Amsterdam and 1.234 in Boston — so it is
            // refused rather than guessed. A currency with three decimals can
            // only mean decimals, and one with none can only mean grouping.
            if dot.is_none() != comma.is_none()
                && fraction.len() == 3
                && !whole.is_empty()
                && (1..3).contains(&exponent)
            {
                return Err(StoreError::Validation(format!(
                    "{value} could mean two different prices; write it with a decimal separator, \
                     for example 1234.00"
                )));
            }
            if exponent == 0 && fraction.len() == 3 {
                (raw.as_str(), "")
            } else {
                (whole, fraction)
            }
        }
    };
    let whole: String = whole
        .chars()
        .filter(|ch| *ch != '.' && *ch != ',')
        .collect();
    if !whole.chars().all(|ch| ch.is_ascii_digit())
        || !fraction.chars().all(|ch| ch.is_ascii_digit())
        || (whole.is_empty() && fraction.is_empty())
    {
        return Err(invalid());
    }
    if fraction.len() > exponent as usize {
        return Err(StoreError::Validation(format!(
            "{value} has more decimals than {currency} has ({exponent})"
        )));
    }
    let units: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| invalid())?
    };
    let scale = 10_i64.pow(exponent);
    let minor: i64 = if fraction.is_empty() {
        0
    } else {
        let padded = format!("{fraction:0<width$}", width = exponent as usize);
        padded.parse().map_err(|_| invalid())?
    };
    units
        .checked_mul(scale)
        .and_then(|major| major.checked_add(minor))
        .filter(|total| *total <= SITE_CATALOG_MAX_PRICE_CENTS)
        .ok_or_else(|| {
            StoreError::Validation(format!("{value} is larger than a catalog price may be"))
        })
}

#[derive(sqlx::FromRow)]
struct SiteCatalogRow {
    id: String,
    name: String,
    currency: String,
    orders_enabled: bool,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl SiteCatalogRow {
    fn into_catalog(self) -> SiteCatalog {
        SiteCatalog {
            id: SiteCatalogId::new(self.id),
            name: self.name,
            currency: self.currency,
            orders_enabled: self.orders_enabled,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SiteCatalogCategoryRow {
    id: String,
    name: String,
    slug: String,
    position: i32,
}

impl SiteCatalogCategoryRow {
    fn into_category(self) -> SiteCatalogCategory {
        SiteCatalogCategory {
            id: SiteCatalogCategoryId::new(self.id),
            name: self.name,
            slug: self.slug,
            position: self.position,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn prices_parse_in_both_european_and_anglo_spellings() {
        for (written, expected) in [
            ("12", 1_200),
            ("12.50", 1_250),
            ("12,50", 1_250),
            ("12.5", 1_250),
            ("0,99", 99),
            ("1.234,50", 123_450),
            ("1,234.50", 123_450),
            ("€ 9,95", 995),
        ] {
            assert_eq!(
                parse_price_minor_units(written, "EUR").expect(written),
                expected,
                "{written}"
            );
        }
    }

    #[test]
    fn a_currency_without_minor_units_takes_whole_numbers_only() {
        assert_eq!(parse_price_minor_units("1200", "JPY").unwrap(), 1_200);
        assert_eq!(parse_price_minor_units("1,200", "JPY").unwrap(), 1_200);
        assert!(parse_price_minor_units("12.50", "JPY").is_err());
        assert_eq!(parse_price_minor_units("12.500", "KWD").unwrap(), 12_500);
    }

    #[test]
    fn an_ambiguous_grouping_is_refused_rather_than_guessed() {
        let error = parse_price_minor_units("1,234", "EUR").unwrap_err();
        assert!(
            format!("{error}").contains("two different prices"),
            "{error}"
        );
        assert!(parse_price_minor_units("12.345", "EUR").is_err());
        assert!(parse_price_minor_units("twelve", "EUR").is_err());
        assert!(parse_price_minor_units("-5", "EUR").is_err());
        assert!(parse_price_minor_units("", "EUR").is_err());
    }

    #[test]
    fn handles_are_derived_from_names_and_bounded() {
        assert_eq!(catalog_slug_from_name("Tomato & Basil"), "tomato-basil");
        assert_eq!(catalog_slug_from_name("  Double  space "), "double-space");
        assert_eq!(catalog_slug_from_name("Crème brûlée"), "cr-me-br-l-e");
        assert_eq!(catalog_slug_from_name("……"), "");
        assert!(catalog_slug_from_name(&"a".repeat(200)).len() <= SITE_CATALOG_SLUG_MAX_CHARS);
    }

    #[test]
    fn currencies_are_normalized_and_checked() {
        assert_eq!(validate_currency(" eur ").unwrap(), "EUR");
        assert!(validate_currency("EURO").is_err());
        assert!(validate_currency("E1R").is_err());
        assert_eq!(currency_exponent("jpy"), 0);
        assert_eq!(currency_exponent("SEK"), 2);
    }
}
