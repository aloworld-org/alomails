//! **The chart of accounts** over HTTP (alo Finance, ADR 0035, wave B4.13c) —
//! the list of places money can be, and the five doors a tenant edits it
//! through — over [`alo_store::fin_accounts`].
//!
//! Five decisions this file makes rather than the store.
//!
//! - **The list seeds the chart on first read**, in the caller's language
//!   ([`crate::finance_chart_names`]). A tenant that has never opened Finance
//!   is handed a working chart rather than an empty screen with an "add your
//!   first account" button — every posting rule's role resolves from the first
//!   document, which is what makes issuing an invoice on day one book itself.
//!   The seed runs once per tenant (`fin_seeds`), so a tenant who deleted our
//!   chart and typed their accountant's is not handed ours again the next
//!   morning.
//! - **Admin or accountant, on every door including the reads.** The chart is
//!   not a preference and it is not a directory: it says what the company owes,
//!   is owed and earns, and it is the instrument the books are kept with. This
//!   is the same [`crate::state::Account::require_finance`] gate the reports,
//!   the approvals inbox, the period lock and the bank use — and the *read*
//!   matters as much as the writes, because a seeding read writes.
//! - **Deactivating is a field of the `PATCH`, deleting is its own door.** They
//!   are different acts with different consequences: an inactive account keeps
//!   its history and its balance and stops offering itself for new work, while
//!   a delete is only ever possible on a custom account that never carried a
//!   posting. The store enforces both refusals (`409`), and this layer does not
//!   restate them.
//! - **A `PATCH` is merged onto the stored record here**, because the store's
//!   update is a full replace. Absent means "leave it alone", so a rename never
//!   silently clears the role — which, on this table, would quietly unhook a
//!   posting rule and make the next invoice unbookable.
//! - **Balances are optional and are the journal's**, not a second sum of our
//!   own: `?from&to` folds [`alo_store::AccountStore::fin_trial_balance`] over
//!   the same period the reports use, and an account the period never moved
//!   carries a zero rather than being dropped from the chart. Without the two
//!   days the list is the chart alone — the shape a picker wants.
//!
//! Nothing here is personal data: an account code, a name a tenant gave their
//! own chart, and money.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{Account as ChartAccount, AccountRole, AccountType, FinAccountId, NewAccount};

