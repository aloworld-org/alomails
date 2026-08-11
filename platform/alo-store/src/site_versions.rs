//! Site version history (ADR 0036, S2.04a) — reading the immutable publish
//! record [`crate::site_publish`] writes, and putting an earlier version back
//! online.
//!
//! Publishing already freezes everything the internet sees into rows nothing
//! ever edits. This module is the other half of that promise: the tenant can
//! see every version their website has had, compare two of them, and restore
//! one — and none of those three operations changes a single frozen byte.
//!
//! **Restoring appends; it never re-points.** A restore copies the chosen
//! publish into a NEW publish (`restored_from` naming the original) and flips
//! the site's published-set pointer to the copy, in one transaction. The
//! rejected alternative — pointing `published_publish_id` back at the old
//! publish id — writes less, but two versions would then share one identity:
//! the public service's cache key and visitor `ETag` are `<publish_id>:<path>`
//! (`alo_sites::serve`), history would show "live" on a row in the middle of
//! the list, and the fact that somebody rolled back would leave no trace.
//! Forward-only history is worth one copy of rows nothing will ever edit.
//!
//! What a restore deliberately does NOT touch is the editable draft. The
//! draft is the tenant's work in progress; silently overwriting it with a
//! version from three weeks ago would destroy work no undo could bring back.
//! Restoring is a statement about what the internet sees, and the visible
//! surface (S2.04b) says so.

use std::collections::BTreeMap;

use serde_json::Value;
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{SiteCollectionId, SiteId, SitePageId, SitePublishId};
use crate::site_collections::SiteCollectionSnapshot;
use crate::site_publish::SitePageSnapshot;
use crate::sites::SiteStatus;

/// Most versions one history read may return. A busy site publishes often;
/// the list is a UI surface, not an export, and stays bounded.
pub const MAX_SITE_PUBLISH_HISTORY: i64 = 200;

/// One entry of a site's version history: an immutable publish described by
/// what it contains, without loading the content itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePublishVersion {
    pub id: SitePublishId,
    pub published_at: OffsetDateTime,
    /// The user id that made this publish (the account door's own user).
    pub published_by: String,
    /// The site's default language frozen with this version.
    pub default_locale: String,
    /// The language contract frozen with this version, in editor order.
    pub enabled_locales: Vec<String>,
    /// The publish this version was copied from, when it came from a restore.
    pub restored_from: Option<SitePublishId>,
    /// Whether this version is the one currently on the internet.
    pub is_current: bool,
    /// Distinct page identities frozen in this version.
    pub pages: usize,
    /// The languages actually frozen, sorted — a page may be missing a
    /// translation, so this can be narrower than `enabled_locales`.
    pub locales: Vec<String>,
    /// Collection snapshots frozen with this version.
    pub collections: usize,
}

/// How one page or collection differs between two versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteVersionChange {
    /// Present in the newer version only.
    Added,
    /// Present in the older version only.
    Removed,
    /// Present in both, with different content.
    Changed,
}

impl SiteVersionChange {
    /// The stable token this change is named by on the wire.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Changed => "changed",
        }
    }
}

/// A frozen page field that can differ between two versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SitePageVersionField {
    Title,
    Slug,
    Sections,
    SeoTitle,
    SeoDescription,
    NavOrder,
    Home,
}

impl SitePageVersionField {
    /// The stable token this field is named by on the wire.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Slug => "slug",
            Self::Sections => "sections",
            Self::SeoTitle => "seo_title",
            Self::SeoDescription => "seo_description",
            Self::NavOrder => "nav_order",
            Self::Home => "home",
        }
    }
}

/// One page-in-a-language that differs between two versions. `slug` and
/// `title` describe the page on the newer side when it exists there, so a
/// reader always sees the most recent name of the thing that changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePageVersionChange {
    pub page_id: SitePageId,
    pub locale: String,
    pub slug: String,
    pub title: String,
    pub change: SiteVersionChange,
    /// Which frozen fields differ — empty for an added or removed page.
    pub fields: Vec<SitePageVersionField>,
}

/// One collection that differs between two versions, described by its size
/// rather than its rows: this is a metadata comparison, not a data export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteCollectionVersionChange {
    pub collection_id: SiteCollectionId,
    pub name: String,
    pub change: SiteVersionChange,
    pub items_before: usize,
    pub items_after: usize,
}

