//! Per-country column mappings for the payroll file (alo HR, ADR 0035, wave
//! B6.10; `docs/design/hr.md`, "Payroll export").
//!
//! # A formatting layer, never a second export
//!
//! [`crate::hr_payroll_export`] decides *what is true* about a period; this
//! module decides *what the sheet looks like*: which columns appear, in which
//! order, under which heading, with which date format, which decimal separator
//! and which field separator. **The data is the same in every mapping.** That is
//! the sentence that keeps this a formatting layer — a mapping that could change
//! a figure would be a second export with a second set of bugs.
//!
//! # Why the mappings are per country, and not per bureau
//!
//! DATEV, SD Worx, Loket and a Polish accountant's spreadsheet all want
//! different sheets, and none of them will change for us. But a bureau's layout
//! is *their* published specification, and shipping a column set called `datev`
//! that we derived from a blog post would be a compliance claim we cannot stand
//! behind — the loop's rule for legal and compliance work is the strict reading
//! of a cited spec, never a loose guess.
//!
//! So what ships is **country conventions**, which are facts we can state: the
//! headings in the country's own language, the date format its offices read
//! (`31.03.2026` in Germany, `31-03-2026` in the Netherlands, `31/03/2026` in
//! France), the decimal comma every one of them uses, and the semicolon
//! separator that a comma decimal makes necessary. A bureau's own layout is then
//! a *tenant-defined* mapping — the same seed-plus-tenant shape leave policies
//! and holiday calendars have — and until that is built, `alo` is the neutral
//! ISO mapping a bureau's import wizard can be pointed at.
//!
//! # Money and hours
//!
//! Amounts are integer cents everywhere in this suite and are rendered here,
//! once, at the edge — the one place a decimal separator is a per-country
//! decision rather than a bug. Leave minutes are rendered as hours to the
//! hundredth, because that is the unit a payroll bureau reads; the exact figure
//! is the minutes, and [`crate::hr_payroll_export::PayrollLine`] carries them.
//!
//! **Nothing here is translated by our catalogues.** A column heading in a
//! payroll file is a contract with the machine that reads it, not a string a
//! reader's locale chooses — which is why the headings live beside the mapping
//! that owns them rather than in `i18n/en.ts`.

use time::Date;

use crate::error::{Result, StoreError};
use crate::hr_payroll_export::{MINUTES_PER_HOUR, PayrollLine};

/// One fact a payroll file can carry. Closed by construction: a bureau asking
/// for something not on this list is asking for a fact we hold somewhere else,
/// and adding it is a decision made here rather than by a mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayrollColumn {
    /// The tenant's own staff number.
    StaffNumber,
    /// Given (first) name.
    GivenName,
    /// Family (last) name.
    FamilyName,
    /// What the person is called, as the directory shows it.
    FullName,
    /// National identifier / social-security number.
    NationalId,
    /// The account wages are paid into.
    Iban,
    /// ISO 3166-1 alpha-2 country of the home address.
    Country,
    /// Date of birth.
    DateOfBirth,
    /// Job title on the terms the period was drawn from.
    JobTitle,
    /// Team or department.
    Team,
    /// The kind of contract, in its stored word.
    ContractKind,
    /// The day those terms began.
    StartedOn,
    /// The day they ended, blank while they run.
    EndedOn,
    /// Hours in a normal week.
    WeeklyHours,
    /// Gross pay, blank when the tenant records none.
    PayAmount,
    /// What that figure is per: `hour`, `month` or `year`.
    PayPeriod,
    /// ISO 4217 currency of the pay and of every amount on the row.
    PayCurrency,
    /// Paid leave taken in the period, in hours.
    PaidLeaveHours,
    /// Paid sick leave taken in the period, in hours.
    SickLeaveHours,
    /// Unpaid absence in the period, in hours — the one that changes pay.
    UnpaidLeaveHours,
    /// Approved expense claims spent in the period, excluding mileage.
    ExpenseAmount,
    /// Approved mileage allowance for the period.
    MileageAmount,
    /// How many approved claims were left out for being in another currency.
    ClaimsOtherCurrency,
    /// First day of the period — repeated on every row, so a row lifted into
    /// another sheet still says which days it covers.
    PeriodFrom,
    /// Last day of the period.
    PeriodTo,
}

