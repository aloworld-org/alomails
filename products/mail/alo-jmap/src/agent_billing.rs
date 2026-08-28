//! Executing the **billing** tools of an approved agent proposal (ADR 0034,
//! ADR 0035 wave B1.25) — the acting half of what
//! [`alo_ai::agent_billing`] describes to the model.
//!
//! Called only from [`crate::agent::agent_execute`], which is the single acting
//! path: the user saw the proposal and approved it. Everything here therefore
//! runs through the caller's own tenant-scoped store handle — an agent can no
//! more reach another tenant's customer than the browser that asked it can.
//!
//! Three rules shape this module, and they are why it is not thin glue:
//!
//! - **The model speaks names; the store speaks ids.** A proposal carries "the
//!   customer Acme", "the product Consulting", "invoice INV-2026-00042" —
//!   whatever the user said. Resolving a name to exactly one of the tenant's
//!   records is [`crate::agent_args`], shared with every other product agent,
//!   and an ambiguous name is a refusal that lists the candidates, never a
//!   guess. This is the source resolution the mail tools do with a source
//!   number, done for records a search does not return.
//! - **Money arrives as integers and is never recomputed.** Prices come in
//!   whole cents and VAT rates in basis points; a quantity may be written
//!   "1.5", and it is converted to milli-units by reading its digits, not by
//!   multiplying a float. Every total is then the store's
//!   ([`alo_store::billing_totals`]) — nothing in this file adds money up.
//! - **Nothing here issues, numbers or sends.** The tools raise a *draft*
//!   invoice, accept a quote into a *draft* invoice, and write a mail *draft*.
//!   Both irreversible acts of billing — assigning a legal number, putting mail
//!   on the wire — stay where a human performs them deliberately.
//!
//! Every store function called here is the same one the `/billing/*` routes
//! call. There is no agent-only write path, so there is no second place for the
//! rules of a document to drift.

use axum::Json;
use serde_json::{Value, json};

use alo_store::billing_invoices::NewInvoice;
use alo_store::billing_line::QTY_MAX_MILLI;
use alo_store::{BillingCustomerId, NewLine};

use crate::agent_args::{integer, pick, pick_name, string_arg, unprocessable};
use crate::billing::map_store_err;
use crate::billing_document::today;
use crate::billing_reminder::draft_reminder;
use crate::error::Problem;
use crate::state::{Account, AppState};

/// The most lines one proposal may raise — the store's own cap
/// ([`alo_store::billing_line::MAX_LINES`]), refused here with a sentence about
/// the proposal rather than about a line set, since a model that produced 900
/// lines has misunderstood the request rather than written a long invoice.
const MAX_PROPOSED_LINES: usize = alo_store::billing_line::MAX_LINES;

