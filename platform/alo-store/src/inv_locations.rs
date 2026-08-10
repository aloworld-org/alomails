//! Locations — the places stock can be, and the four virtual counterparties it
//! comes from and goes to (alo Inventory, ADR 0035, wave B5.04a;
//! `docs/design/inventory.md`, "Locations and the move ledger").
//!
//! Locations are **tenant-wide**: everybody counting the same warehouse counts
//! the same rows, so the predicate on every statement is `tenant_id`, taken
//! from the handle and never from request input.
//!
//! Two things make this file worth reading before [`crate::inv_moves`]:
//!
//! - **[`LocationKind`] is what the ledger's rules are about.** `stock` and
//!   `transit` are real places whose balance may never go negative; `supplier`,
//!   `customer`, `adjustment` and `production` are virtual counterparties,
//!   exactly one of each per tenant, unbounded by construction — `supplier`
//!   goes ever more negative as we buy, which is the correct reading of "how
//!   much has come from outside". Only the two real kinds can be created,
//!   archived or deleted through any door.
//! - **A tenant is seeded a working set on first use, not in the migration.**
//!   [`AccountStore::inv_locations_or_seed`] is [`crate::fin_accounts`]'
//!   mechanism reused whole, down to the `inv_seeds` ledger row that records
//!   *that the seed ran* separately from what it wrote — so a tenant who
//!   deletes the warehouse we gave them is not handed it back the next
//!   morning, and two simultaneous first reads produce one set without a lock.
//!
//! **No English lives here.** The seed's *names* arrive from the HTTP edge in
//! the language of whoever opened Inventory first, exactly as the chart of
//! accounts' do; a location called `Warehouse` in a Dutch tenant would be a
//! hardcoded English string in a European product. The seeded *codes* are
//! minted here from the kind, because a code is an identifier — the shape
//! [`crate::fin_accounts`] has, where `CHART` owns the codes and the caller
//! owns the names. A screen shows the name; the code is what a person types
//! into a count sheet, and a tenant may rename either.
//!
//! A location that has carried a movement is **archived, never deleted**: its
//! name is part of the explanation of that movement. Delete exists only for
//! the mistake made a minute ago, and the database enforces the rule as well
//! as this module does (`inv_moves`' keys restrict).

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::required;
use crate::error::{Result, StoreError};
use crate::id::InvLocationId;

/// A location code is a shelf label, not a sentence — and it is what a person
/// types on a phone in a warehouse.
pub const LOCATION_CODE_MAX_CHARS: usize = 32;
/// A location name is a label on a stock screen.
pub const LOCATION_NAME_MAX_CHARS: usize = 120;

/// The `inv_seeds` key under which the starting set of locations is recorded.
/// Ours, never a caller's — nothing accepts a seed key as input.
pub const LOCATION_SEED_KEY: &str = "starting_locations";

/// The columns every read of a location selects, in `LocationRow` order.
const LOCATION_COLS: &str = "id, code, name, kind, archived_at, created_by, created_at, updated_at";

/// What a location *is* — the column every rule in the ledger consults.
///
/// The set is closed and it does not grow: a seventh kind would not be a new
/// place, it would be a different model of where goods can be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationKind {
    /// A real place: a warehouse, a shop floor, a van. On-hand here is a claim
    /// about physical goods and may never go negative.
    Stock,
    /// Real too, but nobody counts it: goods that have left one location and
    /// not yet arrived at another. Held to the same non-negative rule.
    Transit,
    /// Where received goods come from. Virtual, one per tenant.
    Supplier,
    /// Where delivered goods go. Virtual, one per tenant.
    Customer,
    /// The counterparty of every correction and stocktake variance. Virtual,
    /// one per tenant.
    Adjustment,
    /// Seeded and unused in B5 — assembly is a stated cut — so the day it is
    /// needed there is no migration and no new kind. Virtual, one per tenant.
    Production,
}

