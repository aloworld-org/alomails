//! The rules a tenant teaches the reconciliation screen (alo Finance, ADR 0035,
//! wave B4.09b; `docs/design/finance.md`, "The bank and reconciliation").
//!
//! # "This payer is that customer", said once
//!
//! A bank writes a payer's name the way the payer's own bank wrote it —
//! `MUELLER BAU`, `M. BAU GBR`, `SEPA-UEBERWEISUNG MUELLERBAU` — and none of
//! those is what the tenant typed into the customer record. No amount of
//! cleverness in [`crate::bank_match_heuristic`] recovers that reliably, and
//! cleverness that guesses wrong marks an invoice paid that is not.
//!
//! So the tenant says it, once: *money whose counterparty contains "mueller
//! bau" is Müller Bau GmbH's*. That is a [`MatchRule`], and from then on their
//! documents are ranked for such a line with the reason shown — "the rule you
//! saved for this counterparty".
//!
//! # What a rule is, and what it deliberately is not
//!
//! - It is **plain folded text** looked for in one named field
//!   ([`MatchOn`]) — no globs, no regular expressions. A rule a bookkeeper
//!   cannot read back is a rule they cannot trust, and a regular expression a
//!   tenant can write is a denial of service they can write.
//! - It **proposes, never books**. A rule that fires adds points
//!   ([`MatchEvidence::RuleSaved`](crate::bank_match_heuristic::MatchEvidence::RuleSaved))
//!   and nothing else; a person still confirms, which is what creates the
//!   payment (ADR 0023, and here a money rule).
//! - Its **hits are for the person, not for the ranking**
//!   ([`AccountStore::fin_match_rule_hit`]). The count says how often the rule
//!   turned into a confirmed match, so a rules screen can show which ones earn
//!   their place. It never changes what the rule scores: a heuristic that
//!   quietly re-weights itself is one nobody can predict.
//! - It is **written by a person and deleted by a person**. Nothing in alo
//!   creates a rule on its own; [`AccountStore::learn_fin_match_rule`] exists so
//!   that saying "remember this payer" on a line a person just reconciled does
//!   not mean re-typing what the bank already stated.
//!
//! # Tenancy
//!
//! Every statement binds `tenant_id = self.tenant`, and the customer a rule
//! names is resolved through [`AccountStore::billing_customer`] — so a rule can
//! only ever point at a customer of the tenant that wrote it, and another
//! tenant's rule id is a [`StoreError::NotFound`] exactly as their customer id
//! is. A pattern is somebody's payer's name: it is never logged.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::bank_import::BankLine;
use crate::bank_match_heuristic::{folded, folded_iban};
use crate::error::{Result, StoreError};
use crate::iban::canonicalize;
use crate::id::{BankLineId, BillingCustomerId, FinMatchRuleId, UserId};

/// The shortest pattern that says anything. Two characters would match half the
/// statements in the file, and a rule that fires on everything is worse than no
/// rule: it puts a wrong customer's documents at the top of every line.
pub const RULE_PATTERN_MIN: usize = 3;

/// The longest pattern kept — a company name with its town, not a remittance.
pub const RULE_PATTERN_MAX: usize = 120;

/// The most rules one tenant may hold.
///
/// Every rule is read for every suggestion pass, so this is the bound on that
/// work as much as it is a sanity limit. Five hundred payers a company
/// recognises by sight is far past the point where the rules screen is the
/// problem to solve.
pub const RULES_MAX: usize = 500;

/// The kind of document a rule points at. Only invoices today; `bill` is the
/// kind a supplier rule takes when B5 lands.
const RULE_TARGET_INVOICE: &str = "invoice";

/// The columns every read of a rule selects, in [`RuleRow`] order.
const RULE_COLS: &str = "id, match_on, pattern, customer_id, hits, created_by, created_at";

/// Which field of a staged bank line a rule reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchOn {
    /// The name the bank put on the line. The usual rule: a payer whose bank
    /// spells them differently from the customer record.
    Counterparty,
    /// What the payer wrote. For the customer whose reference is always their
    /// own order number and never ours.
    Remittance,
    /// The account the money came from, compared whole. The strongest of the
    /// three, because an IBAN is not a spelling.
    Iban,
}

impl MatchOn {
    /// The stored word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Counterparty => "counterparty",
            Self::Remittance => "remittance",
            Self::Iban => "iban",
        }
    }

    /// The field a stored word names, or `None` when it is not one this build
    /// knows.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "counterparty" => Some(Self::Counterparty),
            "remittance" => Some(Self::Remittance),
            "iban" => Some(Self::Iban),
            _ => None,
        }
    }
}