/// `create_invoice_draft` — raise a draft invoice for a named customer from the
/// approved lines.
///
/// The customer and every product are resolved against **this tenant's** active
/// records; the lines are then priced from the price list or from the approved
/// integers, and validated by the store before the header is written, so a
/// mistake in the last line does not leave an empty draft behind (the same
/// order the `POST /billing/invoices` route uses, for the same reason).
///
/// # Errors
/// `422` when the customer or a product cannot be resolved to exactly one
/// record, when a line states neither a product nor a price, or when a quantity
/// is not a number the store can hold; the store's own `404`/`422` otherwise.
pub async fn execute_create_invoice_draft(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let wanted = string_arg(args, "customer")
        .ok_or_else(|| unprocessable("which customer this is for is required"))?;
    let customers = account
        .acc
        .billing_customers(false)
        .await
        .map_err(map_store_err)?;
    let customer = pick(
        &wanted,
        customers.iter().map(|c| (c.name.as_str(), c)).collect(),
        "customer",
    )?;

    let proposed = args
        .get("lines")
        .and_then(Value::as_array)
        .filter(|lines| !lines.is_empty())
        .ok_or_else(|| unprocessable("an invoice needs at least one line"))?;
    if proposed.len() > MAX_PROPOSED_LINES {
        return Err(unprocessable(format!(
            "an invoice may have at most {MAX_PROPOSED_LINES} lines"
        )));
    }
    // The price list is read once, and only when a line actually names a
    // product — an invoice written entirely from stated prices costs no query.
    let catalogue = if proposed.iter().any(|line| line.get("product").is_some()) {
        account
            .acc
            .billing_products(false)
            .await
            .map_err(map_store_err)?
    } else {
        Vec::new()
    };
    let mut lines = Vec::with_capacity(proposed.len());
    for (index, line) in proposed.iter().enumerate() {
        lines.push(
            new_line(line, &catalogue)
                .map_err(|why| unprocessable(format!("line {}: {why}", index + 1)))?,
        );
    }
    let header = NewInvoice {
        reference: string_arg(args, "reference").unwrap_or_default(),
        note: string_arg(args, "note").unwrap_or_default(),
        ..NewInvoice::for_customer(BillingCustomerId::new(customer.id.as_str()))
    };
    // The same core `POST /billing/invoices` runs (A4.1b): lines validated
    // before the header is written, the stored document answered.
    let invoice =
        crate::billing_intents::create_invoice_draft(account, &header, Some(&lines)).await?;
    crate::billing_intents::ok(json!({ "kind": "invoice", "invoice": invoice }))
}

/// `quote_to_invoice` — accept the quote with the approved number, which closes
/// it and raises the draft invoice for it.
///
/// The number is resolved to one of the tenant's quotes and the acceptance is
/// the store's own ([`alo_store::AccountStore::accept_billing_quote`]): one
/// transaction, the offer's frozen prices copied into the new draft, the link
/// back recorded. A quote that is not open is the store's `409`, in the words
/// the `/billing/quotes/{id}/accept` route answers with.
///
/// # Errors
/// `422` when no quote of the tenant carries that number; the store's `409`
/// when the offer is not open to being accepted.
pub async fn execute_quote_to_invoice(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let number = string_arg(args, "quote")
        .or_else(|| string_arg(args, "number"))
        .ok_or_else(|| unprocessable("the quote number is required"))?;
    let id = account
        .acc
        .billing_quote_id_by_number(&number)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| no_such("quote", &number))?;
    // The same core `POST /billing/quotes/{id}/accept` runs (A4.1b). Which
    // document the acceptance raised depends on the offer's lines (ADR 0054
    // §5), so the result **names its kind** rather than assuming one. An agent
    // that reported "invoice" for a sales order would be telling the room
    // something untrue about what it had just done.
    let mut body = crate::billing_intents::accept_quote(account, &id).await?;
    let kind = if body["salesOrder"].is_null() {
        "invoice"
    } else {
        "salesOrder"
    };
    if let Some(object) = body.as_object_mut() {
        object.insert("kind".to_owned(), json!(kind));
    }
    crate::billing_intents::ok(body)
}

/// `draft_payment_reminder` — write the reminder for the approved invoice
/// number into the caller's Drafts ([`crate::billing_reminder`]).
///
/// Nothing is sent: the letter lands where the user reads it, edits it and
/// sends it themselves, which is the rule every agent draft tool follows
/// (ADR 0023/0034) and the rule the whole billing module follows for mail.
///
/// # Errors
/// `422` when no invoice of the tenant carries that number, or the customer has
/// no usable address; `409` for a document that owes nothing.
pub async fn execute_draft_payment_reminder(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Result<Json<Value>, Problem> {
    let number = string_arg(args, "invoice")
        .or_else(|| string_arg(args, "number"))
        .ok_or_else(|| unprocessable("the invoice number is required"))?;
    let id = account
        .acc
        .billing_invoice_id_by_number(&number)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| no_such("invoice", &number))?;
    let note = string_arg(args, "note");
    let lang = string_arg(args, "lang").unwrap_or_default();
    let reminder = draft_reminder(account, state, &id, &lang, note.as_deref(), today()).await?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "draft",
            "id": reminder.message_id,
            "invoice": reminder.number,
            "to": reminder.to,
            "subject": reminder.subject,
            "daysOverdue": reminder.days_overdue,
            "outstandingCents": reminder.outstanding_cents,
        }
    })))
}