impl LocationKind {
    /// The stored word — the database value and the wire form, one spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stock => "stock",
            Self::Transit => "transit",
            Self::Supplier => "supplier",
            Self::Customer => "customer",
            Self::Adjustment => "adjustment",
            Self::Production => "production",
        }
    }

    /// Reads the stored word back.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set when the word is not
    /// one of the six.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "stock" => Ok(Self::Stock),
            "transit" => Ok(Self::Transit),
            "supplier" => Ok(Self::Supplier),
            "customer" => Ok(Self::Customer),
            "adjustment" => Ok(Self::Adjustment),
            "production" => Ok(Self::Production),
            _ => Err(StoreError::Validation(
                "location kind must be stock, transit, supplier, customer, adjustment or \
                 production"
                    .to_owned(),
            )),
        }
    }

    /// Whether the kind is one of the four the system owns: seeded once per
    /// tenant, never created, archived or deleted through a door.
    pub fn is_virtual(self) -> bool {
        !self.is_real()
    }

    /// Whether the kind is a place a person could walk into and count.
    ///
    /// **This is the negative-stock rule's question** ([`crate::inv_moves`]):
    /// a balance that claims physical goods may never go below zero, and a
    /// balance that describes the outside world is unbounded by construction.
    pub fn is_real(self) -> bool {
        matches!(self, Self::Stock | Self::Transit)
    }
}

/// The four virtual counterparties, in the order the seed writes them.
pub const VIRTUAL_KINDS: [LocationKind; 4] = [
    LocationKind::Supplier,
    LocationKind::Customer,
    LocationKind::Adjustment,
    LocationKind::Production,
];

/// The code minted for a seeded location. Ours, and stable: the virtuals are
/// found by [`LocationKind`] and never by code, so this is a label a tenant may
/// rename freely without breaking a single rule.
fn seeded_code(kind: LocationKind) -> &'static str {
    match kind {
        LocationKind::Stock => "MAIN",
        LocationKind::Transit => "TRANSIT",
        LocationKind::Supplier => "SUPPLIER",
        LocationKind::Customer => "CUSTOMER",
        LocationKind::Adjustment => "ADJUST",
        LocationKind::Production => "PRODUCTION",
    }
}

/// The names a tenant's starting locations are given, in the language of
/// whoever opens Inventory first.
///
/// Every field is required and non-blank: a tenant handed half a set of
/// locations is worse off than one handed none, because the missing one is
/// discovered by a receipt failing months later.
#[derive(Debug, Clone, Default)]
pub struct LocationSeed {
    /// The one real place a tenant that ships from a single room needs.
    pub stock: String,
    /// Where received goods come from.
    pub supplier: String,
    /// Where delivered goods go.
    pub customer: String,
    /// The counterparty of corrections and stocktake variances.
    pub adjustment: String,
    /// Reserved for assembly; seeded so the day it is needed is not a
    /// migration.
    pub production: String,
}

impl LocationSeed {
    /// The name this seed gives a kind, or `None` for a kind it does not seed
    /// ([`LocationKind::Transit`] — a tenant with one warehouse does not need
    /// one, and a tenant with two creates it themselves).
    fn name_for(&self, kind: LocationKind) -> Option<&str> {
        match kind {
            LocationKind::Stock => Some(&self.stock),
            LocationKind::Supplier => Some(&self.supplier),
            LocationKind::Customer => Some(&self.customer),
            LocationKind::Adjustment => Some(&self.adjustment),
            LocationKind::Production => Some(&self.production),
            LocationKind::Transit => None,
        }
    }
}

/// The writable shape of a location, used for both create and update (an
/// update is a full replace — the route layer merges a partial `PATCH` onto the
/// stored record before calling).
#[derive(Debug, Clone)]
pub struct NewLocation {
    /// What a person types: uppercased and space-free in the store.
    pub code: String,
    /// What a screen shows.
    pub name: String,
    /// What the place is. Only [`LocationKind::Stock`] and
    /// [`LocationKind::Transit`] may be created, and a stored location's kind
    /// never changes.
    pub kind: LocationKind,
}

impl Default for NewLocation {
    fn default() -> Self {
        Self {
            code: String::new(),
            name: String::new(),
            kind: LocationKind::Stock,
        }
    }
}

/// A stored location.
#[derive(Debug, Clone)]
pub struct Location {
    /// Opaque id, unique within the tenant.
    pub id: InvLocationId,
    /// The code a person types, uppercase.
    pub code: String,
    /// What the place is called.
    pub name: String,
    /// What kind of place it is.
    pub kind: LocationKind,
    /// When it was archived; `None` while active.
    pub archived_at: Option<OffsetDateTime>,
    /// The user who created it — the seed's first reader, for a seeded row.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
}