impl PayrollColumn {
    /// Every column this build knows, in the order the neutral mapping writes
    /// them.
    pub const ALL: [Self; 25] = [
        Self::StaffNumber,
        Self::GivenName,
        Self::FamilyName,
        Self::FullName,
        Self::NationalId,
        Self::Iban,
        Self::Country,
        Self::DateOfBirth,
        Self::JobTitle,
        Self::Team,
        Self::ContractKind,
        Self::StartedOn,
        Self::EndedOn,
        Self::WeeklyHours,
        Self::PayAmount,
        Self::PayPeriod,
        Self::PayCurrency,
        Self::PaidLeaveHours,
        Self::SickLeaveHours,
        Self::UnpaidLeaveHours,
        Self::ExpenseAmount,
        Self::MileageAmount,
        Self::ClaimsOtherCurrency,
        Self::PeriodFrom,
        Self::PeriodTo,
    ];

    /// The machine name, so a client can offer a mapping's columns without
    /// parsing its headings.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::StaffNumber => "staffNumber",
            Self::GivenName => "givenName",
            Self::FamilyName => "familyName",
            Self::FullName => "fullName",
            Self::NationalId => "nationalId",
            Self::Iban => "iban",
            Self::Country => "country",
            Self::DateOfBirth => "dateOfBirth",
            Self::JobTitle => "jobTitle",
            Self::Team => "team",
            Self::ContractKind => "contractKind",
            Self::StartedOn => "startedOn",
            Self::EndedOn => "endedOn",
            Self::WeeklyHours => "weeklyHours",
            Self::PayAmount => "payAmount",
            Self::PayPeriod => "payPeriod",
            Self::PayCurrency => "payCurrency",
            Self::PaidLeaveHours => "paidLeaveHours",
            Self::SickLeaveHours => "sickLeaveHours",
            Self::UnpaidLeaveHours => "unpaidLeaveHours",
            Self::ExpenseAmount => "expenseAmount",
            Self::MileageAmount => "mileageAmount",
            Self::ClaimsOtherCurrency => "claimsOtherCurrency",
            Self::PeriodFrom => "periodFrom",
            Self::PeriodTo => "periodTo",
        }
    }

    /// Whether this column carries a person's private data — the fact a screen
    /// needs in order to warn before a download, and the reason the whole file
    /// is behind the HR door.
    #[must_use]
    pub fn is_private(self) -> bool {
        matches!(
            self,
            Self::NationalId | Self::Iban | Self::DateOfBirth | Self::PayAmount
        )
    }

    /// Whether this column carries **text somebody typed** rather than a date,
    /// an amount or a closed vocabulary.
    ///
    /// The caller writing the file uses it to neutralise a leading `=`, `+`, `-`
    /// or `@`, which a spreadsheet would otherwise evaluate as a formula — the
    /// rule `alo-jmap`'s CSV writer leaves to whoever chooses the text, because
    /// neutralising an amount would corrupt a negative number. A staff number
    /// and an IBAN count as typed text: both are strings a person entered, and
    /// neither is arithmetic.
    #[must_use]
    pub fn is_text(self) -> bool {
        matches!(
            self,
            Self::StaffNumber
                | Self::GivenName
                | Self::FamilyName
                | Self::FullName
                | Self::NationalId
                | Self::Iban
                | Self::Country
                | Self::JobTitle
                | Self::Team
        )
    }

    /// This column's value for one line, in `style`.
    #[must_use]
    fn value(self, line: &PayrollLine, style: Style, from: Date, to: Date) -> String {
        match self {
            Self::StaffNumber => line.staff_number.clone(),
            Self::GivenName => line.given_name.clone(),
            Self::FamilyName => line.family_name.clone(),
            Self::FullName => line.full_name.clone(),
            Self::NationalId => line.national_id.clone(),
            Self::Iban => line.iban.clone(),
            Self::Country => line.country.clone(),
            Self::DateOfBirth => line
                .date_of_birth
                .map_or_else(String::new, |born| style.date(born)),
            Self::JobTitle => line.job_title.clone(),
            Self::Team => line.team.clone(),
            Self::ContractKind => line.contract_kind.as_str().to_owned(),
            Self::StartedOn => style.date(line.started_on),
            Self::EndedOn => line
                .ended_on
                .map_or_else(String::new, |last| style.date(last)),
            Self::WeeklyHours => style.hours(i64::from(line.weekly_minutes)),
            Self::PayAmount => line
                .pay_amount_cents
                .map_or_else(String::new, |cents| style.amount(cents)),
            Self::PayPeriod => line.pay_period.as_str().to_owned(),
            Self::PayCurrency => line.pay_currency.clone(),
            Self::PaidLeaveHours => style.hours(line.paid_leave_minutes),
            Self::SickLeaveHours => style.hours(line.sick_leave_minutes),
            Self::UnpaidLeaveHours => style.hours(line.unpaid_leave_minutes),
            Self::ExpenseAmount => style.amount(line.expense_cents),
            Self::MileageAmount => style.amount(line.mileage_cents),
            Self::ClaimsOtherCurrency => line.claims_other_currency.to_string(),
            Self::PeriodFrom => style.date(from),
            Self::PeriodTo => style.date(to),
        }
    }
}

