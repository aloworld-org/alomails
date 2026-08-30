//! The executors of alo Finance's verbs (ADR 0058) — what runs when the
//! Finance agent uses one of the intents `alo_ai::finance_intents` describes.
//!
//! Every executor runs through the asker's account door **and through the same
//! gate as the finance screens**: all of these read the whole tenant's books
//! rather than the caller's own claims, so every one calls
//! [`crate::state::Account::require_finance`] — an admin or the accountant,
//! and nobody else. An agent is exactly the way around that wall somebody
//! would try, and it is gated like every other caller.
//!
//! The figures are the store's own, through the same functions the screens
//! read — [`alo_store::AccountStore::fin_account_ledger`] for what was
//! invoiced, paid and is outstanding, [`alo_store::AccountStore::fin_trial_balance`]
//! for an account's balance (the fold `GET /finance/accounts?from&to` uses),
//! the approvals inbox's own [`crate::finance_approvals::pending_json`] and
//! the bank page's [`crate::finance_bank::line_json`] — so an agent grounds in
//! exactly what a person sees, with money made readable beside its integers
//! ([`crate::billing_intents::ok`], the shared rendering). A write only ever
//! runs from the asker's approval ([`crate::agent::execute_tool`] holds that).
//!
//! The kept executors stay in their own files and are reached only from the
//! dispatch below: [`crate::agent_finance`] (the categorise write, and the
//! approve write beside it), [`crate::agent_finance_answers`] (the VAT figures
//! and the journal scan).

use serde_json::{Value, json};
use time::{Date, Month};

use alo_store::{Account as ChartAccount, AccountRole, BankLineStatus, EntryKind, LedgerLine};

use crate::agent_args::{string_arg, unprocessable};
use crate::billing::{iso_date, map_store_err, parse_iso_date};
use crate::billing_document::today;
use crate::billing_intents::{Reply, ok};
use crate::error::Problem;
use crate::state::{Account, AppState};

/// How many records a list read returns — enough for a question, small enough
/// to sit inside the turn's result window.
const MAX_LISTED: usize = 12;

/// The day one end of a period is read from, or the default the verb states.
pub(crate) fn period_day(args: &Value, key: &str, default: Date) -> Result<Date, Problem> {
    match string_arg(args, key).filter(|raw| !raw.trim().is_empty()) {
        None => Ok(default),
        Some(raw) => parse_iso_date(&raw)
            .ok_or_else(|| unprocessable(format!("{key} must be a date, YYYY-MM-DD"))),
    }
}

/// The same day, for a verb whose end of the period has **no** default — where
/// the absence of a bound means "however far back the records go" rather than a
/// day this file would have to pick.
pub(crate) fn optional_period_day(args: &Value, key: &str) -> Result<Option<Date>, Problem> {
    match string_arg(args, key).filter(|raw| !raw.trim().is_empty()) {
        None => Ok(None),
        Some(raw) => parse_iso_date(&raw)
            .ok_or_else(|| unprocessable(format!("{key} must be a date, YYYY-MM-DD")))
            .map(Some),
    }
}

/// What the receivables ledger's lines add up to, by what each entry was:
/// invoiced (invoices' debits), credited back (credit notes), paid (payments'
/// credits, as a positive number), and the net of everything else — manual
/// corrections, FX differences, opening balances.
fn receivable_sums(lines: &[LedgerLine]) -> (i64, i64, i64, i64) {
    let mut invoiced = 0;
    let mut credit_noted = 0;
    let mut paid = 0;
    let mut other = 0;
    for line in lines {
        match line.kind {
            EntryKind::Invoice => invoiced += line.base_cents,
            EntryKind::CreditNote => credit_noted -= line.base_cents,
            EntryKind::Payment => paid -= line.base_cents,
            _ => other += line.base_cents,
        }
    }
    (invoiced, credit_noted, paid, other)
}