use crate::billing::{flag, iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::finance_chart_names::chart_seed_for;
use crate::finance_reports::{day, reader};
use crate::state::AppState;

/// What one account moved in the period asked for, when one was. Crate-visible
/// so the Finance agent's `account_balance` (`crate::finance_intents`) answers
/// with the same row the chart screen shows.
pub(crate) struct Movement {
    pub(crate) balance_cents: i64,
    pub(crate) debit_cents: i64,
    pub(crate) credit_cents: i64,
    pub(crate) postings: i64,
}

/// One account as JSON.
///
/// `role` is the posting-rule job it does, `null` on an ordinary account, and
/// `system` says we seeded it — renameable, recodeable, never deletable. The
/// movement fields are present only when the caller asked for a period, and are
/// `null` otherwise: zero would be a claim about the journal that was never
/// read.
pub(crate) fn account_json(account: &ChartAccount, moved: Option<&Movement>) -> Value {
    json!({
        "id": account.id.as_str(),
        "code": account.code,
        "name": account.name,
        "type": account.kind.as_str(),
        "role": account.role.map(AccountRole::as_str),
        "active": account.active,
        "system": account.system,
        "balanceCents": moved.map(|m| m.balance_cents),
        "debitCents": moved.map(|m| m.debit_cents),
        "creditCents": moved.map(|m| m.credit_cents),
        "postings": moved.map(|m| m.postings),
        "createdAt": iso(account.created_at),
        "updatedAt": iso(account.updated_at),
    })
}

/// The list's query string: whether to include the accounts a tenant has
/// retired, the period to state each account's movement over, and the language
/// the default chart is written in if this read is the one that seeds it.
/// `camelCase` on the wire, as `/billing/customers`' `includeArchived` spells
/// the same question — and stated deliberately, because a filter whose name a
/// client cannot guess is a filter that silently does nothing, which is exactly
/// how a retired account stays invisible on the screen that exists to bring it
/// back. (Found by the wire check, not by a unit test: serde's default snake
/// case had made `includeInactive` a parameter the server ignored.)
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartQuery {
    #[serde(default)]
    include_inactive: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    lang: Option<String>,
}

impl ChartQuery {
    /// The period the movements are asked over, or `None` for "the chart
    /// alone".
    ///
    /// Both ends together or neither: one end without the other is a period
    /// somebody thinks they asked for and did not get, so it is a `422` naming
    /// the end that is missing rather than a silently open-ended fold.
    ///
    /// # Errors
    /// [`Problem`] with `422` when only one end is stated, or when either is
    /// not a plain day.
    fn period(&self) -> Result<Option<(time::Date, time::Date)>, Problem> {
        let stated = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        match (stated(&self.from), stated(&self.to)) {
            (None, None) => Ok(None),
            (from, to) => Ok(Some((
                day("from", from.as_deref())?,
                day("to", to.as_deref())?,
            ))),
        }
    }
}

/// The writable shape of an account as a client sends it. Every field is
/// optional so one type serves both doors: `POST` requires what the store
/// requires (a blank code or name is its `422`), and `PATCH` merges what is
/// present onto the stored record.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountBody {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    /// The posting-rule job, where `""` means "an ordinary account" — the
    /// store's own spelling of "none", so a form that clears the select says
    /// what it means.
    #[serde(default)]
    role: Option<String>,
    /// Whether it takes new postings. `PATCH` only; a created account is
    /// active, because creating one a tenant then has to switch on is a step
    /// that exists for nobody.
    #[serde(default)]
    active: Option<bool>,
}

impl AccountBody {
    /// The account type the body states, or the `422` naming the accepted set.
    ///
    /// The store owns the vocabulary ([`AccountType::parse`]) so the two doors
    /// into it cannot drift into two spellings of the same refusal.
    fn kind(&self, fallback: AccountType) -> Result<AccountType, Problem> {
        match self.kind.as_deref().map(str::trim) {
            None => Ok(fallback),
            Some(stated) => AccountType::parse(stated).map_err(map_store_err),
        }
    }

    /// The role the body states, where an absent field keeps `fallback` and an
    /// empty string clears it.
    fn role(&self, fallback: Option<AccountRole>) -> Result<Option<AccountRole>, Problem> {
        match self.role.as_deref() {
            None => Ok(fallback),
            Some(stated) => AccountRole::parse(stated).map_err(map_store_err),
        }
    }
}

/// Reads one of the tenant's accounts, or the `404` that says it is not theirs
/// — the same answer an id belonging to another tenant gets, indistinguishably
/// by design.
async fn stored(
    account: &crate::state::Account,
    id: &FinAccountId,
) -> Result<ChartAccount, Problem> {
    account
        .acc
        .fin_account(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "not found"))
}