/// How a date is written in a country's offices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateStyle {
    /// `2026-03-31` — ISO 8601, the neutral mapping's, and the one no importer
    /// can misread as a day-month swap.
    Iso,
    /// `31.03.2026` — German-speaking convention.
    Dots,
    /// `31-03-2026` — Dutch convention.
    Dashes,
    /// `31/03/2026` — French, Belgian and Italian convention.
    Slashes,
}

/// Which character separates the whole part of a number from its fraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecimalStyle {
    /// `1234.56`.
    Point,
    /// `1234,56` — every country this module ships a mapping for but the
    /// neutral one.
    Comma,
}

/// The three formatting decisions a mapping makes, together — passed as one so a
/// column can never be rendered under half of a mapping's rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Style {
    date_style: DateStyle,
    decimal_style: DecimalStyle,
}

impl Style {
    /// One date, written the way the mapping's country writes it.
    fn date(self, day: Date) -> String {
        let (y, m, d) = (day.year(), u8::from(day.month()), day.day());
        match self.date_style {
            DateStyle::Iso => format!("{y:04}-{m:02}-{d:02}"),
            DateStyle::Dots => format!("{d:02}.{m:02}.{y:04}"),
            DateStyle::Dashes => format!("{d:02}-{m:02}-{y:04}"),
            DateStyle::Slashes => format!("{d:02}/{m:02}/{y:04}"),
        }
    }

    /// The separator this mapping's country writes a fraction after.
    fn point(self) -> char {
        match self.decimal_style {
            DecimalStyle::Point => '.',
            DecimalStyle::Comma => ',',
        }
    }

    /// Integer cents as a written amount. Integer arithmetic throughout: no
    /// float touches a number somebody is paid, here or anywhere else in this
    /// suite.
    fn amount(self, cents: i64) -> String {
        let sign = if cents < 0 { "-" } else { "" };
        let magnitude = cents.unsigned_abs();
        format!(
            "{sign}{}{}{:02}",
            magnitude / 100,
            self.point(),
            magnitude % 100
        )
    }