impl Location {
    /// Whether the location is archived — out of the pickers, still nameable by
    /// every movement that already happened there.
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

/// A validated, normalised location ready to be bound into a statement.
#[derive(Debug)]
struct Normalized {
    code: String,
    name: String,
}

/// Validates and normalises the two text fields. Pure — no database, so the
/// rules are unit-tested directly.
///
/// The code is uppercased and refused any whitespace, for the reason an account
/// code is: `wh1` and `WH1` are the same shelf to every human who reads a
/// label, and storing both produces two rows a stock report shows as separate
/// lines with the same meaning.
fn normalize(input: &NewLocation) -> Result<Normalized> {
    let code =
        required("location code", &input.code, LOCATION_CODE_MAX_CHARS)?.to_ascii_uppercase();
    if !code
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        || !code.starts_with(|c: char| c.is_ascii_alphanumeric())
    {
        return Err(StoreError::Validation(
            "location code must start with a letter or digit and use only letters, digits, \
             dot, dash or underscore"
                .to_owned(),
        ));
    }
    Ok(Normalized {
        code,
        name: required("location name", &input.name, LOCATION_NAME_MAX_CHARS)?,
    })
}

/// Turns the two uniqueness rules into typed conflicts naming which was hit,
/// and leaves every other database failure alone.
///
/// Both indexes are tenant-scoped, so a conflict here is always **this**
/// tenant's own other location — never a signal about somebody else's
/// warehouse.
fn map_location_conflict(error: sqlx::Error) -> StoreError {
    match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
            match db.constraint().unwrap_or_default() {
                "inv_locations_code_unique" => {
                    StoreError::Conflict("a location with this code already exists".to_owned())
                }
                "inv_locations_one_per_virtual_kind" => {
                    StoreError::Conflict("this tenant already has that system location".to_owned())
                }
                _ => StoreError::Conflict("unique constraint".to_owned()),
            }
        }
        // Something still points at the location: a movement, or the cached
        // balance a movement wrote. Both mean the same thing to a person, and
        // the answer is the one the design note states — archive it instead.
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23503") => {
            StoreError::Conflict(
                "a location that has carried movements cannot be deleted".to_owned(),
            )
        }
        other => StoreError::Db(other),
    }
}

impl AccountStore {
    /// The tenant's locations, **seeding the starting set on first use**.
    ///
    /// A tenant that has never opened Inventory is given a working set: one
    /// real place to keep things and the four virtual counterparties every
    /// document movement needs, so receiving the first purchase order books
    /// itself instead of failing with "there is nowhere to put it". An empty
    /// state that is a working state — the onboarding law, applied to a table.
    ///
    /// Seeding is a first-use rule, not an every-read one: a tenant that
    /// deleted what we gave them and named their own warehouses is not handed
    /// ours again the next morning, because the question asked is whether the
    /// seed has ever *run* (the `inv_seeds` ledger), not whether the rows are
    /// still there.
    ///
    /// Two first reads at the same instant produce exactly one set: the loser
    /// of the race on the ledger's primary key writes nothing and reads back
    /// what the winner wrote.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the seed itself is malformed (a missing
    /// or blank name); [`StoreError::Db`] on failure.
    pub async fn inv_locations_or_seed(
        &self,
        seed: &LocationSeed,
        include_archived: bool,
    ) -> Result<Vec<Location>> {
        let rows = normalize_seed(seed)?;
        if !self.inv_seed_ran(LOCATION_SEED_KEY).await? {
            match self.seed_locations(&rows).await {
                // A concurrent first read won: its locations are the tenant's.
                Ok(()) | Err(StoreError::Conflict(_)) => {}
                Err(other) => return Err(other),
            }
        }
        self.inv_locations(include_archived).await
    }

    /// Whether the seed named by `system_key` has ever run for this tenant —
    /// the ledger's question, which survives the rows it wrote.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_seed_ran(&self, system_key: &str) -> Result<bool> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM inv_seeds WHERE tenant_id = $1 AND system_key = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(system_key)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    /// Writes the ledger row and every seeded location in **one transaction**:
    /// a tenant is never left holding three of the five, and never left with a
    /// ledger row and no locations.
    async fn seed_locations(&self, rows: &[(LocationKind, String)]) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let claimed = sqlx::query(
            "INSERT INTO inv_seeds (tenant_id, system_key, seeded_by) \
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(LOCATION_SEED_KEY)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if claimed.rows_affected() == 0 {
            // Somebody else is writing them, or already has. Nothing of ours is
            // committed, and the caller reads back their locations.
            return Ok(());
        }
        for (kind, name) in rows {
            sqlx::query(
                "INSERT INTO inv_locations (tenant_id, id, code, name, kind, created_by) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(self.tenant.as_str())
            .bind(InvLocationId::generate().as_str())
            .bind(seeded_code(*kind))
            .bind(name)
            .bind(kind.as_str())
            .bind(self.user.as_str())
            .execute(&mut *tx)
            .await
            .map_err(map_location_conflict)?;
        }
        tx.commit().await.map_err(StoreError::Db)
    }