/// `ledger_summary` — invoiced, paid and outstanding over a period, read from
/// the receivables account of the tenant's own journal. The books' answer to
/// "how much have we invoiced this year, and how much is unpaid?", which is a
/// different answer from Billing's list of documents: this one is what was
/// **booked**.
///
/// A different answer, and until A10.2 an unexplained one. Documents have only
/// been posted to the journal since the document paths were wired to the posting
/// rules, so on a tenant that invoiced before that upgrade this read returned
/// `0.00` for a year Billing and Insights reported in full — two agents
/// contradicting each other on the same tenant in the same minute, neither of
/// them saying why. So the reading now **sets the period's documents against the
/// entries it found** ([`crate::agent_finance_books::gap_json`]): the figures
/// stay the books', and beside them the reply says how many issued documents the
/// journal does not hold, what they come to, and what puts them in. A reader can
/// no longer take the journal's figure for the whole truth without being told it
/// is not.
pub async fn execute_ledger_summary(account: &Account, args: &Value) -> Reply {
    account.require_finance()?;
    let day = today();
    let january_first = Date::from_calendar_date(day.year(), Month::January, 1)
        .map_err(|_| Problem::server_error())?;
    let from = period_day(args, "from", january_first)?;
    let to = period_day(args, "to", day)?;
    if from > to {
        return Err(unprocessable("from is after to"));
    }
    let receivable = account
        .acc
        .fin_account_for_role(AccountRole::Ar)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| {
            unprocessable(
                "the books have no receivables account yet — open Finance once and the chart is made for you",
            )
        })?;
    let ledger = account
        .acc
        .fin_account_ledger(
            &receivable.id,
            Some(from),
            Some(to),
            alo_store::LEDGER_PAGE_MAX,
        )
        .await
        .map_err(map_store_err)?;
    let currency = account
        .acc
        .billing_base_currency()
        .await
        .map_err(map_store_err)?;
    let (invoiced, credit_noted, paid, other) = receivable_sums(&ledger.lines);
    // What the document list says for the same period, set against the issue
    // entries the ledger above actually holds. A cut-short page cannot answer
    // "is this document in the books?" — an absence there means "I stopped
    // looking" — so a truncated read says so instead of naming documents.
    let documents = if ledger.truncated {
        crate::agent_finance_books::not_compared_json()
    } else {
        let issued =
            crate::agent_finance_books::issued_documents(&account.acc, Some(from), to).await?;
        crate::agent_finance_books::gap_json(
            &currency,
            &issued,
            &crate::agent_finance_books::booked_documents(&ledger.lines),
        )
    };
    // The latest movements, newest first — the evidence behind the figures.
    let entries: Vec<Value> = ledger
        .lines
        .iter()
        .rev()
        .take(MAX_LISTED)
        .map(|line| {
            json!({
                "entryId": line.entry_id.as_str(),
                "date": iso_date(line.entry_date),
                "entryKind": line.kind.as_str(),
                "memo": line.entry_memo,
                "amountCents": line.base_cents,
                "runningCents": line.running_cents,
            })
        })
        .collect();
    ok(json!({
        "kind": "ledgerSummary",
        "from": iso_date(from),
        "to": iso_date(to),
        "currency": currency,
        "invoicedCents": invoiced,
        "creditNotedCents": credit_noted,
        "paidCents": paid,
        "otherMovementCents": other,
        // The receivables balance after the period's last entry — everything
        // customers still owe, earlier periods included.
        "outstandingCents": ledger.closing_cents,
        "openingCents": ledger.opening_cents,
        "entryCount": ledger.lines.len(),
        "entries": entries,
        // When the page was cut short the sums are of what was read, not of
        // the period — said rather than left to read as a total.
        "truncated": ledger.truncated,
        // …and what the documents say, which is the same thing for a tenant
        // whose books hold them all and a different thing for one whose do not.
        "documents": documents,
    }))
}

/// `unmatched_bank_lines` — the imported bank lines the books cannot yet
/// explain, oldest first, exactly as `GET /finance/bank/lines?status=unmatched`
/// serves them.
pub async fn execute_unmatched_bank_lines(account: &Account, _args: &Value) -> Reply {
    account.require_finance()?;
    let lines = account
        .acc
        .bank_lines(None, Some(BankLineStatus::Unmatched))
        .await
        .map_err(map_store_err)?;
    let listed: Vec<Value> = lines
        .iter()
        .take(MAX_LISTED)
        .map(crate::finance_bank::line_json)
        .collect();
    ok(json!({
        "kind": "unmatchedBankLines",
        "lineCount": lines.len(),
        "shown": listed.len(),
        "lines": listed,
    }))
}

/// `expenses_awaiting` — the claims waiting on the company: awaiting a
/// decision by default, or approved and not yet paid back when the asker says
/// `waiting: "reimbursement"`. The approvals inbox's own rows
/// ([`crate::finance_approvals::pending_json`]): the claim, its claimant, and
/// the word that says where it books.
pub async fn execute_expenses_awaiting(account: &Account, args: &Value, state: &AppState) -> Reply {
    account.require_finance()?;
    let waiting = string_arg(args, "waiting")
        .map(|word| word.trim().to_lowercase())
        .filter(|word| !word.is_empty())
        .unwrap_or_else(|| "approval".to_owned());
    // The queue is a cross-user read and lives on the tenant door, behind the
    // gate above — exactly as the approvals inbox reaches it.
    let tenant = state.store.for_tenant(account.tenant.clone());
    let claims = match waiting.as_str() {
        "approval" => tenant.pending_expenses().await,
        "reimbursement" => tenant.reimbursable_expenses().await,
        _ => {
            return Err(unprocessable(
                "waiting is \"approval\" or \"reimbursement\"",
            ));
        }
    }
    .map_err(map_store_err)?;
    let listed: Vec<Value> = claims
        .iter()
        .take(MAX_LISTED)
        .map(crate::finance_approvals::pending_json)
        .collect();
    ok(json!({
        "kind": "expensesAwaiting",
        "waiting": waiting,
        "expenseCount": claims.len(),
        "shown": listed.len(),
        "expenses": listed,
    }))
}