    /// Minutes as hours to the hundredth, rounded half-up — the unit a payroll
    /// bureau reads. The exact figure stays the minutes.
    fn hours(self, minutes: i64) -> String {
        let sign = if minutes < 0 { "-" } else { "" };
        let hundredths = minutes
            .unsigned_abs()
            .saturating_mul(100)
            .saturating_add(MINUTES_PER_HOUR.unsigned_abs() / 2)
            / MINUTES_PER_HOUR.unsigned_abs();
        format!(
            "{sign}{}{}{:02}",
            hundredths / 100,
            self.point(),
            hundredths % 100
        )
    }
}

/// One named sheet: the columns a country's payroll offices expect, in their
/// order, under their headings, in their formats.
#[derive(Debug, Clone, Copy)]
pub struct ColumnMapping {
    /// The name a caller asks for it by — lowercase, stable, a contract.
    pub key: &'static str,
    /// What it is called in a picker.
    pub name: &'static str,
    /// The ISO 3166-1 alpha-2 country whose conventions it follows, or `""` for
    /// the neutral one.
    pub country: &'static str,
    /// The character between fields. A comma decimal makes a semicolon
    /// necessary, and every country mapping here uses one.
    pub delimiter: char,
    /// Which date format.
    pub date: DateStyle,
    /// Which decimal separator.
    pub decimal: DecimalStyle,
    /// The columns, in order, each with the heading this sheet gives it.
    pub columns: &'static [(PayrollColumn, &'static str)],
}

impl ColumnMapping {
    /// The headings, in order — the file's first record.
    #[must_use]
    pub fn headings(&self) -> Vec<&'static str> {
        self.columns.iter().map(|(_, heading)| *heading).collect()
    }

    /// One line rendered into this sheet's columns, in order.
    #[must_use]
    pub fn row(&self, line: &PayrollLine, from: Date, to: Date) -> Vec<String> {
        let style = Style {
            date_style: self.date,
            decimal_style: self.decimal,
        };
        self.columns
            .iter()
            .map(|(column, _)| column.value(line, style, from, to))
            .collect()
    }
}

/// Every column, in `PayrollColumn::ALL` order, under machine names — the
/// neutral sheet, and the one a bureau's own import wizard is pointed at.
const NEUTRAL: [(PayrollColumn, &str); 25] = [
    (PayrollColumn::StaffNumber, "staffNumber"),
    (PayrollColumn::GivenName, "givenName"),
    (PayrollColumn::FamilyName, "familyName"),
    (PayrollColumn::FullName, "fullName"),
    (PayrollColumn::NationalId, "nationalId"),
    (PayrollColumn::Iban, "iban"),
    (PayrollColumn::Country, "country"),
    (PayrollColumn::DateOfBirth, "dateOfBirth"),
    (PayrollColumn::JobTitle, "jobTitle"),
    (PayrollColumn::Team, "team"),
    (PayrollColumn::ContractKind, "contractKind"),
    (PayrollColumn::StartedOn, "startedOn"),
    (PayrollColumn::EndedOn, "endedOn"),
    (PayrollColumn::WeeklyHours, "weeklyHours"),
    (PayrollColumn::PayAmount, "payAmount"),
    (PayrollColumn::PayPeriod, "payPeriod"),
    (PayrollColumn::PayCurrency, "payCurrency"),
    (PayrollColumn::PaidLeaveHours, "paidLeaveHours"),
    (PayrollColumn::SickLeaveHours, "sickLeaveHours"),
    (PayrollColumn::UnpaidLeaveHours, "unpaidLeaveHours"),
    (PayrollColumn::ExpenseAmount, "expenseAmount"),
    (PayrollColumn::MileageAmount, "mileageAmount"),
    (PayrollColumn::ClaimsOtherCurrency, "claimsOtherCurrency"),
    (PayrollColumn::PeriodFrom, "periodFrom"),
    (PayrollColumn::PeriodTo, "periodTo"),
];