/// `GET /finance/accounts?includeInactive&from&to&lang` →
/// `{"accounts":[…],"seeded":bool}` — the tenant's chart in code order,
/// **seeding the default one on first use**.
///
/// `seeded` says whether this read is the one that wrote it, so a screen can
/// tell a tenant where the accounts came from instead of presenting twenty rows
/// somebody has to assume they created.
///
/// # Errors
/// `401` without a valid bearer token; `403` for a member who is neither an
/// admin nor an accountant; `422` when half a period is stated or a day is
/// malformed; `500` on a store failure.
pub async fn list_accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChartQuery>,
) -> Result<Json<Value>, Problem> {
    let account = reader(&state, &headers).await?;
    let period = query.period()?;
    let include_inactive = flag(query.include_inactive.as_deref());
    let existed = account
        .acc
        .fin_seed_ran(alo_store::CHART_SEED_KEY)
        .await
        .map_err(map_store_err)?;
    let accounts = account
        .acc
        .fin_accounts_or_seed(
            &chart_seed_for(query.lang.as_deref().unwrap_or_default()),
            include_inactive,
        )
        .await
        .map_err(map_store_err)?;

    // The movements, when a period was asked for: one fold of the journal for
    // the whole chart rather than a query per row, keyed by account id.
    let mut moved: HashMap<String, Movement> = HashMap::new();
    let mut currency: Option<String> = None;
    if let Some((from, to)) = period {
        // The accounting currency travels with the figures, as it does on every
        // report: an amount whose unit a screen had to assume is an amount that
        // reads wrongly the day a tenant keeps books in something else. Read
        // only when there are figures to state it about.
        currency = Some(
            account
                .acc
                .billing_base_currency()
                .await
                .map_err(map_store_err)?,
        );
        let trial = account
            .acc
            .fin_trial_balance(Some(from), Some(to))
            .await
            .map_err(map_store_err)?;
        for balance in trial.accounts {
            moved.insert(
                balance.account_id.as_str().to_owned(),
                Movement {
                    balance_cents: balance.balance_cents,
                    debit_cents: balance.debit_cents,
                    credit_cents: balance.credit_cents,
                    postings: balance.postings,
                },
            );
        }
    }
    // An account the period never touched is a zero, not an absence: the chart
    // is the list of places money can be, and a row missing from it reads as an
    // account that does not exist.
    let zero = Movement {
        balance_cents: 0,
        debit_cents: 0,
        credit_cents: 0,
        postings: 0,
    };
    let rendered: Vec<Value> = accounts
        .iter()
        .map(|account| {
            let movement = period.map(|_| moved.get(account.id.as_str()).unwrap_or(&zero));
            account_json(account, movement)
        })
        .collect();
    Ok(Json(json!({
        "accounts": rendered,
        "seeded": !existed,
        "currency": currency,
    })))
}

/// `GET /finance/accounts/{id}` → `{"account":{…}}` — one account of the
/// tenant's chart.
///
/// # Errors
/// `401`/`403` as above; `404` when the account is not this tenant's.
pub async fn get_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = reader(&state, &headers).await?;
    let stored = stored(&account, &FinAccountId::new(id)).await?;
    Ok(Json(json!({ "account": account_json(&stored, None) })))
}

/// `POST /finance/accounts` `{code,name,type,role?}` → `{"account":{…}}` — a
/// custom account, the tenant's own line in their own chart.
///
/// # Errors
/// `401`/`403` as above; `400` when the body is not JSON; `422` when the code
/// or the name is blank, over-long or spaced, or the type or role is not one of
/// ours; `409` when the code is taken or another account already holds the
/// role; `500` on a store failure.
pub async fn create_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = reader(&state, &headers).await?;
    let req: AccountBody = parse_body(&body)?;
    // The type is the one field with no sensible default: an account is an
    // asset or an expense, and guessing which would put a cost on the balance
    // sheet. The store's own words name the accepted set.
    let kind = match req.kind.as_deref().map(str::trim) {
        None | Some("") => {
            return Err(Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "type is required: an account is an asset, a liability, equity, income \
                 or an expense",
            ));
        }
        Some(stated) => AccountType::parse(stated).map_err(map_store_err)?,
    };
    let input = NewAccount {
        code: req.code.clone().unwrap_or_default(),
        name: req.name.clone().unwrap_or_default(),
        kind,
        role: req.role(None)?,
    };
    let id = account
        .acc
        .create_fin_account(&input)
        .await
        .map_err(map_store_err)?;
    let stored = stored(&account, &id).await?;
    Ok(Json(json!({ "account": account_json(&stored, None) })))
}