    /// The tenant's locations in code order. Archived ones are excluded unless
    /// `include_archived`, and then sort after the active ones.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_locations(&self, include_archived: bool) -> Result<Vec<Location>> {
        let rows = sqlx::query_as::<_, LocationRow>(&format!(
            "SELECT {LOCATION_COLS} FROM inv_locations \
             WHERE tenant_id = $1 AND ($2 OR archived_at IS NULL) \
             ORDER BY (archived_at IS NOT NULL), code"
        ))
        .bind(self.tenant.as_str())
        .bind(include_archived)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(LocationRow::into_location).collect()
    }

    /// One location of the tenant, or `None` — including when the id belongs to
    /// another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_location(&self, id: &InvLocationId) -> Result<Option<Location>> {
        let row = sqlx::query_as::<_, LocationRow>(&format!(
            "SELECT {LOCATION_COLS} FROM inv_locations WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(LocationRow::into_location).transpose()
    }

    /// **The lookup every document movement makes**: the tenant's location for
    /// a virtual kind — where a receipt's goods come from, where a delivery's
    /// go, what a stocktake variance is booked against.
    ///
    /// `None` means the tenant has never opened Inventory (or deleted what they
    /// were given), and the document that asked is refused rather than posted
    /// somewhere plausible. Asking by kind rather than by code is what lets a
    /// tenant rename every location without breaking a rule.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_location_of_kind(&self, kind: LocationKind) -> Result<Option<Location>> {
        let row = sqlx::query_as::<_, LocationRow>(&format!(
            "SELECT {LOCATION_COLS} FROM inv_locations \
             WHERE tenant_id = $1 AND kind = $2 AND archived_at IS NULL \
             ORDER BY code LIMIT 1"
        ))
        .bind(self.tenant.as_str())
        .bind(kind.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(LocationRow::into_location).transpose()
    }

    /// Creates a real place — a warehouse, a shop floor, a van, or the transit
    /// a second warehouse needs.
    ///
    /// The four virtual counterparties are **not creatable**: exactly one of
    /// each exists per tenant, written by the seed, because a receipt that
    /// could choose between two supplier locations makes every balance on it a
    /// half-truth.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a blank or malformed code or name, or on
    /// asking for a virtual kind; [`StoreError::Conflict`] when the code is
    /// already another of this tenant's locations'; [`StoreError::Db`] on
    /// failure.
    pub async fn create_inv_location(&self, input: &NewLocation) -> Result<InvLocationId> {
        if input.kind.is_virtual() {
            return Err(StoreError::Validation(
                "only stock and transit locations can be created".to_owned(),
            ));
        }
        let l = normalize(input)?;
        let id = InvLocationId::generate();
        sqlx::query(
            "INSERT INTO inv_locations (tenant_id, id, code, name, kind, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&l.code)
        .bind(&l.name)
        .bind(input.kind.as_str())
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(map_location_conflict)?;
        Ok(id)
    }

    /// Renames a location — code and name, both of which a tenant owns,
    /// including on the four seeded virtuals (a system location is renameable
    /// for the reason a system account is: our word for it was a starting
    /// point, not a claim).
    ///
    /// **The kind never changes.** It is what every rule in the ledger is
    /// about, and re-kinding a location retroactively rewrites the meaning of
    /// every movement already recorded there — turning a warehouse into a
    /// virtual counterparty would make its balance stop being a claim about
    /// physical goods without a single quantity moving.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a blank or malformed field, or on a kind
    /// that differs from the stored one; [`StoreError::NotFound`] when the
    /// location isn't the tenant's; [`StoreError::Conflict`] when the code is
    /// taken; [`StoreError::Db`] on failure.
    pub async fn update_inv_location(&self, id: &InvLocationId, input: &NewLocation) -> Result<()> {
        let l = normalize(input)?;
        let stored = self.inv_location(id).await?.ok_or(StoreError::NotFound)?;
        if stored.kind != input.kind {
            return Err(StoreError::Validation(
                "a location's kind cannot change".to_owned(),
            ));
        }
        let done = sqlx::query(
            "UPDATE inv_locations SET code = $3, name = $4, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&l.code)
        .bind(&l.name)
        .execute(&self.pool)
        .await
        .map_err(map_location_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Archives or restores a location — the removal that keeps history
    /// readable. Idempotent: re-archiving keeps the original time.
    ///
    /// A virtual counterparty cannot be archived: every receipt, delivery and
    /// stocktake in the system needs it, and hiding it would break those
    /// documents in the one way a person could not diagnose.
    ///
    /// Archiving a location that still holds stock is **allowed**, deliberately
    /// — a shed being emptied is archived before the last pallet leaves it, and
    /// refusing would make the tidy-up impossible. What it stops is *new*
    /// movements: the pickers no longer offer it.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] on a virtual location; [`StoreError::NotFound`]
    /// when the location isn't the tenant's; [`StoreError::Db`] on failure.
    pub async fn set_inv_location_archived(
        &self,
        id: &InvLocationId,
        archived: bool,
    ) -> Result<()> {
        let stored = self.inv_location(id).await?.ok_or(StoreError::NotFound)?;
        if stored.kind.is_virtual() {
            return Err(StoreError::Conflict(
                "a system location cannot be archived".to_owned(),
            ));
        }
        let done = sqlx::query(
            "UPDATE inv_locations \
             SET archived_at = CASE WHEN $3 THEN COALESCE(archived_at, now()) ELSE NULL END, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(archived)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes a location that has **never carried a movement** — the escape
    /// hatch for the row created a minute ago with a typo in its code, and
    /// nothing more.
    ///
    /// Once anything has moved through it, its name is part of the explanation
    /// of that movement and the answer is
    /// [`AccountStore::set_inv_location_archived`]. The refusal is the
    /// database's as well as this function's: `inv_moves` and `inv_stock` both
    /// restrict, so a bug here still cannot delete history.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] on a virtual location or one that carries
    /// movements; [`StoreError::NotFound`] when the location isn't the
    /// tenant's; [`StoreError::Db`] on failure.
    pub async fn delete_inv_location(&self, id: &InvLocationId) -> Result<()> {
        let stored = self.inv_location(id).await?.ok_or(StoreError::NotFound)?;
        if stored.kind.is_virtual() {
            return Err(StoreError::Conflict(
                "a system location cannot be deleted".to_owned(),
            ));
        }
        let done = sqlx::query("DELETE FROM inv_locations WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(map_location_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Resolves a location id to the row a movement needs — **this tenant's**,
    /// so a guessed id from another tenant is a [`StoreError::NotFound`] rather
    /// than a cross-tenant movement.
    ///
    /// Archived locations resolve: a movement *out of* the shed being emptied
    /// is exactly what archiving must not block ([`crate::inv_moves`] decides
    /// which direction is allowed, not this).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is not this tenant's location;
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn require_tenant_location(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &InvLocationId,
    ) -> Result<Location> {
        let row = sqlx::query_as::<_, LocationRow>(&format!(
            "SELECT {LOCATION_COLS} FROM inv_locations WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        row.ok_or(StoreError::NotFound)
            .and_then(LocationRow::into_location)
    }
}

/// Checks the seed: one non-blank name for each kind it seeds, normalised the
/// way a caller's own name would be.
///
/// It is *our* input rather than a caller's, so a failure here is a bug — and a
/// bug that hands a tenant three of the five locations is worse than one that
/// refuses to write any.
fn normalize_seed(seed: &LocationSeed) -> Result<Vec<(LocationKind, String)>> {
    let mut out = Vec::with_capacity(1 + VIRTUAL_KINDS.len());
    for kind in std::iter::once(LocationKind::Stock).chain(VIRTUAL_KINDS) {
        let Some(name) = seed.name_for(kind) else {
            continue;
        };
        out.push((
            kind,
            required(
                &format!("the {} location name", kind.as_str()),
                name,
                LOCATION_NAME_MAX_CHARS,
            )?,
        ));
    }
    Ok(out)
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct LocationRow {
    id: String,
    code: String,
    name: String,
    kind: String,
    archived_at: Option<OffsetDateTime>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl LocationRow {
    fn into_location(self) -> Result<Location> {
        Ok(Location {
            id: InvLocationId::new(self.id),
            code: self.code,
            name: self.name,
            kind: LocationKind::parse(&self.kind)?,
            archived_at: self.archived_at,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    fn full_seed() -> LocationSeed {
        LocationSeed {
            stock: "Hoofdmagazijn".to_owned(),
            supplier: "Leveranciers".to_owned(),
            customer: "Klanten".to_owned(),
            adjustment: "Correcties".to_owned(),
            production: "Productie".to_owned(),
        }
    }

    #[test]
    fn the_kind_vocabulary_round_trips_and_refuses_anything_else() {
        for kind in [
            LocationKind::Stock,
            LocationKind::Transit,
            LocationKind::Supplier,
            LocationKind::Customer,
            LocationKind::Adjustment,
            LocationKind::Production,
        ] {
            assert_eq!(
                LocationKind::parse(kind.as_str()).unwrap_or(LocationKind::Production),
                kind
            );
        }
        for bad in ["", "warehouse", "STOCK", "shelf"] {
            assert!(invalid(LocationKind::parse(bad)).contains("location kind"));
        }
    }

    #[test]
    fn real_and_virtual_partition_the_vocabulary() {
        // The negative-stock rule asks exactly this question, so the two must
        // stay each other's complement.
        for kind in [LocationKind::Stock, LocationKind::Transit] {
            assert!(kind.is_real() && !kind.is_virtual());
        }
        for kind in VIRTUAL_KINDS {
            assert!(kind.is_virtual() && !kind.is_real());
        }
    }

    #[test]
    fn a_code_is_uppercased_and_shape_checked() {
        let ok = normalize(&NewLocation {
            code: "  wh-1.a_2 ".to_owned(),
            name: " Hoofdmagazijn ".to_owned(),
            kind: LocationKind::Stock,
        })
        .unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(ok.code, "WH-1.A_2");
        assert_eq!(ok.name, "Hoofdmagazijn");

        for bad in ["", "   ", "WH 1", "-WH", ".A", "WH/1", "shelf#3"] {
            let input = NewLocation {
                code: bad.to_owned(),
                name: "Magazijn".to_owned(),
                kind: LocationKind::Stock,
            };
            assert!(
                matches!(normalize(&input), Err(StoreError::Validation(_))),
                "expected rejection: {bad:?}"
            );
        }
        let long = NewLocation {
            code: "A".repeat(LOCATION_CODE_MAX_CHARS + 1),
            name: "Magazijn".to_owned(),
            kind: LocationKind::Stock,
        };
        assert!(invalid(normalize(&long)).contains("at most"));
    }

    #[test]
    fn a_name_is_required_and_bounded() {
        for blank in ["", "   ", "\t"] {
            let input = NewLocation {
                code: "WH1".to_owned(),
                name: blank.to_owned(),
                kind: LocationKind::Stock,
            };
            assert!(invalid(normalize(&input)).contains("location name"));
        }
        let long = NewLocation {
            code: "WH1".to_owned(),
            name: "x".repeat(LOCATION_NAME_MAX_CHARS + 1),
            kind: LocationKind::Stock,
        };
        assert!(invalid(normalize(&long)).contains("at most"));
    }

    #[test]
    fn the_seed_writes_one_real_place_and_the_four_virtuals() {
        let rows = normalize_seed(&full_seed()).unwrap_or_else(|e| panic!("rejected: {e}"));
        let kinds: Vec<LocationKind> = rows.iter().map(|(kind, _)| *kind).collect();
        assert_eq!(
            kinds,
            vec![
                LocationKind::Stock,
                LocationKind::Supplier,
                LocationKind::Customer,
                LocationKind::Adjustment,
                LocationKind::Production,
            ],
            "transit is deliberately not seeded — a tenant with one warehouse \
             does not need one"
        );
        assert!(rows.iter().all(|(_, name)| !name.is_empty()));
        // The codes are ours and distinct, so the uniqueness index cannot fire
        // on our own seed.
        let mut codes: Vec<&str> = kinds.iter().map(|k| seeded_code(*k)).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), kinds.len());
    }

    #[test]
    fn a_seed_missing_a_name_is_refused_whole() {
        for blank_out in [
            LocationSeed {
                stock: "  ".to_owned(),
                ..full_seed()
            },
            LocationSeed {
                supplier: String::new(),
                ..full_seed()
            },
            LocationSeed {
                adjustment: "\n".to_owned(),
                ..full_seed()
            },
        ] {
            assert!(matches!(
                normalize_seed(&blank_out),
                Err(StoreError::Validation(_))
            ));
        }
    }
}