/// Germany: `Personalnummer`, `Sozialversicherungsnummer`, dots in dates,
/// commas in figures, semicolons between fields.
const DE: [(PayrollColumn, &str); 18] = [
    (PayrollColumn::StaffNumber, "Personalnummer"),
    (PayrollColumn::FamilyName, "Nachname"),
    (PayrollColumn::GivenName, "Vorname"),
    (PayrollColumn::DateOfBirth, "Geburtsdatum"),
    (PayrollColumn::NationalId, "Sozialversicherungsnummer"),
    (PayrollColumn::Iban, "IBAN"),
    (PayrollColumn::ContractKind, "Vertragsart"),
    (PayrollColumn::StartedOn, "Eintrittsdatum"),
    (PayrollColumn::EndedOn, "Austrittsdatum"),
    (PayrollColumn::WeeklyHours, "Wochenstunden"),
    (PayrollColumn::PayAmount, "Bruttoentgelt"),
    (PayrollColumn::PayPeriod, "Entgeltzeitraum"),
    (PayrollColumn::PayCurrency, "Waehrung"),
    (
        PayrollColumn::UnpaidLeaveHours,
        "Unbezahlte Fehlzeit Stunden",
    ),
    (PayrollColumn::SickLeaveHours, "Krankheitsstunden"),
    (PayrollColumn::PaidLeaveHours, "Urlaubsstunden"),
    (PayrollColumn::ExpenseAmount, "Auslagenerstattung"),
    (PayrollColumn::MileageAmount, "Kilometergeld"),
];

/// The Netherlands: `Personeelsnummer`, `BSN`, dashes in dates.
const NL: [(PayrollColumn, &str); 18] = [
    (PayrollColumn::StaffNumber, "Personeelsnummer"),
    (PayrollColumn::FamilyName, "Achternaam"),
    (PayrollColumn::GivenName, "Voornaam"),
    (PayrollColumn::DateOfBirth, "Geboortedatum"),
    (PayrollColumn::NationalId, "BSN"),
    (PayrollColumn::Iban, "IBAN"),
    (PayrollColumn::ContractKind, "Contractsoort"),
    (PayrollColumn::StartedOn, "Datum in dienst"),
    (PayrollColumn::EndedOn, "Datum uit dienst"),
    (PayrollColumn::WeeklyHours, "Uren per week"),
    (PayrollColumn::PayAmount, "Brutoloon"),
    (PayrollColumn::PayPeriod, "Loonperiode"),
    (PayrollColumn::PayCurrency, "Valuta"),
    (PayrollColumn::UnpaidLeaveHours, "Onbetaald verlof uren"),
    (PayrollColumn::SickLeaveHours, "Ziekte uren"),
    (PayrollColumn::PaidLeaveHours, "Verlof uren"),
    (PayrollColumn::ExpenseAmount, "Onkostenvergoeding"),
    (PayrollColumn::MileageAmount, "Kilometervergoeding"),
];

/// France and Belgium's French-speaking bureaux: `Matricule`, slashes in dates.
const FR: [(PayrollColumn, &str); 18] = [
    (PayrollColumn::StaffNumber, "Matricule"),
    (PayrollColumn::FamilyName, "Nom"),
    (PayrollColumn::GivenName, "Prenom"),
    (PayrollColumn::DateOfBirth, "Date de naissance"),
    (PayrollColumn::NationalId, "Numero de securite sociale"),
    (PayrollColumn::Iban, "IBAN"),
    (PayrollColumn::ContractKind, "Type de contrat"),
    (PayrollColumn::StartedOn, "Date d'entree"),
    (PayrollColumn::EndedOn, "Date de sortie"),
    (PayrollColumn::WeeklyHours, "Heures hebdomadaires"),
    (PayrollColumn::PayAmount, "Salaire brut"),
    (PayrollColumn::PayPeriod, "Periodicite"),
    (PayrollColumn::PayCurrency, "Devise"),
    (PayrollColumn::UnpaidLeaveHours, "Heures conge sans solde"),
    (PayrollColumn::SickLeaveHours, "Heures maladie"),
    (PayrollColumn::PaidLeaveHours, "Heures conges payes"),
    (PayrollColumn::ExpenseAmount, "Remboursement de frais"),
    (PayrollColumn::MileageAmount, "Indemnite kilometrique"),
];