/// `PATCH /finance/accounts/{id}` `{code?,name?,type?,role?,active?}` →
/// `{"account":{…}}` — renames, recodes, reclassifies, rehooks or retires an
/// account, **including a seeded one**: a tenant whose accountant wants `1400`
/// for receivables must be able to say so, and the posting rules follow the
/// role rather than the code.
///
/// Absent fields are left exactly as they were, which is what stops a rename
/// clearing the role and unhooking a posting rule.
///
/// # Errors
/// `401`/`403` as above; `404` when the account is not this tenant's; `400`
/// when the body is not JSON; `422` as for create; `409` when the code is taken
/// or the role is held by another account; `500` on a store failure.
pub async fn update_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = reader(&state, &headers).await?;
    let id = FinAccountId::new(id);
    let current = stored(&account, &id).await?;
    let req: AccountBody = parse_body(&body)?;
    let input = NewAccount {
        code: req.code.clone().unwrap_or_else(|| current.code.clone()),
        name: req.name.clone().unwrap_or_else(|| current.name.clone()),
        kind: req.kind(current.kind)?,
        role: req.role(current.role)?,
    };
    account
        .acc
        .update_fin_account(&id, &input)
        .await
        .map_err(map_store_err)?;
    // Retiring an account is its own store call, so an ordinary rename can
    // never drop one out of the posting rules by accident. Idempotent, and only
    // made when the body actually says something about it.
    if let Some(active) = req.active
        && active != current.active
    {
        account
            .acc
            .set_fin_account_active(&id, active)
            .await
            .map_err(map_store_err)?;
    }
    let stored = stored(&account, &id).await?;
    Ok(Json(json!({ "account": account_json(&stored, None) })))
}