/// The refusal for a document number that is not the tenant's.
///
/// A `422` rather than a `404`: the request is well-formed and the route
/// exists — it is the *name in it* that resolves to nothing, which is the same
/// class of answer an unknown customer name gets, and it keeps the two ways a
/// billing tool can fail to find a record from being reported two ways.
fn no_such(kind: &str, number: &str) -> Problem {
    unprocessable(format!("no {kind} of yours is numbered {number}"))
}

/// One approved line, priced from the price list or from the stated integers.
///
/// Returns the reason as plain text so the caller can prefix it with the line's
/// position — which is how the store reports a bad line, and how a user finds
/// it in a proposal of twelve.
fn new_line(line: &Value, catalogue: &[alo_store::Product]) -> Result<NewLine, String> {
    let qty_milli = quantity_milli(line.get("quantity"))?;
    let stated_price = integer(line.get("unitPriceCents"), "unitPriceCents")?;
    let stated_rate = integer(line.get("vatRateBp"), "vatRateBp")?;
    let described = line
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(str::to_owned);

    match line.get("product").and_then(Value::as_str).map(str::trim) {
        Some(wanted) if !wanted.is_empty() => {
            if stated_price.is_some() || stated_rate.is_some() {
                return Err(
                    "state a product or a price, not both — a product is priced by the price list"
                        .to_owned(),
                );
            }
            let product = pick_name(
                wanted,
                catalogue.iter().map(|p| (p.name.as_str(), p)).collect(),
                "product",
            )?;
            Ok(NewLine {
                description: described.unwrap_or_else(|| product.name.clone()),
                unit: product.unit.clone(),
                qty_milli,
                unit_price_cents: product.unit_price_cents,
                vat_rate_bp: product.vat_rate_bp,
            })
        }
        _ => {
            let description = described.ok_or("a line needs a product or a description")?;
            let unit_price_cents =
                stated_price.ok_or("a line without a product needs unitPriceCents")?;
            let vat_rate_bp = stated_rate.ok_or("a line without a product needs vatRateBp")?;
            Ok(NewLine {
                description,
                unit: line
                    .get("unit")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or_default()
                    .to_owned(),
                qty_milli,
                unit_price_cents,
                vat_rate_bp: i32::try_from(vat_rate_bp)
                    .map_err(|_| "vatRateBp is not a VAT rate".to_owned())?,
            })
        }
    }
}

/// The approved quantity in milli-units; a line that states none is one unit.
fn quantity_milli(value: Option<&Value>) -> Result<i64, String> {
    let text = match value {
        None | Some(Value::Null) => return Ok(1_000),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(other) => return Err(format!("quantity must be a number, not {other}")),
    };
    let milli = milli_from_decimal(&text).ok_or_else(|| {
        format!("{text} is not a quantity we can hold — at most three decimal places")
    })?;
    if milli == 0 {
        return Err("a line of nothing is not a line".to_owned());
    }
    Ok(milli)
}