/// What changed between two versions of one site, at the level of pages,
/// languages, collections, and the theme — never section-by-section content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePublishComparison {
    /// The older-side version as named by the caller (not necessarily older
    /// in time — the caller chooses both ends).
    pub from: SitePublishVersion,
    pub to: SitePublishVersion,
    pub theme_changed: bool,
    pub default_locale_changed: bool,
    /// Languages enabled in `to` and not in `from`.
    pub locales_added: Vec<String>,
    /// Languages enabled in `from` and not in `to`.
    pub locales_removed: Vec<String>,
    /// Pages that differ, in navigation order; identical pages are counted
    /// only ([`Self::unchanged_pages`]).
    pub pages: Vec<SitePageVersionChange>,
    pub unchanged_pages: usize,
    pub collections: Vec<SiteCollectionVersionChange>,
    pub unchanged_collections: usize,
}

impl SitePublishComparison {
    /// Whether the two versions are indistinguishable to a visitor.
    #[must_use]
    pub fn is_identical(&self) -> bool {
        !self.theme_changed
            && !self.default_locale_changed
            && self.locales_added.is_empty()
            && self.locales_removed.is_empty()
            && self.pages.is_empty()
            && self.collections.is_empty()
    }
}

impl AccountStore {
    /// The site's version history, newest first, capped at `limit` entries
    /// (clamped to [`MAX_SITE_PUBLISH_HISTORY`]). A site of another tenant —
    /// or one that does not exist — reads as an empty history, by design.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_publish_history(
        &self,
        site: &SiteId,
        limit: i64,
    ) -> Result<Vec<SitePublishVersion>> {
        let limit = limit.clamp(1, MAX_SITE_PUBLISH_HISTORY);
        let rows = sqlx::query_as::<_, SitePublishVersionRow>(&format!(
            "{VERSION_SELECT} WHERE p.tenant_id = $1 AND p.site_id = $2 \
             ORDER BY p.published_at DESC, p.id DESC LIMIT $3"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(SitePublishVersionRow::into_version)
            .collect())
    }

    /// One version of the tenant's site. A publish of another tenant, or one
    /// belonging to another site, reads as `None` — indistinguishable from an
    /// unknown id.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_publish_version(
        &self,
        site: &SiteId,
        publish: &SitePublishId,
    ) -> Result<Option<SitePublishVersion>> {
        let row = sqlx::query_as::<_, SitePublishVersionRow>(&format!(
            "{VERSION_SELECT} WHERE p.tenant_id = $1 AND p.site_id = $2 AND p.id = $3"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(publish.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(SitePublishVersionRow::into_version))
    }

    /// Compares two versions of the tenant's site: what a visitor would see
    /// differently, at metadata level. Reads only; nothing is written.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when either version is not this site's (or not
    /// this tenant's); [`StoreError::Db`] on failure.
    pub async fn compare_site_publishes(
        &self,
        site: &SiteId,
        from: &SitePublishId,
        to: &SitePublishId,
    ) -> Result<SitePublishComparison> {
        let from_version = self
            .site_publish_version(site, from)
            .await?
            .ok_or(StoreError::NotFound)?;
        let to_version = self
            .site_publish_version(site, to)
            .await?
            .ok_or(StoreError::NotFound)?;
        let themes: Vec<(String, sqlx::types::Json<Value>)> = sqlx::query_as(
            "SELECT id, theme FROM site_publishes \
             WHERE tenant_id = $1 AND site_id = $2 AND id = ANY($3)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(vec![from.as_str().to_owned(), to.as_str().to_owned()])
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let theme_of = |wanted: &SitePublishId| {
            themes
                .iter()
                .find(|(id, _)| id == wanted.as_str())
                .map(|(_, theme)| theme.0.clone())
                .unwrap_or(Value::Null)
        };
        let theme_changed = theme_of(from) != theme_of(to);
        let (pages, unchanged_pages) = compare_pages(
            &self.site_publish_snapshots(site, from).await?,
            &self.site_publish_snapshots(site, to).await?,
        );
        let (collections, unchanged_collections) = compare_collections(
            &self.site_publish_collection_snapshots(site, from).await?,
            &self.site_publish_collection_snapshots(site, to).await?,
        );
        Ok(SitePublishComparison {
            default_locale_changed: from_version.default_locale != to_version.default_locale,
            locales_added: missing_from(&to_version.enabled_locales, &from_version.enabled_locales),
            locales_removed: missing_from(
                &from_version.enabled_locales,
                &to_version.enabled_locales,
            ),
            from: from_version,
            to: to_version,
            theme_changed,
            pages,
            unchanged_pages,
            collections,
            unchanged_collections,
        })
    }

    /// Puts an earlier version of `site` back on the internet: copies the
    /// chosen publish (its theme, its language contract, every page snapshot,
    /// every collection snapshot) into a NEW publish that records where it
    /// came from, and flips the site's published-set pointer to it — one
    /// transaction, so the public service switches between complete sets.
    /// History is untouched; the editable draft is untouched.
    ///
    /// Returns the id of the new publish.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site or the chosen version is not the
    /// tenant's; [`StoreError::Conflict`] when the chosen version froze no
    /// pages (nothing to put back online); [`StoreError::Db`] on failure.
    pub async fn restore_site_publish(
        &self,
        site: &SiteId,
        publish: &SitePublishId,
    ) -> Result<SitePublishId> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Lock the site row so a restore and a publish cannot interleave on
        // the pointer flip, exactly as `publish_site` does.
        let exists: Option<(String,)> =
            sqlx::query_as("SELECT id FROM sites WHERE tenant_id = $1 AND id = $2 FOR UPDATE")
                .bind(self.tenant.as_str())
                .bind(site.as_str())
                .fetch_optional(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
        if exists.is_none() {
            return Err(StoreError::NotFound);
        }
        let source: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM site_publishes WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(publish.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if source.is_none() {
            return Err(StoreError::NotFound);
        }
        let frozen: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM site_page_snapshots WHERE tenant_id = $1 AND publish_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(publish.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if frozen.0 == 0 {
            return Err(StoreError::Conflict(
                "that version has no pages to put back online".to_owned(),
            ));
        }
        let id = SitePublishId::generate();
        // Copy inside SQL, like publishing: the restored set is byte-for-byte
        // the frozen one, never a round trip through the application.
        sqlx::query(
            "INSERT INTO site_publishes \
                (tenant_id, site_id, id, theme, default_locale, enabled_locales, \
                 published_by, restored_from) \
             SELECT tenant_id, site_id, $4, theme, default_locale, enabled_locales, $5, id \
             FROM site_publishes WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(publish.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO site_page_snapshots \
                 (tenant_id, publish_id, page_id, locale, slug, title, sections, \
                  seo_title, seo_description, nav_order, is_home) \
             SELECT tenant_id, $3, page_id, locale, slug, title, sections, \
                    seo_title, seo_description, nav_order, is_home \
             FROM site_page_snapshots WHERE tenant_id = $1 AND publish_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(publish.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO site_collection_snapshots \
                 (tenant_id, publish_id, collection_id, name, items) \
             SELECT tenant_id, $3, collection_id, name, items \
             FROM site_collection_snapshots WHERE tenant_id = $1 AND publish_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(publish.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "UPDATE sites SET published_publish_id = $3, status = $4, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(SiteStatus::Live.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }
}

/// The shape every version read shares: the publish row plus the counts that
/// describe what it froze, without loading the frozen content.
const VERSION_SELECT: &str = "SELECT p.id, p.published_at, p.published_by, p.default_locale, \
        p.enabled_locales, p.restored_from, \
        (s.published_publish_id IS NOT DISTINCT FROM p.id) AS is_current, \
        (SELECT count(DISTINCT sn.page_id) FROM site_page_snapshots sn \
          WHERE sn.tenant_id = p.tenant_id AND sn.publish_id = p.id) AS pages, \
        (SELECT coalesce(array_agg(DISTINCT sn.locale), '{}') FROM site_page_snapshots sn \
          WHERE sn.tenant_id = p.tenant_id AND sn.publish_id = p.id) AS locales, \
        (SELECT count(*) FROM site_collection_snapshots c \
          WHERE c.tenant_id = p.tenant_id AND c.publish_id = p.id) AS collections \
     FROM site_publishes p \
     JOIN sites s ON s.tenant_id = p.tenant_id AND s.id = p.site_id";

/// Everything in `values` that `other` does not have, order preserved.
fn missing_from(values: &[String], other: &[String]) -> Vec<String> {
    values
        .iter()
        .filter(|value| !other.contains(value))
        .cloned()
        .collect()
}

/// Compares two frozen page sets by page identity **and** language: the same
/// page in two languages is two things a visitor can see. Returns the
/// differences in navigation order plus the count of identical entries.
fn compare_pages(
    from: &[SitePageSnapshot],
    to: &[SitePageSnapshot],
) -> (Vec<SitePageVersionChange>, usize) {
    let key = |page: &SitePageSnapshot| (page.page_id.as_str().to_owned(), page.locale.clone());
    let before: BTreeMap<_, _> = from.iter().map(|page| (key(page), page)).collect();
    let after: BTreeMap<_, _> = to.iter().map(|page| (key(page), page)).collect();
    let mut changes = Vec::new();
    let mut unchanged = 0;
    // Ordered by the newer side's navigation order where the page still
    // exists, so the list reads like the site does.
    let mut ordered: Vec<(i32, &(String, String))> = after
        .iter()
        .map(|(key, page)| (page.nav_order, key))
        .chain(
            before
                .iter()
                .filter(|(key, _)| !after.contains_key(*key))
                .map(|(key, page)| (page.nav_order, key)),
        )
        .collect();
    ordered.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    for (_, key) in ordered {
        match (before.get(key), after.get(key)) {
            (Some(old), Some(new)) => {
                let fields = changed_fields(old, new);
                if fields.is_empty() {
                    unchanged += 1;
                } else {
                    changes.push(SitePageVersionChange {
                        page_id: new.page_id.clone(),
                        locale: new.locale.clone(),
                        slug: new.slug.clone(),
                        title: new.title.clone(),
                        change: SiteVersionChange::Changed,
                        fields,
                    });
                }
            }
            (None, Some(new)) => changes.push(added_or_removed(new, SiteVersionChange::Added)),
            (Some(old), None) => changes.push(added_or_removed(old, SiteVersionChange::Removed)),
            (None, None) => unreachable!("the key came from one of the two maps"),
        }
    }
    (changes, unchanged)
}

fn added_or_removed(page: &SitePageSnapshot, change: SiteVersionChange) -> SitePageVersionChange {
    SitePageVersionChange {
        page_id: page.page_id.clone(),
        locale: page.locale.clone(),
        slug: page.slug.clone(),
        title: page.title.clone(),
        change,
        fields: Vec::new(),
    }
}

/// Which frozen fields differ between two snapshots of the same page and
/// language, in a stable order.
fn changed_fields(old: &SitePageSnapshot, new: &SitePageSnapshot) -> Vec<SitePageVersionField> {
    let mut fields = Vec::new();
    if old.title != new.title {
        fields.push(SitePageVersionField::Title);
    }
    if old.slug != new.slug {
        fields.push(SitePageVersionField::Slug);
    }
    if old.sections != new.sections {
        fields.push(SitePageVersionField::Sections);
    }
    if old.seo_title != new.seo_title {
        fields.push(SitePageVersionField::SeoTitle);
    }
    if old.seo_description != new.seo_description {
        fields.push(SitePageVersionField::SeoDescription);
    }
    if old.nav_order != new.nav_order {
        fields.push(SitePageVersionField::NavOrder);
    }
    if old.is_home != new.is_home {
        fields.push(SitePageVersionField::Home);
    }
    fields
}

/// Compares two frozen collection sets by collection identity, describing
/// each difference by name and size rather than by row.
fn compare_collections(
    from: &[SiteCollectionSnapshot],
    to: &[SiteCollectionSnapshot],
) -> (Vec<SiteCollectionVersionChange>, usize) {
    let before: BTreeMap<_, _> = from
        .iter()
        .map(|c| (c.collection_id.as_str().to_owned(), c))
        .collect();
    let after: BTreeMap<_, _> = to
        .iter()
        .map(|c| (c.collection_id.as_str().to_owned(), c))
        .collect();
    let mut changes = Vec::new();
    let mut unchanged = 0;
    let mut keys: Vec<&String> = before.keys().chain(after.keys()).collect();
    keys.sort_unstable();
    keys.dedup();
    for key in keys {
        match (before.get(key), after.get(key)) {
            (Some(old), Some(new)) => {
                if old.name == new.name && old.items == new.items {
                    unchanged += 1;
                } else {
                    changes.push(SiteCollectionVersionChange {
                        collection_id: new.collection_id.clone(),
                        name: new.name.clone(),
                        change: SiteVersionChange::Changed,
                        items_before: old.items.len(),
                        items_after: new.items.len(),
                    });
                }
            }
            (None, Some(new)) => changes.push(SiteCollectionVersionChange {
                collection_id: new.collection_id.clone(),
                name: new.name.clone(),
                change: SiteVersionChange::Added,
                items_before: 0,
                items_after: new.items.len(),
            }),
            (Some(old), None) => changes.push(SiteCollectionVersionChange {
                collection_id: old.collection_id.clone(),
                name: old.name.clone(),
                change: SiteVersionChange::Removed,
                items_before: old.items.len(),
                items_after: 0,
            }),
            (None, None) => unreachable!("the key came from one of the two maps"),
        }
    }
    (changes, unchanged)
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SitePublishVersionRow {
    id: String,
    published_at: OffsetDateTime,
    published_by: String,
    default_locale: String,
    enabled_locales: Vec<String>,
    restored_from: Option<String>,
    is_current: bool,
    pages: i64,
    locales: Vec<String>,
    collections: i64,
}

impl SitePublishVersionRow {
    fn into_version(self) -> SitePublishVersion {
        let mut locales = self.locales;
        locales.sort();
        SitePublishVersion {
            id: SitePublishId::new(self.id),
            published_at: self.published_at,
            published_by: self.published_by,
            default_locale: self.default_locale,
            enabled_locales: self.enabled_locales,
            restored_from: self.restored_from.map(SitePublishId::new),
            is_current: self.is_current,
            pages: usize::try_from(self.pages).unwrap_or(0),
            locales,
            collections: usize::try_from(self.collections).unwrap_or(0),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::{
        SiteCollectionVersionChange, SitePageVersionField, SiteVersionChange, compare_collections,
        compare_pages, missing_from,
    };
    use crate::id::{SiteCollectionId, SitePageId};
    use crate::site_collections::{SiteCollectionItem, SiteCollectionSnapshot};
    use crate::site_publish::SitePageSnapshot;
    use serde_json::json;

    fn page(id: &str, locale: &str, slug: &str, title: &str, nav: i32) -> SitePageSnapshot {
        SitePageSnapshot {
            page_id: SitePageId::new(id.to_owned()),
            locale: locale.to_owned(),
            slug: slug.to_owned(),
            title: title.to_owned(),
            sections: json!({"schema_version": 1, "sections": []}),
            seo_title: None,
            seo_description: None,
            nav_order: nav,
            is_home: slug.is_empty(),
        }
    }

    fn collection(id: &str, name: &str, titles: &[&str]) -> SiteCollectionSnapshot {
        SiteCollectionSnapshot {
            collection_id: SiteCollectionId::new(id.to_owned()),
            name: name.to_owned(),
            items: titles
                .iter()
                .map(|title| SiteCollectionItem {
                    title: (*title).to_owned(),
                    slug: None,
                    summary: None,
                    body: None,
                    image: None,
                    link: None,
                    published_at: None,
                })
                .collect(),
        }
    }

    #[test]
    fn identical_versions_report_no_page_differences() {
        let pages = vec![
            page("p1", "en", "", "Home", 0),
            page("p2", "en", "a", "A", 1),
        ];
        let (changes, unchanged) = compare_pages(&pages, &pages);
        assert!(changes.is_empty());
        assert_eq!(unchanged, 2);
    }

    #[test]
    fn every_frozen_field_is_named_when_it_differs() {
        let old = page("p1", "en", "old", "Old", 3);
        let mut new = page("p1", "en", "new", "New", 4);
        new.sections = json!({"schema_version": 1, "sections": [{"type": "hero"}]});
        new.seo_title = Some("Seo".to_owned());
        new.seo_description = Some("Desc".to_owned());
        new.is_home = true;
        let (changes, unchanged) = compare_pages(&[old], &[new]);
        assert_eq!(unchanged, 0);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change, SiteVersionChange::Changed);
        assert_eq!(changes[0].title, "New", "the newer name is the one shown");
        assert_eq!(
            changes[0].fields,
            vec![
                SitePageVersionField::Title,
                SitePageVersionField::Slug,
                SitePageVersionField::Sections,
                SitePageVersionField::SeoTitle,
                SitePageVersionField::SeoDescription,
                SitePageVersionField::NavOrder,
                SitePageVersionField::Home,
            ]
        );
    }

    #[test]
    fn a_page_in_two_languages_is_two_comparable_things() {
        let before = vec![
            page("p1", "en", "", "Home", 0),
            page("p1", "fr", "", "Accueil", 0),
        ];
        let mut french = page("p1", "fr", "", "Bienvenue", 0);
        french.sections = json!({"schema_version": 1, "sections": [{"type": "cta"}]});
        let after = vec![page("p1", "en", "", "Home", 0), french];
        let (changes, unchanged) = compare_pages(&before, &after);
        assert_eq!(unchanged, 1, "the English page is untouched");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].locale, "fr");
        assert_eq!(
            changes[0].fields,
            vec![SitePageVersionField::Title, SitePageVersionField::Sections]
        );
    }

    #[test]
    fn added_and_removed_pages_carry_no_field_list() {
        let before = vec![
            page("p1", "en", "", "Home", 0),
            page("p2", "en", "gone", "Gone", 1),
        ];
        let after = vec![
            page("p1", "en", "", "Home", 0),
            page("p3", "en", "new", "New", 1),
        ];
        let (changes, unchanged) = compare_pages(&before, &after);
        assert_eq!(unchanged, 1);
        assert_eq!(changes.len(), 2);
        let added = changes
            .iter()
            .find(|change| change.change == SiteVersionChange::Added)
            .unwrap();
        assert_eq!(added.slug, "new");
        assert!(added.fields.is_empty());
        let removed = changes
            .iter()
            .find(|change| change.change == SiteVersionChange::Removed)
            .unwrap();
        assert_eq!(removed.slug, "gone");
        assert!(removed.fields.is_empty());
    }

    #[test]
    fn differences_read_in_navigation_order() {
        let before = vec![page("p1", "en", "", "Home", 0)];
        let after = vec![
            page("p1", "en", "", "Renamed", 0),
            page("p3", "en", "third", "Third", 3),
            page("p2", "en", "second", "Second", 2),
        ];
        let (changes, _) = compare_pages(&before, &after);
        let order: Vec<&str> = changes.iter().map(|change| change.slug.as_str()).collect();
        assert_eq!(order, vec!["", "second", "third"]);
    }

    #[test]
    fn collections_are_compared_by_name_and_rows() {
        let before = vec![
            collection("c1", "Team", &["Ada", "Grace"]),
            collection("c2", "Cases", &["One"]),
        ];
        let after = vec![
            collection("c1", "Team", &["Ada", "Grace"]),
            collection("c2", "Case studies", &["One"]),
            collection("c3", "Events", &[]),
        ];
        let (changes, unchanged) = compare_collections(&before, &after);
        assert_eq!(unchanged, 1);
        assert_eq!(
            changes,
            vec![
                SiteCollectionVersionChange {
                    collection_id: SiteCollectionId::new("c2".to_owned()),
                    name: "Case studies".to_owned(),
                    change: SiteVersionChange::Changed,
                    items_before: 1,
                    items_after: 1,
                },
                SiteCollectionVersionChange {
                    collection_id: SiteCollectionId::new("c3".to_owned()),
                    name: "Events".to_owned(),
                    change: SiteVersionChange::Added,
                    items_before: 0,
                    items_after: 0,
                },
            ]
        );
    }

    #[test]
    fn a_dropped_collection_is_reported_as_removed() {
        let (changes, unchanged) = compare_collections(&[collection("c1", "Team", &["Ada"])], &[]);
        assert_eq!(unchanged, 0);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change, SiteVersionChange::Removed);
        assert_eq!(changes[0].items_before, 1);
        assert_eq!(changes[0].items_after, 0);
    }

    #[test]
    fn language_differences_keep_the_editors_order() {
        let before = ["en".to_owned(), "fr".to_owned()];
        let after = ["en".to_owned(), "nl".to_owned(), "de".to_owned()];
        assert_eq!(missing_from(&after, &before), vec!["nl", "de"]);
        assert_eq!(missing_from(&before, &after), vec!["fr"]);
    }

    #[test]
    fn change_and_field_tokens_are_the_published_ones() {
        assert_eq!(SiteVersionChange::Added.as_str(), "added");
        assert_eq!(SiteVersionChange::Removed.as_str(), "removed");
        assert_eq!(SiteVersionChange::Changed.as_str(), "changed");
        assert_eq!(SitePageVersionField::SeoTitle.as_str(), "seo_title");
        assert_eq!(SitePageVersionField::NavOrder.as_str(), "nav_order");
        assert_eq!(SitePageVersionField::Home.as_str(), "home");
    }
}