/// `DELETE /finance/accounts/{id}` → `204` — removes a custom account that has
/// never been used.
///
/// Two refusals, both the store's and both `409`: a **seeded** account is never
/// deletable (deactivating is what a tenant who does not use one actually
/// wants), and an account that **carries a posting** is history rather than a
/// preference.
///
/// # Errors
/// `401`/`403` as above; `404` when the account is not this tenant's; `409` as
/// above; `500` on a store failure.
pub async fn delete_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, Problem> {
    let account = reader(&state, &headers).await?;
    account
        .acc
        .delete_fin_account(&FinAccountId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Month, OffsetDateTime};

    fn query(from: Option<&str>, to: Option<&str>) -> ChartQuery {
        ChartQuery {
            include_inactive: None,
            from: from.map(str::to_owned),
            to: to.map(str::to_owned),
            lang: None,
        }
    }

    fn body(json: serde_json::Value) -> AccountBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("{e}"))
    }

    fn account() -> ChartAccount {
        ChartAccount {
            id: FinAccountId::new("acc-1"),
            code: "1100".to_owned(),
            name: "Trade receivables".to_owned(),
            kind: AccountType::Asset,
            role: Some(AccountRole::Ar),
            active: true,
            system: true,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn an_account_says_what_it_is_what_it_does_and_who_wrote_it() {
        let value = account_json(&account(), None);
        assert_eq!(value["id"], "acc-1");
        assert_eq!(value["code"], "1100");
        assert_eq!(value["type"], "asset");
        assert_eq!(value["role"], "ar");
        assert_eq!(value["active"], true);
        assert_eq!(value["system"], true);
        // No period was asked for, so nothing is claimed about the journal.
        assert_eq!(value["balanceCents"], Value::Null);
        assert_eq!(value["postings"], Value::Null);
    }

    #[test]
    fn an_ordinary_account_has_no_role_rather_than_an_empty_one() {
        let mut ordinary = account();
        ordinary.role = None;
        ordinary.system = false;
        let value = account_json(&ordinary, None);
        assert_eq!(value["role"], Value::Null, "not \"\"");
        assert_eq!(value["system"], false);
    }

    #[test]
    fn a_period_puts_the_journals_own_figures_on_the_row() {
        let value = account_json(
            &account(),
            Some(&Movement {
                balance_cents: -1_250,
                debit_cents: 0,
                credit_cents: 1_250,
                postings: 3,
            }),
        );
        assert_eq!(value["balanceCents"], -1_250);
        assert_eq!(value["debitCents"], 0);
        assert_eq!(value["creditCents"], 1_250);
        assert_eq!(value["postings"], 3);
    }

    #[test]
    fn the_chart_alone_is_asked_for_by_stating_no_period() {
        assert!(
            query(None, None)
                .period()
                .unwrap_or_else(|e| panic!("{e:?}"))
                .is_none()
        );
        assert!(
            query(Some("  "), Some(""))
                .period()
                .unwrap_or_else(|e| panic!("{e:?}"))
                .is_none(),
            "blank is unstated, not a period"
        );
    }

    #[test]
    fn half_a_period_is_refused_rather_than_folded_open_ended() {
        for (from, to, expected) in [
            (None, Some("2026-12-31"), "from"),
            (Some("2026-01-01"), None, "to"),
        ] {
            let problem = query(from, to)
                .period()
                .err()
                .unwrap_or_else(|| panic!("{from:?}/{to:?} should have been refused"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert!(
                problem.detail.unwrap_or_default().starts_with(expected),
                "the refusal names the end that is missing"
            );
        }
    }

    #[test]
    fn a_stated_period_is_read_as_two_plain_days() {
        let (from, to) = query(Some("2026-01-01"), Some("2026-03-31"))
            .period()
            .unwrap_or_else(|e| panic!("{e:?}"))
            .unwrap_or_else(|| panic!("a period was stated"));
        assert_eq!(
            from,
            time::Date::from_calendar_date(2026, Month::January, 1)
                .unwrap_or_else(|e| panic!("{e}"))
        );
        assert_eq!(
            to,
            time::Date::from_calendar_date(2026, Month::March, 31)
                .unwrap_or_else(|e| panic!("{e}"))
        );
    }

    #[test]
    fn a_patch_that_says_nothing_changes_nothing() {
        let current = account();
        let patch = body(json!({}));
        assert_eq!(
            patch.kind(current.kind).unwrap_or_else(|e| panic!("{e:?}")),
            AccountType::Asset
        );
        assert_eq!(
            patch.role(current.role).unwrap_or_else(|e| panic!("{e:?}")),
            Some(AccountRole::Ar),
            "a rename must never unhook a posting rule"
        );
        assert_eq!(patch.active, None);
    }

    #[test]
    fn an_empty_role_is_how_a_form_says_ordinary_account() {
        let patch = body(json!({ "role": "" }));
        assert_eq!(
            patch
                .role(Some(AccountRole::Ar))
                .unwrap_or_else(|e| panic!("{e:?}")),
            None
        );
    }

    #[test]
    fn a_word_that_is_not_one_of_ours_is_refused_in_the_stores_own_sentence() {
        let problem = body(json!({ "role": "profit" }))
            .role(None)
            .err()
            .unwrap_or_else(|| panic!("an invented role should have been refused"));
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        let detail = problem.detail.unwrap_or_default();
        assert!(detail.contains("account role must be"), "{detail}");
        assert!(detail.contains("vat_output"), "the set is named: {detail}");

        let problem = body(json!({ "type": "profit" }))
            .kind(AccountType::Asset)
            .err()
            .unwrap_or_else(|| panic!("an invented type should have been refused"));
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            problem
                .detail
                .unwrap_or_default()
                .contains("account type must be"),
        );
    }

    #[test]
    fn a_patch_reads_every_writable_field() {
        let patch = body(json!({
            "code": "1400",
            "name": "Debtors",
            "type": "asset",
            "role": "ar",
            "active": false,
        }));
        assert_eq!(patch.code.as_deref(), Some("1400"));
        assert_eq!(patch.name.as_deref(), Some("Debtors"));
        assert_eq!(
            patch
                .kind(AccountType::Expense)
                .unwrap_or_else(|e| panic!("{e:?}")),
            AccountType::Asset
        );
        assert_eq!(
            patch.role(None).unwrap_or_else(|e| panic!("{e:?}")),
            Some(AccountRole::Ar)
        );
        assert_eq!(patch.active, Some(false));
    }
}
