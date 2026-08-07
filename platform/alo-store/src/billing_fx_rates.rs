//! The euro reference rates a tenant has imported, and the rate a document is
//! issued at (alo Billing, ADR 0035, wave B1.21) — reached through the account
//! door like every other billing record.
//!
//! [`crate::billing_fx`] owns the arithmetic and [`crate::billing_fx_ecb`] owns
//! the published file; this module owns the *rows*: writing them, listing them,
//! and answering the one question an issue asks — **"what was this currency
//! worth on this day?"**
//!
//! Three decisions shape the table:
//!
//! - **The rates are tenant-scoped**, although the published rates are a public
//!   fact. A tenant is audited against the file *it* imported, some member
//!   states prescribe a different published series than the ECB's, and a shared
//!   table would let one tenant's import restate another tenant's books. The
//!   volume is about thirty rows per working day, so isolation costs nothing
//!   here (law 1).
//! - **Everything is quoted against the euro**, the way the file publishes it.
//!   An issuer that keeps books in another currency gets the **cross** of two
//!   euro quotes *from the same publication day*
//!   ([`crate::billing_fx::cross_rate_micro`]) — never two rates from different
//!   days silently combined.
//! - **"On this day" means the last publication at or before it.** Reference
//!   rates are published on TARGET working days only, so an invoice issued on a
//!   Sunday is converted at Friday's rate — which is exactly what EU VAT
//!   Directive art. 91(2) prescribes ("the last preceding date of
//!   publication"). A gap longer than [`MAX_RATE_AGE_DAYS`] is refused rather
//!   than reached across: the longest real gap in the series is four days
//!   (Easter, Christmas), so anything longer means the tenant simply has not
//!   imported the rates, and converting from a stale rate would misstate the
//!   money on a legal document.

use time::{Date, Duration, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::currency as currency_code;
use crate::billing_fx::{FxSnapshot, RATE_SCALE, cross_rate_micro, rate_micro as validate_rate};
use crate::billing_fx_ecb::{QUOTED_AGAINST, parse_reference_rates};
use crate::error::{Result, StoreError};

/// How far back a document may reach for a rate, in days.
///
/// The published series skips weekends and TARGET holidays; the longest real
/// gap is four days (Good Friday to Easter Monday, and the Christmas cluster).
/// Seven admits every one of them and still refuses to convert a document from
/// a rate that is a fortnight old — which would not be the art. 91 rate at all,
/// it would be the last rate somebody remembered to import.
pub const MAX_RATE_AGE_DAYS: i64 = 7;

/// The most rates one list read returns. A tenant's whole history is not a
/// screen; a period and a currency are.
pub const LIST_MAX: i64 = 1_000;

/// How many rows one `INSERT` carries. A full historical import is a few
/// hundred thousand rates, and chunking keeps any single statement's arrays
/// bounded without turning the import into a statement per rate.
const IMPORT_CHUNK: usize = 10_000;

/// Where a stored rate came from — the audit trail an imported figure needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxRateSource {
    /// Parsed out of a published euro reference-rate file.
    Ecb,
    /// Entered by hand, one rate at a time.
    Manual,
}

impl FxRateSource {
    /// The value stored in the `source` column.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ecb => "ecb",
            Self::Manual => "manual",
        }
    }

    /// Parses a stored source, or `None` if it is not one we know.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ecb" => Some(Self::Ecb),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

/// One stored rate: on this day, one euro bought this much of this currency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FxRate {
    /// ISO 4217 code of the quoted currency, uppercase.
    pub currency: String,
    /// The day the rate was published.
    pub rate_date: Date,
    /// Micro-units of `currency` per one euro
    /// ([`crate::billing_fx::RATE_SCALE`]).
    pub rate_micro: i64,
    /// Where the row came from.
    pub source: FxRateSource,
    /// Who wrote it.
    pub updated_by: String,
    /// When it was written.
    pub updated_at: OffsetDateTime,
}

/// What an import did, in the words a confirmation screen needs: how many rates
/// landed, over how many days and currencies, and which days they span.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FxImport {
    /// How many rates were written (new ones and corrections together — a
    /// re-imported day is an overwrite, and pretending to tell the two apart
    /// would mean reading every row back first).
    pub rates: i64,
    /// How many distinct publication days the file held.
    pub days: i64,
    /// How many distinct currencies it quoted.
    pub currencies: i64,
    /// The earliest day imported, `None` for a file with no data rows.
    pub from: Option<Date>,
    /// The latest day imported.
    pub to: Option<Date>,
}

#[derive(sqlx::FromRow)]
struct RateRow {
    currency: String,
    rate_date: Date,
    rate_micro: i64,
    source: String,
    updated_by: String,
    updated_at: OffsetDateTime,
}