/// The account a code or a name resolves to: an exact code wins alone, then an
/// exact name, then a containing match — and an ambiguity is a refusal that
/// lists the candidates rather than a guess about somebody's books.
fn resolve_account<'a>(
    chart: &'a [ChartAccount],
    wanted: &str,
) -> Result<&'a ChartAccount, Problem> {
    let stated = wanted.trim();
    if stated.is_empty() {
        return Err(unprocessable("name the account by its code or its name"));
    }
    let wanted = stated.to_lowercase();
    let exact_code: Vec<&ChartAccount> = chart
        .iter()
        .filter(|one| one.code.to_lowercase() == wanted)
        .collect();
    let exact_name: Vec<&ChartAccount> = chart
        .iter()
        .filter(|one| one.name.to_lowercase() == wanted)
        .collect();
    let containing: Vec<&ChartAccount> = chart
        .iter()
        .filter(|one| one.name.to_lowercase().contains(&wanted))
        .collect();
    let found = if !exact_code.is_empty() {
        exact_code
    } else if !exact_name.is_empty() {
        exact_name
    } else {
        containing
    };
    match found.as_slice() {
        [] => Err(unprocessable(format!(
            "no account is coded or named \"{stated}\""
        ))),
        [one] => Ok(one),
        several => Err(unprocessable(format!(
            "several accounts match \"{stated}\": {} — say which",
            several
                .iter()
                .map(|one| format!("{} {}", one.code, one.name))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// `account_balance` — one account of the chart with its balance: the whole
/// journal's to date, or its movement over a stated period. The same fold the
/// chart screen reads (`GET /finance/accounts?from&to`,
/// [`alo_store::AccountStore::fin_trial_balance`]), on one account.
pub async fn execute_account_balance(account: &Account, args: &Value) -> Reply {
    account.require_finance()?;
    let wanted = string_arg(args, "account").ok_or_else(|| unprocessable("name the account"))?;
    let chart = account
        .acc
        .fin_accounts(true)
        .await
        .map_err(map_store_err)?;
    if chart.is_empty() {
        return Err(unprocessable(
            "the books have no chart of accounts yet — open Finance once and one is made for you",
        ));
    }
    let one = resolve_account(&chart, &wanted)?;
    let to = period_day(args, "to", today())?;
    let from = optional_period_day(args, "from")?;
    if from.is_some_and(|from| from > to) {
        return Err(unprocessable("from is after to"));
    }
    let trial = account
        .acc
        .fin_trial_balance(from, Some(to))
        .await
        .map_err(map_store_err)?;
    // An account the period never moved is a zero, not an absence — the same
    // reading the chart screen gives it.
    let moved = trial
        .accounts
        .iter()
        .find(|balance| balance.account_id == one.id)
        .map_or((0, 0, 0, 0), |balance| {
            (
                balance.balance_cents,
                balance.debit_cents,
                balance.credit_cents,
                balance.postings,
            )
        });
    let currency = account
        .acc
        .billing_base_currency()
        .await
        .map_err(map_store_err)?;
    ok(json!({
        "kind": "account",
        "from": from.map(iso_date),
        "to": iso_date(to),
        "currency": currency,
        "account": crate::finance_chart::account_json(
            one,
            Some(&crate::finance_chart::Movement {
                balance_cents: moved.0,
                debit_cents: moved.1,
                credit_cents: moved.2,
                postings: moved.3,
            }),
        ),
    }))
}

/// The module's verbs by name (A4.1c) — Finance's one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module. The kept executors —
/// [`crate::agent_finance`] for the writes, [`crate::agent_finance_answers`]
/// for the VAT figures and the journal scan — are reached from here so the
/// agent has one place to look.
pub(crate) fn dispatch<'a>(
    state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "ledger_summary" => Box::pin(execute_ledger_summary(account, args)),
        "vat_summary" => Box::pin(crate::agent_finance_answers::execute_vat_summary(
            account, args,
        )),
        "flag_anomalies" => Box::pin(crate::agent_finance_answers::execute_flag_anomalies(
            account, args,
        )),
        "unmatched_bank_lines" => Box::pin(execute_unmatched_bank_lines(account, args)),
        "expenses_awaiting" => Box::pin(execute_expenses_awaiting(account, args, state)),
        "account_balance" => Box::pin(execute_account_balance(account, args)),
        "categorise_transactions" => Box::pin(
            crate::agent_finance::execute_categorise_transactions(account, args),
        ),
        "approve_expense" => Box::pin(crate::agent_finance::execute_approve_expense(
            account, args, state,
        )),
        "post_missing_documents" => Box::pin(
            crate::agent_finance_books::execute_post_missing_documents(account, args),
        ),
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alo_ai::finance_intents::FINANCE;
    use alo_store::{AccountType, FinAccountId, FinEntryId, FinPostingId};
    use time::OffsetDateTime;

    /// Every `/finance/` route the router registers is the adapter of a verb
    /// or excluded with a reason — the coverage ADR 0058 makes structural.
    #[test]
    fn every_finance_route_is_a_verb_or_an_exclusion() {
        let router = include_str!("server.rs");
        let missing = FINANCE.uncovered(router, "/finance/");
        assert!(
            missing.is_empty(),
            "routes with neither a verb nor a reason: {missing:?}"
        );
        // …and every verb's route exists, so an intent cannot name a route the
        // app does not have.
        let routes = alo_ai::routes_in(router, "/finance/");
        for intent in FINANCE.intents {
            for route in intent.routes {
                assert!(
                    routes.contains(&(*route).to_owned()),
                    "{}: {route} is not a route",
                    intent.name
                );
            }
        }
    }

    #[test]
    fn every_verb_the_registry_offers_is_dispatched() {
        let dispatch = include_str!("finance_intents.rs");
        for intent in FINANCE.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// Finance's registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, the registry names it once, and the
    /// two lists are the same length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("finance_intents::").count(),
            1,
            "agent.rs names Finance only in MODULES"
        );
        assert!(agent.contains("crate::finance_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }

    fn line(kind: EntryKind, base_cents: i64) -> LedgerLine {
        LedgerLine {
            posting_id: FinPostingId::new("p"),
            entry_id: FinEntryId::new("e"),
            entry_date: today(),
            kind,
            source: None,
            entry_memo: String::new(),
            memo: String::new(),
            currency: "EUR".to_owned(),
            amount_cents: base_cents,
            base_cents,
            running_cents: 0,
            vat_rate_bp: None,
            customer_id: None,
            supplier_key: None,
            project_id: None,
            user_id: None,
        }
    }

    #[test]
    fn the_receivable_sums_split_the_ledger_by_what_each_entry_was() {
        let lines = vec![
            line(EntryKind::Invoice, 121_000),
            line(EntryKind::Invoice, 60_500),
            line(EntryKind::CreditNote, -12_100),
            line(EntryKind::Payment, -60_500),
            line(EntryKind::Manual, 500),
        ];
        let (invoiced, credit_noted, paid, other) = receivable_sums(&lines);
        assert_eq!(invoiced, 181_500);
        assert_eq!(credit_noted, 12_100, "a credit note is a positive figure");
        assert_eq!(paid, 60_500, "money in is a positive figure");
        assert_eq!(other, 500);
        assert_eq!(receivable_sums(&[]), (0, 0, 0, 0));
    }

    fn chart_account(code: &str, name: &str) -> ChartAccount {
        ChartAccount {
            id: FinAccountId::new(code),
            code: code.to_owned(),
            name: name.to_owned(),
            kind: AccountType::Expense,
            role: None,
            active: true,
            system: true,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn an_account_resolves_by_code_by_name_or_by_a_containing_match() {
        let chart = vec![
            chart_account("6100", "Marketing"),
            chart_account("6110", "Marketing events"),
            chart_account("4000", "Sales"),
        ];
        assert_eq!(resolve_account(&chart, " 6100 ").unwrap().code, "6100");
        assert_eq!(resolve_account(&chart, "marketing").unwrap().code, "6100");
        assert_eq!(resolve_account(&chart, "Sales").unwrap().code, "4000");
        assert_eq!(resolve_account(&chart, "events").unwrap().code, "6110");
    }

    #[test]
    fn an_ambiguous_account_is_a_refusal_that_lists_the_candidates() {
        let chart = vec![
            chart_account("6100", "Marketing"),
            chart_account("6110", "Marketing events"),
        ];
        let problem = resolve_account(&chart, "market").expect_err("ambiguous");
        let detail = problem.detail.unwrap_or_default();
        assert!(detail.contains("6100 Marketing"), "{detail}");
        assert!(detail.contains("6110 Marketing events"), "{detail}");
        assert!(detail.contains("say which"), "{detail}");
        let missing = resolve_account(&chart, "Aardvark").expect_err("unknown");
        assert!(
            missing
                .detail
                .unwrap_or_default()
                .contains("no account is coded or named \"Aardvark\"")
        );
        assert!(resolve_account(&chart, "  ").is_err());
    }
}