/// The mappings this build ships. Seed data: a tenant-defined mapping is the
/// same shape and is not built yet (see the module docs).
pub const MAPPINGS: [ColumnMapping; 4] = [
    ColumnMapping {
        key: "alo",
        name: "alo (neutral, ISO)",
        country: "",
        delimiter: ',',
        date: DateStyle::Iso,
        decimal: DecimalStyle::Point,
        columns: &NEUTRAL,
    },
    ColumnMapping {
        key: "de",
        name: "Deutschland",
        country: "DE",
        delimiter: ';',
        date: DateStyle::Dots,
        decimal: DecimalStyle::Comma,
        columns: &DE,
    },
    ColumnMapping {
        key: "nl",
        name: "Nederland",
        country: "NL",
        delimiter: ';',
        date: DateStyle::Dashes,
        decimal: DecimalStyle::Comma,
        columns: &NL,
    },
    ColumnMapping {
        key: "fr",
        name: "France / Belgique",
        country: "FR",
        delimiter: ';',
        date: DateStyle::Slashes,
        decimal: DecimalStyle::Comma,
        columns: &FR,
    },
];

/// The mapping `key` names, matched without regard to case.
///
/// # Errors
/// [`StoreError::Validation`] naming every mapping there is — a caller who
/// guessed wrong is told what to ask for, rather than being given a file in a
/// layout they did not choose.
pub fn mapping(key: &str) -> Result<&'static ColumnMapping> {
    let wanted = key.trim().to_ascii_lowercase();
    MAPPINGS
        .iter()
        .find(|mapping| mapping.key == wanted)
        .ok_or_else(|| {
            StoreError::Validation(format!(
                "column mapping must be one of: {}",
                MAPPINGS
                    .iter()
                    .map(|mapping| mapping.key)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
}

/// The mapping a tenant in `country` gets when they state none — their own
/// country's, and the neutral one for a country we ship no conventions for.
#[must_use]
pub fn mapping_for_country(country: &str) -> &'static ColumnMapping {
    let wanted = country.trim().to_ascii_uppercase();
    MAPPINGS
        .iter()
        .find(|mapping| !mapping.country.is_empty() && mapping.country == wanted)
        .unwrap_or(&MAPPINGS[0])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::hr_employments::{ContractKind, PayPeriod};
    use crate::id::HrEmployeeId;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("a real date")
    }

    fn line() -> PayrollLine {
        PayrollLine {
            employee_id: HrEmployeeId::new("e".to_owned()),
            staff_number: "0042".to_owned(),
            given_name: "Ada".to_owned(),
            family_name: "Byron".to_owned(),
            full_name: "Ada Byron".to_owned(),
            national_id: "123456782".to_owned(),
            iban: "NL91ABNA0417164300".to_owned(),
            country: "NL".to_owned(),
            date_of_birth: Some(day(1990, Month::December, 10)),
            job_title: "Systeembeheerder".to_owned(),
            team: "Techniek".to_owned(),
            contract_kind: ContractKind::PartTime,
            started_on: day(2024, Month::March, 4),
            ended_on: None,
            weekly_minutes: 1_920,
            pay_amount_cents: Some(320_050),
            pay_period: PayPeriod::Month,
            pay_currency: "EUR".to_owned(),
            paid_leave_minutes: 960,
            sick_leave_minutes: 25,
            unpaid_leave_minutes: 480,
            expense_cents: 4_500,
            mileage_cents: 3_750,
            claims_other_currency: 2,
        }
    }

    fn rendered(key: &str) -> Vec<String> {
        let mapping = mapping(key).unwrap_or_else(|error| panic!("{error}"));
        mapping.row(
            &line(),
            day(2026, Month::March, 1),
            day(2026, Month::March, 31),
        )
    }

    fn value_of(key: &str, column: PayrollColumn) -> String {
        let mapping = mapping(key).unwrap_or_else(|error| panic!("{error}"));
        let at = mapping
            .columns
            .iter()
            .position(|(candidate, _)| *candidate == column)
            .unwrap_or_else(|| panic!("{key} has no {}", column.key()));
        rendered(key)
            .get(at)
            .cloned()
            .unwrap_or_else(|| panic!("no value at {at}"))
    }

    #[test]
    fn every_mapping_writes_one_value_per_heading_and_no_unknown_column() {
        for mapping in &MAPPINGS {
            let row = mapping.row(
                &line(),
                day(2026, Month::March, 1),
                day(2026, Month::March, 31),
            );
            assert_eq!(
                row.len(),
                mapping.headings().len(),
                "{}: a row and its headings are one list",
                mapping.key
            );
            for (column, heading) in mapping.columns {
                assert!(
                    !heading.is_empty(),
                    "{}: {} is unheaded",
                    mapping.key,
                    column.key()
                );
                assert!(
                    PayrollColumn::ALL.contains(column),
                    "{}: {} is not a column this build knows",
                    mapping.key,
                    column.key()
                );
            }
            // A comma decimal in a comma-separated file is a bug waiting for a
            // German tenant: the two decisions travel together.
            if mapping.decimal == DecimalStyle::Comma {
                assert_eq!(mapping.delimiter, ';', "{}", mapping.key);
            }
        }
    }

    #[test]
    fn the_neutral_mapping_carries_every_fact_the_fold_produces() {
        let mapping = mapping("alo").unwrap_or_else(|error| panic!("{error}"));
        for column in PayrollColumn::ALL {
            assert!(
                mapping.columns.iter().any(|(c, _)| *c == column),
                "the neutral sheet drops {}",
                column.key()
            );
        }
        assert_eq!(mapping.headings()[0], "staffNumber");
    }

    #[test]
    fn a_country_writes_its_own_dates_and_its_own_decimal_comma() {
        assert_eq!(value_of("alo", PayrollColumn::StartedOn), "2024-03-04");
        assert_eq!(value_of("de", PayrollColumn::StartedOn), "04.03.2024");
        assert_eq!(value_of("nl", PayrollColumn::StartedOn), "04-03-2024");
        assert_eq!(value_of("fr", PayrollColumn::StartedOn), "04/03/2024");
        assert_eq!(value_of("alo", PayrollColumn::PayAmount), "3200.50");
        assert_eq!(value_of("de", PayrollColumn::PayAmount), "3200,50");
        // The same facts under every mapping — a mapping formats, it never
        // decides.
        for key in ["alo", "de", "nl", "fr"] {
            assert_eq!(value_of(key, PayrollColumn::StaffNumber), "0042");
            assert_eq!(
                value_of(key, PayrollColumn::UnpaidLeaveHours).replace(',', "."),
                "8.00"
            );
        }
    }

    #[test]
    fn hours_are_minutes_to_the_hundredth_and_money_is_never_a_float() {
        let style = Style {
            date_style: DateStyle::Iso,
            decimal_style: DecimalStyle::Point,
        };
        assert_eq!(style.hours(0), "0.00");
        assert_eq!(style.hours(30), "0.50");
        assert_eq!(style.hours(480), "8.00");
        assert_eq!(style.hours(1_920), "32.00");
        // 25 minutes is 0.4166…; the file carries the hundredth, and the exact
        // figure stays the minutes on the line.
        assert_eq!(style.hours(25), "0.42");
        assert_eq!(style.hours(-30), "-0.50");
        // An absurd figure saturates rather than panicking; it is not a number
        // anybody is paid, and the file would rather be wrong loudly than gone.
        assert!(style.hours(i64::MAX).ends_with(".60"));
        assert_eq!(style.amount(0), "0.00");
        assert_eq!(style.amount(5), "0.05");
        assert_eq!(style.amount(-102_997), "-1029.97");
        assert_eq!(style.amount(i64::MIN), "-92233720368547758.08");
    }

    #[test]
    fn an_unrecorded_fact_is_blank_never_a_zero_or_a_guess() {
        let mut nothing = line();
        nothing.pay_amount_cents = None;
        nothing.date_of_birth = None;
        nothing.staff_number = String::new();
        let alo = mapping("alo").unwrap_or_else(|error| panic!("{error}"));
        let row = alo.row(
            &nothing,
            day(2026, Month::March, 1),
            day(2026, Month::March, 31),
        );
        let at = |column: PayrollColumn| {
            alo.columns
                .iter()
                .position(|(candidate, _)| *candidate == column)
                .and_then(|at| row.get(at).cloned())
                .unwrap_or_else(|| panic!("no {}", column.key()))
        };
        assert_eq!(
            at(PayrollColumn::PayAmount),
            "",
            "an unpaid intern is not 0.00"
        );
        assert_eq!(at(PayrollColumn::DateOfBirth), "");
        assert_eq!(at(PayrollColumn::EndedOn), "", "still employed");
        assert_eq!(at(PayrollColumn::StaffNumber), "");
        // A claim total of nothing IS zero: the company owes nothing, which is a
        // figure rather than a gap.
        assert_eq!(at(PayrollColumn::ExpenseAmount), "45.00");
    }

    #[test]
    fn a_mapping_is_asked_for_by_name_and_a_wrong_name_is_told_the_names() {
        assert_eq!(mapping("DE").map(|m| m.key).unwrap_or(""), "de");
        assert_eq!(mapping("  nl ").map(|m| m.key).unwrap_or(""), "nl");
        let message = match mapping("datev") {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected Validation, got {other:?}"),
        };
        for key in ["alo", "de", "nl", "fr"] {
            assert!(message.contains(key), "{message}");
        }
        assert_eq!(mapping_for_country("NL").key, "nl");
        assert_eq!(mapping_for_country("de").key, "de");
        assert_eq!(
            mapping_for_country("PL").key,
            "alo",
            "a country we ship no conventions for gets the neutral sheet"
        );
        assert_eq!(mapping_for_country("").key, "alo");
    }

    #[test]
    fn the_typed_columns_are_named_so_the_writer_can_neutralise_a_formula() {
        for column in [
            PayrollColumn::FullName,
            PayrollColumn::JobTitle,
            PayrollColumn::StaffNumber,
            PayrollColumn::Iban,
        ] {
            assert!(column.is_text(), "{}", column.key());
        }
        // An amount and a date are never neutralised: a negative amount begins
        // with `-` and must stay a number.
        for column in [
            PayrollColumn::PayAmount,
            PayrollColumn::ExpenseAmount,
            PayrollColumn::StartedOn,
            PayrollColumn::PaidLeaveHours,
            PayrollColumn::ClaimsOtherCurrency,
        ] {
            assert!(!column.is_text(), "{}", column.key());
        }
    }

    #[test]
    fn the_private_columns_are_named_so_a_screen_can_warn_before_the_download() {
        for column in [
            PayrollColumn::NationalId,
            PayrollColumn::Iban,
            PayrollColumn::DateOfBirth,
            PayrollColumn::PayAmount,
        ] {
            assert!(column.is_private(), "{}", column.key());
        }
        for column in [
            PayrollColumn::StaffNumber,
            PayrollColumn::Team,
            PayrollColumn::PeriodFrom,
        ] {
            assert!(!column.is_private(), "{}", column.key());
        }
    }
}