impl RateRow {
    fn into_rate(self) -> Result<FxRate> {
        let source = FxRateSource::parse(&self.source).ok_or_else(|| {
            StoreError::Validation(format!("unknown stored rate source: {}", self.source))
        })?;
        Ok(FxRate {
            currency: self.currency,
            rate_date: self.rate_date,
            rate_micro: self.rate_micro,
            source,
            updated_by: self.updated_by,
            updated_at: self.updated_at,
        })
    }
}

/// The columns every read selects, in `RateRow` order.
const RATE_COLS: &str = "currency, rate_date, rate_micro, source, updated_by, updated_at";

impl AccountStore {
    /// Writes one rate by hand: one euro bought `rate_micro` micro-units of
    /// `currency` on `on`.
    ///
    /// Re-writing a day overwrites it — which is how a published correction, or
    /// a typo, is fixed. Documents already issued are unaffected: they carry
    /// their own snapshot ([`FxSnapshot`]), so correcting the table never
    /// restates a document that is already in a customer's hands.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the currency is not a three-letter code,
    /// when it is the euro (the currency everything is quoted *against*, whose
    /// rate against itself is not a stored fact), or when the rate is outside
    /// the usable range; [`StoreError::Db`] on failure.
    pub async fn save_billing_fx_rate(
        &self,
        currency: &str,
        on: Date,
        rate_micro: i64,
    ) -> Result<()> {
        let currency = quoted_currency(currency)?;
        let rate_micro = validate_rate(rate_micro)?;
        sqlx::query(
            "INSERT INTO billing_fx_rates \
                 (tenant_id, currency, rate_date, rate_micro, source, updated_by) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (tenant_id, currency, rate_date) DO UPDATE \
                 SET rate_micro = EXCLUDED.rate_micro, source = EXCLUDED.source, \
                     updated_by = EXCLUDED.updated_by, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(&currency)
        .bind(on)
        .bind(rate_micro)
        .bind(FxRateSource::Manual.as_str())
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Imports a published euro reference-rate file
    /// ([`crate::billing_fx_ecb`]) for this tenant.
    ///
    /// **All or nothing.** The file is parsed in full before a single row is
    /// written, and the writes share one transaction: a file with a corrupt cell
    /// leaves the table exactly as it was, because half an imported day would
    /// convert the next document issued from rates the tenant believes it
    /// imported completely.
    ///
    /// A day the file states twice is written once, with the **last** of the two
    /// values — the file's own last word, and the only reading that lets one
    /// statement carry the whole chunk.
    ///
    /// # Errors
    /// [`StoreError::Validation`] with the row and column when the file is not a
    /// reference-rate file; [`StoreError::Db`] on failure.
    pub async fn import_billing_fx_rates(&self, text: &str) -> Result<FxImport> {
        let published = parse_reference_rates(text)?;

        // Deduplicated on (currency, day), last statement winning. Also what
        // makes the upsert below legal: one statement may not touch a row twice.
        let mut rows: Vec<(String, Date, i64)> = Vec::with_capacity(published.len());
        for rate in published {
            let key = (rate.currency.clone(), rate.day);
            match rows
                .iter_mut()
                .find(|(currency, day, _)| *currency == key.0 && *day == key.1)
            {
                Some(existing) => existing.2 = rate.rate_micro,
                None => rows.push((rate.currency, rate.day, rate.rate_micro)),
            }
        }

        let mut summary = FxImport::default();
        let mut days: Vec<Date> = Vec::new();
        let mut currencies: Vec<String> = Vec::new();
        for (currency, day, _) in &rows {
            if let Err(at) = days.binary_search(day) {
                days.insert(at, *day);
            }
            if let Err(at) = currencies.binary_search(currency) {
                currencies.insert(at, currency.clone());
            }
        }
        summary.rates = rows.len() as i64;
        summary.days = days.len() as i64;
        summary.currencies = currencies.len() as i64;
        summary.from = days.first().copied();
        summary.to = days.last().copied();

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        for chunk in rows.chunks(IMPORT_CHUNK) {
            let mut codes: Vec<String> = Vec::with_capacity(chunk.len());
            let mut dates: Vec<Date> = Vec::with_capacity(chunk.len());
            let mut micros: Vec<i64> = Vec::with_capacity(chunk.len());
            for (currency, day, rate_micro) in chunk {
                codes.push(currency.clone());
                dates.push(*day);
                micros.push(*rate_micro);
            }
            sqlx::query(
                "INSERT INTO billing_fx_rates \
                     (tenant_id, currency, rate_date, rate_micro, source, updated_by) \
                 SELECT $1, code, day, micro, $5, $6 \
                   FROM unnest($2::text[], $3::date[], $4::bigint[]) AS f(code, day, micro) \
                 ON CONFLICT (tenant_id, currency, rate_date) DO UPDATE \
                     SET rate_micro = EXCLUDED.rate_micro, source = EXCLUDED.source, \
                         updated_by = EXCLUDED.updated_by, updated_at = now()",
            )
            .bind(self.tenant.as_str())
            .bind(&codes)
            .bind(&dates)
            .bind(&micros)
            .bind(FxRateSource::Ecb.as_str())
            .bind(self.user.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(summary)
    }

    /// This tenant's stored rates, newest day first, optionally narrowed to one
    /// currency and to a period. At most [`LIST_MAX`] rows.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the currency is not a three-letter code
    /// or the period ends before it starts; [`StoreError::Db`] on failure.
    pub async fn billing_fx_rate_list(
        &self,
        currency: Option<&str>,
        from: Option<Date>,
        to: Option<Date>,
    ) -> Result<Vec<FxRate>> {
        let currency = currency.map(currency_code).transpose()?;
        if let (Some(from), Some(to)) = (from, to)
            && to < from
        {
            return Err(StoreError::Validation(
                "the end of the period must not be before its start".to_owned(),
            ));
        }
        let rows = sqlx::query_as::<_, RateRow>(&format!(
            "SELECT {RATE_COLS} FROM billing_fx_rates \
             WHERE tenant_id = $1 \
               AND ($2::text IS NULL OR currency = $2) \
               AND ($3::date IS NULL OR rate_date >= $3) \
               AND ($4::date IS NULL OR rate_date <= $4) \
             ORDER BY rate_date DESC, currency \
             LIMIT $5"
        ))
        .bind(self.tenant.as_str())
        .bind(currency.as_deref())
        .bind(from)
        .bind(to)
        .bind(LIST_MAX)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(RateRow::into_rate).collect()
    }

    /// The rate this tenant would convert `currency` at on `on`: the last
    /// publication at or before that day, or `None` when there is none within
    /// [`MAX_RATE_AGE_DAYS`].
    ///
    /// The euro answers itself — it is the currency the table quotes against, so
    /// its rate is the identity rather than a row somebody has to remember to
    /// import.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the currency is not a three-letter code;
    /// [`StoreError::Db`] on failure.
    pub async fn billing_fx_rate_on(&self, currency: &str, on: Date) -> Result<Option<FxRate>> {
        let currency = currency_code(currency)?;
        if currency == QUOTED_AGAINST {
            return Ok(None);
        }
        let row = sqlx::query_as::<_, RateRow>(&format!(
            "SELECT {RATE_COLS} FROM billing_fx_rates \
             WHERE tenant_id = $1 AND currency = $2 AND rate_date <= $3 AND rate_date >= $4 \
             ORDER BY rate_date DESC LIMIT 1"
        ))
        .bind(self.tenant.as_str())
        .bind(&currency)
        .bind(on)
        .bind(oldest_usable(on))
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(RateRow::into_rate).transpose()
    }
}

/// The oldest publication day a document issued on `on` may be converted from.
fn oldest_usable(on: Date) -> Date {
    on.checked_sub(Duration::days(MAX_RATE_AGE_DAYS))
        .unwrap_or(Date::MIN)
}

/// Validates a quoted currency: a three-letter code that is not the currency
/// the table quotes *against*.
fn quoted_currency(value: &str) -> Result<String> {
    let currency = currency_code(value)?;
    if currency == QUOTED_AGAINST {
        return Err(StoreError::Validation(format!(
            "reference rates are quoted against the {QUOTED_AGAINST}, so it has no rate of its own"
        )));
    }
    Ok(currency)
}

/// The snapshot a document raised in `document_currency` is issued at on `on`,
/// inside the issuing transaction.
///
/// Takes the transaction rather than the pool because the snapshot is part of
/// what issuing *is*: the rate is read under the same lock, and in the same
/// atomic step, as the number and the dates it is frozen alongside.
///
/// - The base currency answers itself with the identity rate, so a
///   single-currency tenant never needs a rate table at all.
/// - A euro-based issuer reads one published rate.
/// - Any other issuer reads **two rates of the same publication day** and
///   crosses them, so the number on the document is one auditable figure rather
///   than a pair a reader has to re-cross.
///
/// # Errors
/// [`StoreError::Validation`] naming the currency and the day when no usable
/// rate has been imported — the document is refused rather than issued at a
/// guessed rate; [`StoreError::Db`] on failure.
pub(crate) async fn snapshot_at(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    base_currency: &str,
    document_currency: &str,
    on: Date,
) -> Result<FxSnapshot> {
    if document_currency == base_currency {
        return Ok(FxSnapshot::identity(base_currency, on));
    }
    let missing = |currency: &str| {
        StoreError::Validation(format!(
            "no exchange rate for {currency} published on or within {MAX_RATE_AGE_DAYS} days \
             before {on}; import the reference rates before issuing this document"
        ))
    };

    // The euro side of a euro-quoted table is the identity, so a euro-based
    // issuer needs one row and a euro-denominated document needs none.
    if base_currency == QUOTED_AGAINST {
        let (rate_micro, rate_date) = rate_at(tx, tenant, document_currency, on)
            .await?
            .ok_or_else(|| missing(document_currency))?;
        return Ok(FxSnapshot {
            base_currency: base_currency.to_owned(),
            rate_micro,
            rate_date,
        });
    }
    if document_currency == QUOTED_AGAINST {
        let (base_micro, rate_date) = rate_at(tx, tenant, base_currency, on)
            .await?
            .ok_or_else(|| missing(base_currency))?;
        let rate_micro =
            cross_rate_micro(RATE_SCALE, base_micro).ok_or_else(|| missing(base_currency))?;
        return Ok(FxSnapshot {
            base_currency: base_currency.to_owned(),
            rate_micro,
            rate_date,
        });
    }

    // Two currencies, neither of them the euro: the newest day that quotes
    // BOTH. Two rates from different days would be a rate that was never
    // published.
    let row: Option<(Date, i64, i64)> = sqlx::query_as(
        "SELECT rate_date, \
                max(rate_micro) FILTER (WHERE currency = $2) AS quote, \
                max(rate_micro) FILTER (WHERE currency = $3) AS base \
           FROM billing_fx_rates \
          WHERE tenant_id = $1 AND currency IN ($2, $3) \
            AND rate_date <= $4 AND rate_date >= $5 \
          GROUP BY rate_date \
         HAVING count(DISTINCT currency) = 2 \
          ORDER BY rate_date DESC LIMIT 1",
    )
    .bind(tenant)
    .bind(document_currency)
    .bind(base_currency)
    .bind(on)
    .bind(oldest_usable(on))
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    let (rate_date, quote_micro, base_micro) = row.ok_or_else(|| {
        StoreError::Validation(format!(
            "no publication day within {MAX_RATE_AGE_DAYS} days before {on} quotes both \
             {document_currency} and {base_currency}; import the reference rates before \
             issuing this document"
        ))
    })?;
    let rate_micro =
        cross_rate_micro(quote_micro, base_micro).ok_or_else(|| missing(document_currency))?;
    Ok(FxSnapshot {
        base_currency: base_currency.to_owned(),
        rate_micro,
        rate_date,
    })
}

/// The last rate for `currency` published at or before `on`, within
/// [`MAX_RATE_AGE_DAYS`], inside `tx`.
async fn rate_at(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    currency: &str,
    on: Date,
) -> Result<Option<(i64, Date)>> {
    sqlx::query_as(
        "SELECT rate_micro, rate_date FROM billing_fx_rates \
         WHERE tenant_id = $1 AND currency = $2 AND rate_date <= $3 AND rate_date >= $4 \
         ORDER BY rate_date DESC LIMIT 1",
    )
    .bind(tenant)
    .bind(currency)
    .bind(on)
    .bind(oldest_usable(on))
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::Db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn every_source_round_trips_through_its_stored_form() {
        for source in [FxRateSource::Ecb, FxRateSource::Manual] {
            assert_eq!(FxRateSource::parse(source.as_str()), Some(source));
        }
        for bad in ["", "ECB", "Manual", "ecb ", "imported"] {
            assert_eq!(FxRateSource::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_euro_is_never_a_quoted_currency() {
        assert!(quoted_currency("USD").is_ok());
        assert_eq!(quoted_currency("usd").unwrap_or_default(), "USD");
        for bad in ["EUR", "eur", " eur "] {
            let message = match quoted_currency(bad) {
                Err(StoreError::Validation(message)) => message,
                other => panic!("expected Validation for {bad:?}, got {other:?}"),
            };
            assert!(message.contains("quoted against"), "{message}");
        }
        assert!(quoted_currency("EU").is_err(), "shape still applies");
    }

    #[test]
    fn a_document_reaches_back_a_week_and_no_further() {
        // The Easter gap (Thursday's rate used on Monday) and the Christmas one
        // are four days; a fortnight is not a gap, it is a missing import.
        let issued = day(2026, Month::August, 7);
        assert_eq!(oldest_usable(issued), day(2026, Month::July, 31));
        // Total at the edge of the calendar rather than panicking.
        assert_eq!(oldest_usable(Date::MIN), Date::MIN);
    }
}