/// A rule as a person states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMatchRule {
    /// Which field to read.
    pub match_on: MatchOn,
    /// What to look for in it, in the person's own spelling — folded before it
    /// is stored ([`NewMatchRule::normalized_pattern`]).
    pub pattern: String,
    /// Whose documents this identifies.
    pub customer_id: BillingCustomerId,
}

impl NewMatchRule {
    /// The pattern as it will be stored and compared: folded, and for an IBAN
    /// canonicalised first so that a rule cannot be saved for an account number
    /// that cannot exist.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the folded pattern is shorter than
    /// [`RULE_PATTERN_MIN`] or longer than [`RULE_PATTERN_MAX`], or when an IBAN
    /// rule's pattern is not an IBAN. The message never echoes the pattern: it
    /// is a payer's name (Law 1).
    pub fn normalized_pattern(&self) -> Result<String> {
        let pattern = match self.match_on {
            MatchOn::Iban => {
                let iban = canonicalize(&self.pattern).map_err(|_| {
                    StoreError::Validation(
                        "that is not an IBAN; check the account number and try again".to_owned(),
                    )
                })?;
                let iban = iban.ok_or_else(|| {
                    StoreError::Validation(
                        "an IBAN rule needs an account number to look for".to_owned(),
                    )
                })?;
                folded_iban(&iban)
            }
            MatchOn::Counterparty | MatchOn::Remittance => folded(&self.pattern),
        };
        let length = pattern.chars().count();
        if length < RULE_PATTERN_MIN {
            return Err(StoreError::Validation(format!(
                "a rule needs at least {RULE_PATTERN_MIN} characters to look for; a shorter one \
                 would match almost every line on the statement"
            )));
        }
        if length > RULE_PATTERN_MAX {
            return Err(StoreError::Validation(format!(
                "a rule looks for at most {RULE_PATTERN_MAX} characters"
            )));
        }
        Ok(pattern)
    }
}

/// One stored rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchRule {
    /// Opaque id, unique within the tenant.
    pub id: FinMatchRuleId,
    /// Which field it reads.
    pub match_on: MatchOn,
    /// What it looks for, folded as it is compared.
    pub pattern: String,
    /// Whose documents it identifies.
    pub customer_id: BillingCustomerId,
    /// How many confirmed matches it has proposed.
    pub hits: i32,
    /// Who saved it.
    pub created_by: UserId,
    /// When.
    pub created_at: OffsetDateTime,
}

impl MatchRule {
    /// Whether this rule fires on `line`.
    ///
    /// The line's field is folded the same way the pattern was when it was
    /// stored ([`crate::bank_match_heuristic::folded`]), so the comparison is
    /// between two strings normalised by one function — which is the only reason
    /// a plain `contains` is safe here.
    ///
    /// An IBAN is compared **whole**: half an account number is not a weaker
    /// match, it is a different account.
    #[must_use]
    pub fn fires_on(&self, line: &BankLine) -> bool {
        match self.match_on {
            MatchOn::Counterparty => folded(&line.counterparty_name).contains(&self.pattern),
            MatchOn::Remittance => folded(&line.remittance).contains(&self.pattern),
            MatchOn::Iban => {
                let iban = folded_iban(&line.counterparty_iban);
                !iban.is_empty() && iban == self.pattern
            }
        }
    }
}

impl AccountStore {
    /// The tenant's rules, the most productive first.
    ///
    /// Ordered by hits so the rules screen leads with the ones that earn their
    /// place, and by pattern under that so the list is stable between reads.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_match_rules(&self) -> Result<Vec<MatchRule>> {
        let rows = sqlx::query_as::<_, RuleRow>(&format!(
            "SELECT {RULE_COLS} FROM fin_match_rules \
             WHERE tenant_id = $1 AND target_kind = $2 \
             ORDER BY hits DESC, pattern, id LIMIT $3"
        ))
        .bind(self.tenant.as_str())
        .bind(RULE_TARGET_INVOICE)
        .bind(RULES_MAX as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(RuleRow::into_rule).collect()
    }