/// Reads a decimal written as text into milli-units, exactly: `"1.5"` → `1500`,
/// `"2"` → `2000`, `"-0.25"` → `-250`.
///
/// By digits rather than by arithmetic on a floating-point value, because a
/// quantity multiplies a price and the constitution's "no floats for money"
/// does not stop at the price. Anything that is not a plain decimal — an
/// exponent, a thousands separator, a fourth decimal place, a quantity beyond
/// the store's cap — is `None`, so it is refused rather than silently rounded.
fn milli_from_decimal(text: &str) -> Option<i64> {
    let text = text.trim();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (whole, fraction) = match digits.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (digits, ""),
    };
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !fraction.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if fraction.len() > 3 {
        return None;
    }
    let whole: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let thousandths: i64 = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<3}").parse().ok()?
    };
    let milli = whole.checked_mul(1_000)?.checked_add(thousandths)?;
    if milli > QTY_MAX_MILLI {
        return None;
    }
    Some(if negative { -milli } else { milli })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use alo_store::BillingProductId;
    use alo_store::billing_products::Product;
    use axum::http::StatusCode;
    use time::OffsetDateTime;

    fn product(name: &str, unit: &str, cents: i64, bp: i32) -> Product {
        Product {
            id: BillingProductId::new(format!("p-{name}")),
            name: name.to_owned(),
            unit: unit.to_owned(),
            unit_price_cents: cents,
            vat_rate_bp: bp,
            // The catalog half (B5.02) says nothing about resolving a product
            // by name, which is what these tests exercise: a service with no
            // codes is exactly the shape a billing tenant has.
            sku: String::new(),
            barcode: String::new(),
            stocked: false,
            purchase_price_cents: 0,
            photo_node_id: None,
            default_supplier_id: None,
            archived_at: None,
            created_by: "u1".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn catalogue() -> Vec<Product> {
        vec![
            product("Consulting", "hour", 12_000, 2100),
            product("Consulting retainer", "month", 250_000, 2100),
            product("Travel", "km", 42, 900),
        ]
    }

    #[test]
    fn a_decimal_quantity_is_read_by_its_digits() {
        assert_eq!(milli_from_decimal("1"), Some(1_000));
        assert_eq!(milli_from_decimal("1.5"), Some(1_500));
        assert_eq!(milli_from_decimal("0.001"), Some(1));
        assert_eq!(milli_from_decimal(" 12.25 "), Some(12_250));
        assert_eq!(milli_from_decimal("-2.5"), Some(-2_500));
        assert_eq!(milli_from_decimal("+3"), Some(3_000));
        assert_eq!(milli_from_decimal(".5"), Some(500));
        assert_eq!(milli_from_decimal("2."), Some(2_000));
        assert_eq!(milli_from_decimal("0"), Some(0));
        // A trailing zero is not extra precision.
        assert_eq!(milli_from_decimal("1.50"), Some(1_500));
        assert_eq!(milli_from_decimal("1.500"), Some(1_500));
    }

    #[test]
    fn anything_that_is_not_a_plain_decimal_is_refused_never_rounded() {
        for bad in [
            "1.2345", // finer than a thousandth
            "1e3",    // an exponent
            "1,5",    // a separator we do not read
            "1 000",  // grouped
            "abc", "", " ", "-", ".", "--1", "1.-5", "٣", // digits, but not ASCII ones
        ] {
            assert_eq!(milli_from_decimal(bad), None, "{bad:?} must be refused");
        }
        // Beyond the store's cap is a typo, not a big order.
        assert_eq!(milli_from_decimal("1000000"), Some(QTY_MAX_MILLI));
        assert_eq!(milli_from_decimal("1000000.001"), None);
        assert_eq!(milli_from_decimal("999999999999"), None);
    }

    #[test]
    fn a_quantity_defaults_to_one_and_never_to_nothing() {
        assert_eq!(quantity_milli(None), Ok(1_000));
        assert_eq!(quantity_milli(Some(&Value::Null)), Ok(1_000));
        assert_eq!(quantity_milli(Some(&json!(2))), Ok(2_000));
        assert_eq!(quantity_milli(Some(&json!(1.5))), Ok(1_500));
        assert_eq!(quantity_milli(Some(&json!("0.25"))), Ok(250));
        assert_eq!(
            quantity_milli(Some(&json!(-1))),
            Ok(-1_000),
            "a discount line"
        );
        assert!(quantity_milli(Some(&json!(0))).is_err());
        assert!(quantity_milli(Some(&json!(true))).is_err());
        assert!(quantity_milli(Some(&json!("lots"))).is_err());
    }

    #[test]
    fn a_line_from_the_price_list_carries_the_price_list_price() {
        let line = new_line(
            &json!({ "product": "consulting", "quantity": 7.5 }),
            &catalogue(),
        )
        .expect("an exact name wins over the longer one that contains it");
        assert_eq!(line.description, "Consulting");
        assert_eq!(line.unit, "hour");
        assert_eq!(line.qty_milli, 7_500);
        assert_eq!(line.unit_price_cents, 12_000);
        assert_eq!(line.vat_rate_bp, 2100);
        // A stated description overrides the product's name; the money does not
        // move with it.
        let renamed = new_line(
            &json!({ "product": "Travel", "description": "Site visit, Berlin", "quantity": 120 }),
            &catalogue(),
        )
        .unwrap();
        assert_eq!(renamed.description, "Site visit, Berlin");
        assert_eq!(renamed.unit_price_cents, 42);
        assert_eq!(renamed.qty_milli, 120_000);
    }

    #[test]
    fn a_line_may_state_a_product_or_a_price_but_never_both() {
        let why = new_line(
            &json!({ "product": "Travel", "unitPriceCents": 99, "quantity": 1 }),
            &catalogue(),
        )
        .unwrap_err();
        assert!(why.contains("not both"), "{why}");
        let why = new_line(
            &json!({ "product": "Travel", "vatRateBp": 2100 }),
            &catalogue(),
        )
        .unwrap_err();
        assert!(why.contains("not both"), "{why}");
    }

    #[test]
    fn a_free_line_needs_every_figure_it_cannot_look_up() {
        let line = new_line(
            &json!({ "description": "Rush surcharge", "unit": "piece",
                     "quantity": 1, "unitPriceCents": 15_000, "vatRateBp": 2100 }),
            &catalogue(),
        )
        .unwrap();
        assert_eq!(line.description, "Rush surcharge");
        assert_eq!(line.unit_price_cents, 15_000);

        for (line, hint) in [
            (json!({ "quantity": 1 }), "product or a description"),
            (
                json!({ "description": "X", "vatRateBp": 2100 }),
                "needs unitPriceCents",
            ),
            (
                json!({ "description": "X", "unitPriceCents": 100 }),
                "needs vatRateBp",
            ),
        ] {
            let why = new_line(&line, &catalogue()).unwrap_err();
            assert!(why.contains(hint), "{why}");
        }
        // A VAT rate that is not one is refused before it reaches the store.
        let why = new_line(
            &json!({ "description": "X", "unitPriceCents": 100, "vatRateBp": 9_999_999_999i64 }),
            &catalogue(),
        )
        .unwrap_err();
        assert!(why.contains("not a VAT rate"), "{why}");
    }

    #[test]
    fn a_product_name_resolves_through_the_shared_rule() {
        // The rule itself is `agent_args`' (exact first, then a unique
        // containment, an ambiguity listed); what this asserts is that a *line*
        // resolves its product by it, and carries that record's own price.
        let line = new_line(&json!({ "product": "retainer" }), &catalogue())
            .expect("one product contains 'retainer'");
        assert_eq!(line.description, "Consulting retainer");
        assert_eq!(line.unit_price_cents, 250_000);
        let why = new_line(&json!({ "product": "consult" }), &catalogue()).unwrap_err();
        assert!(why.contains("Consulting, Consulting retainer"), "{why}");
    }

    #[test]
    fn the_refusal_for_an_unknown_document_number_says_which_kind() {
        let problem = no_such("invoice", "INV-2026-09999");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            problem.detail.as_deref(),
            Some("no invoice of yours is numbered INV-2026-09999")
        );
    }
}