    /// Saves a rule.
    ///
    /// The pattern is validated and folded before anything is written
    /// ([`NewMatchRule::normalized_pattern`]), and the customer is resolved
    /// through this tenant's own door — so a guessed id from another tenant is a
    /// [`StoreError::NotFound`], indistinguishable from one that never existed.
    ///
    /// An **archived** customer is allowed on purpose: archiving means "we no
    /// longer do business with them", and their old invoices still have to be
    /// reconciled when the money finally arrives.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the pattern breaks its rule or the tenant
    /// already holds [`RULES_MAX`] rules; [`StoreError::NotFound`] when the
    /// customer is not this tenant's; [`StoreError::Conflict`] when a rule
    /// already looks for the same thing in the same field; [`StoreError::Db`] on
    /// failure.
    pub async fn create_fin_match_rule(&self, new: &NewMatchRule) -> Result<MatchRule> {
        let pattern = new.normalized_pattern()?;
        self.billing_customer(&new.customer_id)
            .await?
            .ok_or(StoreError::NotFound)?;

        let held: i64 =
            sqlx::query_scalar("SELECT count(*) FROM fin_match_rules WHERE tenant_id = $1")
                .bind(self.tenant.as_str())
                .fetch_one(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        if held >= RULES_MAX as i64 {
            return Err(StoreError::Validation(format!(
                "a tenant may hold at most {RULES_MAX} matching rules; delete one you no longer \
                 recognise before saving another"
            )));
        }

        let id = FinMatchRuleId::generate();
        let created_at: Option<OffsetDateTime> = sqlx::query_scalar(
            "INSERT INTO fin_match_rules \
                 (tenant_id, id, match_on, pattern, target_kind, customer_id, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (tenant_id, match_on, pattern) DO NOTHING \
             RETURNING created_at",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(new.match_on.as_str())
        .bind(&pattern)
        .bind(RULE_TARGET_INVOICE)
        .bind(new.customer_id.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        // The conflict is answered rather than raised: two people saving the
        // same rule from two screens is an ordinary race, and the second one
        // deserves a sentence, not a database error.
        let created_at = created_at.ok_or_else(|| {
            StoreError::Conflict(
                "a rule already looks for that in the same field; delete it first if it should \
                 point somewhere else"
                    .to_owned(),
            )
        })?;
        Ok(MatchRule {
            id,
            match_on: new.match_on,
            pattern,
            customer_id: new.customer_id.clone(),
            hits: 0,
            created_by: self.user.clone(),
            created_at,
        })
    }

    /// Saves a rule **from a line the person is looking at**: "remember that
    /// this payer is that customer".
    ///
    /// The pattern is taken from the line itself, which is the whole point —
    /// re-typing a name the bank already stated is how a rule ends up with a
    /// typo in it and never fires again.
    ///
    /// The **remittance** is refused here: what a payer wrote on one transfer
    /// contains that transfer's own reference, so it would never match a second
    /// time. A remittance rule is a fragment a person chooses, and it goes
    /// through [`AccountStore::create_fin_match_rule`].
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the line or the customer is not this
    /// tenant's; [`StoreError::Validation`] when the bank stated nothing in that
    /// field, when the field is the remittance, or when the pattern breaks its
    /// rule; [`StoreError::Conflict`] when such a rule already exists;
    /// [`StoreError::Db`] on failure.
    pub async fn learn_fin_match_rule(
        &self,
        line_id: &BankLineId,
        match_on: MatchOn,
        customer_id: &BillingCustomerId,
    ) -> Result<MatchRule> {
        let line = self.bank_line(line_id).await?.ok_or(StoreError::NotFound)?;
        let pattern = match match_on {
            MatchOn::Counterparty => line.counterparty_name.clone(),
            MatchOn::Iban => line.counterparty_iban.clone(),
            MatchOn::Remittance => {
                return Err(StoreError::Validation(
                    "what the payer wrote on this transfer names this transfer; type the part of \
                     it that will be there next time instead"
                        .to_owned(),
                ));
            }
        };
        if pattern.trim().is_empty() {
            return Err(StoreError::Validation(
                "the bank stated nothing in that field on this line, so there is nothing to \
                 remember"
                    .to_owned(),
            ));
        }
        self.create_fin_match_rule(&NewMatchRule {
            match_on,
            pattern,
            customer_id: customer_id.clone(),
        })
        .await
    }

    /// Counts a confirmed match against the rule that proposed it.
    ///
    /// Called by the confirming path when the person took a suggestion a rule
    /// raised — never by a read, because a counter that moves every time a
    /// screen refreshes measures the screen and not the rule.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the rule is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn fin_match_rule_hit(&self, id: &FinMatchRuleId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.fin_match_rule_hit_in(&mut tx, id).await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// [`AccountStore::fin_match_rule_hit`], inside a transaction the caller
    /// owns — the form the settling path uses ([`crate::bank_reconcile`]).
    ///
    /// A hit counted outside the transaction that earned it could survive a
    /// settlement that rolled back, and it is the same statement either way, so
    /// the counter and the money move together. It doubles as the **ownership
    /// check** on a rule id a client sent: another tenant's rule updates no row
    /// and is a [`StoreError::NotFound`], indistinguishable from one that never
    /// existed.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the rule is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn fin_match_rule_hit_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &FinMatchRuleId,
    ) -> Result<()> {
        let moved = sqlx::query(
            "UPDATE fin_match_rules SET hits = hits + 1 WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        if moved.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Forgets a rule.
    ///
    /// There is no edit: a rule is a pattern and a customer, and correcting one
    /// is deleting it and saying the right thing. Matches it already proposed
    /// are untouched — they are money somebody confirmed, and `rule_id` on them
    /// is history rather than a link that has to resolve.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the rule is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_fin_match_rule(&self, id: &FinMatchRuleId) -> Result<()> {
        let removed = sqlx::query("DELETE FROM fin_match_rules WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        if removed.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

/// One row of `fin_match_rules`, in [`RULE_COLS`] order.
#[derive(sqlx::FromRow)]
struct RuleRow {
    id: String,
    match_on: String,
    pattern: String,
    customer_id: Option<String>,
    hits: i32,
    created_by: String,
    created_at: OffsetDateTime,
}

impl RuleRow {
    /// The stored rule.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the row names a field or a customer this
    /// build cannot read — a decode failure rather than a guess, because a rule
    /// read wrongly proposes the wrong customer's documents.
    fn into_rule(self) -> Result<MatchRule> {
        let match_on = MatchOn::parse(&self.match_on).ok_or_else(|| {
            StoreError::Validation(
                "this rule reads a field of a bank line this version does not know".to_owned(),
            )
        })?;
        let customer_id = self
            .customer_id
            .ok_or_else(|| StoreError::Validation("this rule points at no customer".to_owned()))?;
        Ok(MatchRule {
            id: FinMatchRuleId::new(self.id),
            match_on,
            pattern: self.pattern,
            customer_id: BillingCustomerId::new(customer_id),
            hits: self.hits,
            created_by: UserId::new(self.created_by),
            created_at: self.created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::bank_import::BankLineStatus;
    use crate::id::BankStatementId;
    use time::{Date, Month};

    fn line(counterparty: &str, iban: &str, remittance: &str) -> BankLine {
        BankLine {
            id: BankLineId::new("line-1".to_owned()),
            statement_id: BankStatementId::new("stmt-1".to_owned()),
            line_no: 1,
            booked_on: Date::from_calendar_date(2026, Month::February, 10).unwrap_or(Date::MIN),
            value_on: Date::from_calendar_date(2026, Month::February, 10).unwrap_or(Date::MIN),
            amount_cents: 50_000,
            currency: "EUR".to_owned(),
            counterparty_name: counterparty.to_owned(),
            counterparty_iban: iban.to_owned(),
            remittance: remittance.to_owned(),
            bank_ref: "REF9".to_owned(),
            status: BankLineStatus::Unmatched,
            ignored_reason: String::new(),
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn stated(match_on: MatchOn, pattern: &str) -> NewMatchRule {
        NewMatchRule {
            match_on,
            pattern: pattern.to_owned(),
            customer_id: BillingCustomerId::new("cust-1".to_owned()),
        }
    }

    fn saved(match_on: MatchOn, pattern: &str) -> MatchRule {
        MatchRule {
            id: FinMatchRuleId::new("rule-1".to_owned()),
            match_on,
            pattern: stated(match_on, pattern)
                .normalized_pattern()
                .expect("a valid pattern"),
            customer_id: BillingCustomerId::new("cust-1".to_owned()),
            hits: 0,
            created_by: UserId::new("u-1".to_owned()),
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn refused(result: Result<String>) -> String {
        match result {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pattern_is_stored_folded_so_it_is_compared_the_way_a_line_is() {
        assert_eq!(
            stated(MatchOn::Counterparty, "  Müller   Bau GmbH ")
                .normalized_pattern()
                .expect("valid"),
            "muller bau gmbh"
        );
        assert_eq!(
            stated(MatchOn::Remittance, "Bestellung 4711/A")
                .normalized_pattern()
                .expect("valid"),
            "bestellung 4711 a"
        );
    }

    #[test]
    fn an_iban_rule_is_an_iban_or_it_is_not_a_rule() {
        assert_eq!(
            stated(MatchOn::Iban, "DE02 1203 0000 0000 2020 51")
                .normalized_pattern()
                .expect("valid"),
            "de02120300000000202051",
            "spacing is the bank's, not the account's"
        );
        // A checksum that does not hold is a typo, and a rule with a typo in it
        // never fires and nobody can see why.
        let message =
            refused(stated(MatchOn::Iban, "DE02 1203 0000 0000 2020 52").normalized_pattern());
        assert!(message.contains("not an IBAN"), "{message}");
        let message = refused(stated(MatchOn::Iban, "   ").normalized_pattern());
        assert!(message.contains("needs an account number"), "{message}");
    }

    #[test]
    fn a_pattern_too_short_to_mean_anything_is_refused() {
        for hopeless in ["a", "ab", " x ", "-- --"] {
            let message = refused(stated(MatchOn::Counterparty, hopeless).normalized_pattern());
            assert!(message.contains("at least"), "{hopeless:?}: {message}");
        }
        assert!(
            stated(MatchOn::Counterparty, "abc")
                .normalized_pattern()
                .is_ok(),
            "exactly the minimum is a rule"
        );
        let message = refused(
            stated(MatchOn::Remittance, &"x".repeat(RULE_PATTERN_MAX + 1)).normalized_pattern(),
        );
        assert!(message.contains("at most"), "{message}");
    }

    #[test]
    fn no_refusal_repeats_the_pattern_it_refused() {
        // A pattern is a payer's name (Law 1).
        let secret = "Kaffeehaus Bergmann";
        for message in [
            refused(stated(MatchOn::Iban, secret).normalized_pattern()),
            refused(stated(MatchOn::Counterparty, "x").normalized_pattern()),
        ] {
            assert!(!message.contains("Kaffeehaus"), "{message}");
        }
    }

    #[test]
    fn a_counterparty_rule_reads_the_name_the_bank_wrote_however_it_wrote_it() {
        let rule = saved(MatchOn::Counterparty, "Müller Bau");
        assert!(rule.fires_on(&line("SEPA MÜLLER BAU GMBH", "", "")));
        assert!(rule.fires_on(&line("müller bau", "", "")));
        assert!(
            rule.fires_on(&line("MÜLLER-BAU", "", "")),
            "the separator is the bank's"
        );
        assert!(!rule.fires_on(&line("Müller Handel", "", "")));
        assert!(
            !rule.fires_on(&line("MUELLER BAU", "", "")),
            "a rule is the text it says, spelled the way it was saved"
        );
        assert!(
            !rule.fires_on(&line("", "", "Müller Bau")),
            "it reads one field"
        );
    }

    #[test]
    fn a_wrongly_spelled_payer_is_exactly_what_a_rule_is_for() {
        // The transliteration `folded` deliberately does not undo.
        let rule = saved(MatchOn::Counterparty, "MUELLER BAU");
        assert!(rule.fires_on(&line("MUELLER BAU GMBH BERLIN", "", "")));
    }

    #[test]
    fn an_iban_rule_matches_the_whole_account_and_never_half_of_one() {
        let rule = saved(MatchOn::Iban, "DE02 1203 0000 0000 2020 51");
        assert!(rule.fires_on(&line("", "DE02120300000000202051", "")));
        assert!(rule.fires_on(&line("", "de02 1203 0000 0000 2020 51", "")));
        assert!(!rule.fires_on(&line("", "DE02 1203 0000 0000 2020", "")));
        assert!(
            !rule.fires_on(&line("", "", "")),
            "a line with no IBAN matches nothing"
        );
    }

    #[test]
    fn a_remittance_rule_looks_inside_what_the_payer_wrote() {
        let rule = saved(MatchOn::Remittance, "bestellung 4711");
        assert!(rule.fires_on(&line("", "", "Zahlung Bestellung 4711/2 danke")));
        assert!(!rule.fires_on(&line("", "", "Bestellung 4712")));
    }

    #[test]
    fn the_stored_words_round_trip() {
        for field in [MatchOn::Counterparty, MatchOn::Remittance, MatchOn::Iban] {
            assert_eq!(MatchOn::parse(field.as_str()), Some(field));
        }
        // A field from a newer build is not guessed at.
        assert_eq!(MatchOn::parse("bic"), None);
        assert_eq!(MatchOn::parse(""), None);
    }
}
