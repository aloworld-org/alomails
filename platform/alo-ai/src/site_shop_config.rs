//! The shop configuration proposal — ADR 0041's setup screen, engine half.
//!
//! Odoo loses the customers who cannot afford a consultant on the settings
//! form: product types, fiscal positions, delivery carriers, all blank fields
//! somebody must already understand. alo's move is to propose the whole
//! configuration from one sentence about the business — the catalog items,
//! one VAT treatment per item, the delivery rate — and let the owner approve
//! it. Not fewer settings: the same settings, already answered, shown for
//! confirmation with every guess flagged.
//!
//! This module owns the prompt and the strict envelope parser, nothing else.
//! Parsing never writes: applying an approved proposal is the approval
//! screen's job, and it goes through the owned Billing product and shop
//! routes only — this crate has no store handle to misuse.
//!
//! The flags are the item, so the parser enforces them rather than trusting
//! the model to volunteer them:
//!
//! - **A price is either stated or absent.** `price_cents` is accepted only
//!   when that exact amount appears in the business description itself (all
//!   the European ways of writing it — `19,50`, `19.50`, `1.950` — count).
//!   Any other number is an invented price and the whole proposal is
//!   refused; an unstated price must be `null`, which parses to
//!   [`ProposedPrice::NeedsInput`] — a blank the owner fills in, flagged.
//! - **VAT is structurally a guess.** The parsed type is [`VatGuess`] and
//!   there is no field that could mark it confirmed; a rendering of this
//!   envelope cannot show a VAT rate as settled. The rate is bounded to
//!   plausible basis points and must arrive with a one-sentence basis the
//!   owner's accountant can judge.
//! - **Shipping follows the goods.** A delivery rate is only accepted when
//!   at least one proposed item is physical stock; stock with no stated rate
//!   parses to [`ProposedShipping::NeedsInput`], and a proposal with nothing
//!   to ship must carry no rate at all.
//!
//! What each kind becomes when approved (through owned doors, later): a
//! `stock` item is a Billing product with `stocked = true`, counted by the
//! Inventory ledger and sold with the site's flat delivery rate; a `dated`
//! item is an event sold as tickets, the calendar being its inventory; a
//! `service` is an undated price-list line. Amounts are integer cents in the
//! tenant's accounting currency — the proposal never names a currency of its
//! own.

use std::collections::HashSet;
use std::future::Future;

use serde::{Deserialize, Serialize};

use crate::agent::extract_json;
use crate::{AiConfig, ChatMessage, InferenceError, chat};

/// Current version of the shop configuration proposal envelope.
pub const SHOP_CONFIG_SCHEMA_VERSION: u64 = 1;

/// A proposal is a reviewable list, not a dump; forty rows is already a long
/// approval screen.
const MAX_PROPOSED_ITEMS: usize = 40;
const ITEM_NAME_MAX_CHARS: usize = 120;
const UNIT_MAX_CHARS: usize = 40;
const VAT_BASIS_MAX_CHARS: usize = 200;
const NOTE_MAX_CHARS: usize = 300;
/// One million euros for one unit is not a shop item; it is a typo.
const MAX_AMOUNT_CENTS: i64 = 100_000_000;
/// EU VAT rates top out at Hungary's 27 %; 30 % leaves headroom without
/// accepting a percentage that was really a price.
const MAX_VAT_RATE_BP: i32 = 3_000;

/// What one proposed catalog item will become when approved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProposedItemKind {
    /// Physical goods kept on a shelf and shipped — a Billing product with
    /// `stocked = true`, availability counted by the Inventory ledger.
    Stock,
    /// An event, workshop, class, or anything else sold for a moment in
    /// time — tickets, with the calendar as the inventory.
    Dated,
    /// Undated work sold by time or by job — a price-list line with no
    /// quantity and nothing to ship.
    Service,
}

/// A proposed unit price: the description's own figure, or a flagged blank.
/// Never a number the model invented — the parser refuses those outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ProposedPrice {
    /// The description states this amount (integer cents, tenant currency).
    Stated {
        /// The stated amount in integer cents.
        cents: i64,
    },
    /// The description names the item but not its price; the approval screen
    /// shows a blank the owner must fill in before this row can be applied.
    NeedsInput,
}

/// A proposed VAT treatment. The type is named for what it always is: a
/// guess. There is deliberately no way to represent a *confirmed* rate in
/// this envelope — confirmation happens on the approval screen, by a human,
/// and applying an unconfirmed rate is that screen's bug, not this type's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VatGuess {
    /// Proposed rate in basis points (2100 = 21 %).
    pub rate_bp: i32,
    /// One sentence naming the rate and why it plausibly applies, for the
    /// owner's accountant to judge ("Belgian reduced rate for printed
    /// books").
    pub basis: String,
}

/// One proposed catalog item, ready for the approval screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposedShopItem {
    /// What the item is called on the price list, and so on the shelf.
    pub name: String,
    /// What approving this row creates.
    pub kind: ProposedItemKind,
    /// Unit label ("piece", "seat", "hour"); empty for a unitless item.
    pub unit: String,
    /// The stated price, or the flagged blank.
    pub price: ProposedPrice,
    /// The VAT treatment — always presented as a guess to confirm; the wire
    /// name says so too.
    #[serde(rename = "vat_guess")]
    pub vat: VatGuess,
    /// One short remark for the review screen, when the model had one.
    pub note: Option<String>,
}

/// The proposed flat per-order delivery price for the shop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ProposedShipping {
    /// Nothing in the proposal ships, so there is nothing to charge for.
    NotNeeded,
    /// The description states the rate (`0` when it says delivery is free).
    Stated {
        /// The stated flat rate in integer cents.
        cents: i64,
    },
    /// Goods ship but the description names no rate; a flagged blank.
    NeedsInput,
}

/// A complete proposed shop configuration. Parsing this value never creates
/// anything; the approval screen applies it through the owned Billing and
/// shop routes after the owner has confirmed every guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShopConfigProposal {
    /// Envelope version, [`SHOP_CONFIG_SCHEMA_VERSION`].
    pub schema_version: u64,
    /// The proposed catalog, in the model's presentation order.
    pub items: Vec<ProposedShopItem>,
    /// The proposed delivery treatment.
    pub shipping: ProposedShipping,
    /// One short delivery remark for the review screen, when there was one.
    pub shipping_note: Option<String>,
}

/// Why a model response could not become a safe configuration proposal.
#[derive(Debug, thiserror::Error)]
pub enum ShopConfigError {
    /// The backend call itself failed; never retried here.
    #[error(transparent)]
    Inference(#[from] InferenceError),
    /// The reply did not contain one JSON object.
    #[error("shop configuration response did not contain one JSON object")]
    MissingObject,
    /// The reply speaks a version this build does not.
    #[error(
        "unsupported shop configuration schema_version {0} (this build speaks {SHOP_CONFIG_SCHEMA_VERSION})"
    )]
    UnsupportedVersion(u64),
    /// The JSON shape did not match the closed schema.
    #[error("shop configuration JSON does not match schema v{SHOP_CONFIG_SCHEMA_VERSION}: {0}")]
    Shape(#[from] serde_json::Error),
    /// The shape matched but a rule refused it.
    #[error("shop configuration proposal is invalid: {0}")]
    Invalid(String),
    /// The one repair turn produced another refused proposal.
    #[error("shop configuration proposal was still invalid after one repair: {0}")]
    RepairFailed(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawShopConfig {
    schema_version: u64,
    items: Vec<RawItem>,
    shipping_rate_cents: Option<i64>,
    shipping_note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawItem {
    name: String,
    kind: RawKind,
    unit: String,
    price_cents: Option<i64>,
    vat_rate_bp: i32,
    vat_basis: String,
    note: Option<String>,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum RawKind {
    Stock,
    Dated,
    Service,
}

const SHOP_CONFIG_SYSTEM: &str = r#"You propose ONE shop configuration for alo Commerce from a business description: the catalog items, one VAT treatment per item, and the flat delivery rate. The owner reviews and approves the proposal; nothing you write is applied automatically. Reply with a SINGLE JSON object and nothing else: no prose, no markdown, no code fences.

Schema (all unknown fields are forbidden):
{"schema_version":1,"items":[item,...],"shipping_rate_cents":integer|null,"shipping_note":string|null}
- items: 1-40 catalog items with unique names, one per thing the business sells.
- item: {"name":string,"kind":"stock"|"dated"|"service","unit":string,"price_cents":integer|null,"vat_rate_bp":integer,"vat_basis":string,"note":string|null}
- kind: "stock" = physical goods kept on a shelf and shipped; "dated" = an event, workshop, class, or ticket for a moment in time; "service" = undated work sold by time or by job.
- unit: how one is counted ("piece", "seat", "hour"); "" for a unitless item.
- price_cents: the price of one unit in integer cents, ONLY when the description states that exact price; otherwise null. NEVER invent a price - a null is shown to the owner as a blank to fill in, while an invented number gets the whole proposal refused.
- vat_rate_bp: the most plausible VAT rate in basis points (2100 = 21%) for this item in the seller's country, judged from the description; when the description does not name or clearly imply a country, propose a common standard rate and say so in vat_basis. Every VAT proposal is presented to the owner as a guess their accountant must confirm.
- vat_basis: one short sentence naming the rate and why it plausibly applies ("Belgian reduced rate for printed books"). Required.
- shipping_rate_cents: the flat per-order delivery price in integer cents, ONLY when the description states it (0 when it says delivery is free); otherwise null. Only "stock" items ship: when the proposal has none, this must be null.
- shipping_note / note: one short remark for the review screen, or null.

Write names and remarks in the language of the description. Do not invent prices, delivery rates, products the description does not sell, or facts about the business. Output ONLY the JSON object."#;

/// Builds the two-message proposal conversation. Pure and fixture-testable;
/// no backend call is made here.
#[must_use]
pub fn shop_config_messages(description: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".to_owned(),
            content: SHOP_CONFIG_SYSTEM.to_owned(),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: format!("Business description:\n{}", description.trim()),
        },
    ]
}

/// Adds the model's refused reply and the validator's own reason to the base
/// conversation. The wording explicitly grants one correction, not a fresh
/// creative attempt.
#[must_use]
pub fn shop_config_repair_messages(
    base: &[ChatMessage],
    reply: &str,
    refusal: &ShopConfigError,
) -> Vec<ChatMessage> {
    const MAX_REFUSAL_CHARS: usize = 1_000;

    let refusal: String = refusal
        .to_string()
        .chars()
        .take(MAX_REFUSAL_CHARS)
        .collect();
    let mut messages = base.to_vec();
    messages.push(ChatMessage {
        role: "assistant".to_owned(),
        content: reply.trim().to_owned(),
    });
    messages.push(ChatMessage {
        role: "user".to_owned(),
        content: format!(
            "That proposal was refused by the shop configuration schema: {refusal}\n\
             Correct only the refused fields. Reply with ONE complete corrected configuration \
             JSON object and nothing else. This is your only repair attempt."
        ),
    });
    messages
}

/// Generates and validates one shop configuration proposal, with exactly one
/// schema-repair attempt. Transport/configuration failures are not retried;
/// only a well-formed model response that the schema refuses earns the
/// correction turn.
///
/// This function never persists anything. Callers present the proposal for
/// approval; applying it goes through the owned Billing and shop routes.
///
/// # Errors
/// [`ShopConfigError`] when the backend fails or both attempts are refused.
pub async fn propose_shop_config(
    config: &AiConfig,
    description: &str,
) -> Result<ShopConfigProposal, ShopConfigError> {
    propose_shop_config_with(description, |messages| async move {
        chat(config, &messages, 0.2).await
    })
    .await
}

async fn propose_shop_config_with<T, F>(
    description: &str,
    mut turn: T,
) -> Result<ShopConfigProposal, ShopConfigError>
where
    T: FnMut(Vec<ChatMessage>) -> F,
    F: Future<Output = Result<String, InferenceError>>,
{
    let base = shop_config_messages(description);
    let first = turn(base.clone()).await?;
    let refusal = match parse_shop_config(description, &first) {
        Ok(proposal) => return Ok(proposal),
        Err(error) => error,
    };

    let repair = shop_config_repair_messages(&base, &first, &refusal);
    let second = turn(repair).await?;
    parse_shop_config(description, &second)
        .map_err(|error| ShopConfigError::RepairFailed(error.to_string()))
}

/// Parses one proposed configuration through the complete, closed v1 schema.
///
/// The description the proposal was drafted from is part of the contract: it
/// is the only source a stated amount may come from. A surrounding fence or
/// preamble is tolerated because the extracted object is still validated
/// strictly; unknown fields, unknown kinds, invented amounts, out-of-range
/// VAT, duplicate names, and shipping on a proposal with nothing to ship are
/// all refused before any caller sees the value.
///
/// # Errors
/// [`ShopConfigError`] naming the first refused field.
pub fn parse_shop_config(
    description: &str,
    text: &str,
) -> Result<ShopConfigProposal, ShopConfigError> {
    let json = extract_json(text).ok_or(ShopConfigError::MissingObject)?;
    let value: serde_json::Value = serde_json::from_str(json)?;
    if !value.is_object() {
        return Err(ShopConfigError::MissingObject);
    }
    if let Some(version) = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        && version != SHOP_CONFIG_SCHEMA_VERSION
    {
        return Err(ShopConfigError::UnsupportedVersion(version));
    }

    let raw: RawShopConfig = serde_json::from_value(value)?;
    if raw.schema_version != SHOP_CONFIG_SCHEMA_VERSION {
        return Err(ShopConfigError::UnsupportedVersion(raw.schema_version));
    }
    if raw.items.is_empty() || raw.items.len() > MAX_PROPOSED_ITEMS {
        return Err(invalid(format!(
            "items must contain 1-{MAX_PROPOSED_ITEMS} items"
        )));
    }

    let stated = stated_amounts_cents(description);
    let mut names = HashSet::with_capacity(raw.items.len());
    let mut items = Vec::with_capacity(raw.items.len());
    for item in raw.items {
        check_text("item.name", &item.name, ITEM_NAME_MAX_CHARS)?;
        if !names.insert(item.name.to_lowercase()) {
            return Err(invalid(format!(
                "item.name \"{}\" is proposed twice; names must be unique",
                item.name
            )));
        }
        if !item.unit.is_empty() {
            check_text("item.unit", &item.unit, UNIT_MAX_CHARS)?;
        }
        let price = match item.price_cents {
            None => ProposedPrice::NeedsInput,
            Some(cents) => {
                check_amount("item.price_cents", cents)?;
                if !stated.contains(&cents) {
                    return Err(invalid(format!(
                        "item \"{}\": price_cents {cents} is not stated in the business \
                         description; a price the model invented may not be proposed - use null",
                        item.name
                    )));
                }
                ProposedPrice::Stated { cents }
            }
        };
        if !(0..=MAX_VAT_RATE_BP).contains(&item.vat_rate_bp) {
            return Err(invalid(format!(
                "item \"{}\": vat_rate_bp must be 0-{MAX_VAT_RATE_BP} basis points",
                item.name
            )));
        }
        check_text("item.vat_basis", &item.vat_basis, VAT_BASIS_MAX_CHARS)?;
        check_optional_text("item.note", item.note.as_deref(), NOTE_MAX_CHARS)?;
        items.push(ProposedShopItem {
            name: item.name,
            kind: match item.kind {
                RawKind::Stock => ProposedItemKind::Stock,
                RawKind::Dated => ProposedItemKind::Dated,
                RawKind::Service => ProposedItemKind::Service,
            },
            unit: item.unit,
            price,
            vat: VatGuess {
                rate_bp: item.vat_rate_bp,
                basis: item.vat_basis,
            },
            note: item.note,
        });
    }

    let ships = items
        .iter()
        .any(|item| item.kind == ProposedItemKind::Stock);
    let shipping = match (ships, raw.shipping_rate_cents) {
        (false, Some(_)) => {
            return Err(invalid(
                "shipping_rate_cents: nothing in this proposal ships; use null".to_owned(),
            ));
        }
        (false, None) => ProposedShipping::NotNeeded,
        (true, None) => ProposedShipping::NeedsInput,
        (true, Some(cents)) => {
            check_amount("shipping_rate_cents", cents)?;
            if cents != 0 && !stated.contains(&cents) {
                return Err(invalid(format!(
                    "shipping_rate_cents {cents} is not stated in the business description; \
                     a delivery rate the model invented may not be proposed - use null"
                )));
            }
            ProposedShipping::Stated { cents }
        }
    };
    check_optional_text(
        "shipping_note",
        raw.shipping_note.as_deref(),
        NOTE_MAX_CHARS,
    )?;

    Ok(ShopConfigProposal {
        schema_version: raw.schema_version,
        items,
        shipping,
        shipping_note: raw.shipping_note,
    })
}

/// Every amount, in integer cents, that the description itself states — the
/// closed set a proposed price may come from. Recognises the forms a European
/// owner actually types: `60`, `19.50`, `19,50`, `1.950`, `1.234,56`.
fn stated_amounts_cents(description: &str) -> HashSet<i64> {
    let mut amounts = HashSet::new();
    let chars: Vec<char> = description.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == ',') {
            i += 1;
        }
        let mut token = &chars[start..i];
        while let Some((last, rest)) = token.split_last()
            && !last.is_ascii_digit()
        {
            token = rest;
        }
        if let Some(cents) = token_cents(token) {
            amounts.insert(cents);
        }
    }
    amounts
}

/// One digit-and-separator token as integer cents, or `None` when it is not
/// a plausible amount. A final group of one or two digits after a separator
/// is a decimal part; any other separator is a thousands separator.
fn token_cents(token: &[char]) -> Option<i64> {
    let text: String = token.iter().collect();
    let groups: Vec<&str> = text.split(['.', ',']).collect();
    if groups.iter().any(|group| group.is_empty()) {
        return None;
    }
    let (whole_groups, frac) = match groups.split_last() {
        Some((last, rest)) if !rest.is_empty() && last.len() <= 2 => (rest, Some(*last)),
        _ => (&groups[..], None),
    };
    let whole = whole_groups.concat();
    if whole.len() > 9 {
        return None;
    }
    let whole: i64 = whole.parse().ok()?;
    let frac_cents = match frac {
        None => 0,
        Some(frac) if frac.len() == 1 => frac.parse::<i64>().ok()? * 10,
        Some(frac) => frac.parse::<i64>().ok()?,
    };
    whole.checked_mul(100)?.checked_add(frac_cents)
}

fn check_amount(field: &str, cents: i64) -> Result<(), ShopConfigError> {
    if !(0..=MAX_AMOUNT_CENTS).contains(&cents) {
        return Err(invalid(format!(
            "{field} must be 0-{MAX_AMOUNT_CENTS} integer cents"
        )));
    }
    Ok(())
}

fn check_text(field: &str, value: &str, cap: usize) -> Result<(), ShopConfigError> {
    if value.trim().is_empty() {
        return Err(invalid(format!("{field} must not be empty")));
    }
    if value != value.trim() {
        return Err(invalid(format!(
            "{field} must not have surrounding whitespace"
        )));
    }
    if value.chars().count() > cap {
        return Err(invalid(format!("{field} must be at most {cap} characters")));
    }
    Ok(())
}

fn check_optional_text(
    field: &str,
    value: Option<&str>,
    cap: usize,
) -> Result<(), ShopConfigError> {
    if let Some(value) = value {
        check_text(field, value, cap)?;
    }
    Ok(())
}

fn invalid(detail: String) -> ShopConfigError {
    ShopConfigError::Invalid(detail)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::future::ready;

    const VALID: &str = include_str!("../tests/fixtures/sites/valid_shop_config.json");
    const NEAR_MISS_PRICE: &str =
        include_str!("../tests/fixtures/sites/near_miss_invented_price.json");

    /// ADR 0041's own example, in one sentence.
    const DESCRIPTION: &str = "I run pottery workshops in Antwerp and sell two books: \
         Glaze Basics at €25 and Wheel Notes at €19,50. A workshop seat is €60. \
         Shipping is €5 per order.";

    #[test]
    fn fixture_is_a_complete_strict_proposal() {
        let proposal = parse_shop_config(DESCRIPTION, VALID).unwrap();
        assert_eq!(proposal.items.len(), 3);
        assert_eq!(proposal.items[0].kind, ProposedItemKind::Stock);
        assert_eq!(
            proposal.items[0].price,
            ProposedPrice::Stated { cents: 2500 }
        );
        assert_eq!(
            proposal.items[1].price,
            ProposedPrice::Stated { cents: 1950 }
        );
        assert_eq!(proposal.items[2].kind, ProposedItemKind::Dated);
        assert_eq!(
            proposal.items[2].price,
            ProposedPrice::Stated { cents: 6000 }
        );
        assert_eq!(proposal.shipping, ProposedShipping::Stated { cents: 500 });
        assert!(
            proposal.items.iter().all(|item| !item.vat.basis.is_empty()),
            "every VAT guess arrives with its basis"
        );
    }

    #[test]
    fn prompt_documents_the_kinds_and_the_never_invent_rules() {
        let prompt = &shop_config_messages("a bookshop")[0].content;
        for kind in ["\"stock\"", "\"dated\"", "\"service\""] {
            assert!(prompt.contains(kind), "missing kind {kind}");
        }
        for rule in [
            "NEVER invent a price",
            "vat_basis",
            "shipping_rate_cents",
            "guess",
            "nothing you write is applied automatically",
        ] {
            assert!(prompt.contains(rule), "missing rule {rule}");
        }
        assert!(prompt.ends_with("Output ONLY the JSON object."));
    }

    #[test]
    fn description_is_trimmed_but_never_mixed_into_the_system_contract() {
        let messages = shop_config_messages("  A quiet bookshop.  ");
        assert_eq!(messages.len(), 2);
        assert_eq!(
            messages[1].content,
            "Business description:\nA quiet bookshop."
        );
        assert!(!messages[0].content.contains("A quiet bookshop"));
    }

    #[test]
    fn unknown_fields_and_kinds_are_refused() {
        let unknown_top = VALID.replacen(
            "\"schema_version\": 1,",
            "\"schema_version\": 1, \"surprise\": true,",
            1,
        );
        assert!(matches!(
            parse_shop_config(DESCRIPTION, &unknown_top),
            Err(ShopConfigError::Shape(_))
        ));

        let unknown_kind = VALID.replacen("\"kind\": \"dated\"", "\"kind\": \"bundle\"", 1);
        assert!(matches!(
            parse_shop_config(DESCRIPTION, &unknown_kind),
            Err(ShopConfigError::Shape(_))
        ));
    }

    /// The item's core rule: a price the description does not state is an
    /// invented price, and the proposal is refused rather than flagged —
    /// while an honest null parses to a flagged blank.
    #[test]
    fn an_invented_price_is_refused_and_a_null_is_flagged_for_input() {
        let error = parse_shop_config(DESCRIPTION, NEAR_MISS_PRICE)
            .expect_err("an invented price must refuse the whole proposal");
        assert!(
            matches!(&error, ShopConfigError::Invalid(detail)
                if detail.contains("not stated in the business description")),
            "{error}"
        );

        let blank = VALID.replacen("\"price_cents\": 2500", "\"price_cents\": null", 1);
        let proposal = parse_shop_config(DESCRIPTION, &blank).unwrap();
        assert_eq!(proposal.items[0].price, ProposedPrice::NeedsInput);
    }

    #[test]
    fn european_number_forms_all_state_the_same_amounts() {
        let stated = stated_amounts_cents("prices: 60, 19,50, 19.50, 1.950 and 1.234,56 euros.");
        for cents in [6000, 1950, 195_000, 123_456] {
            assert!(stated.contains(&cents), "missing {cents}");
        }
        assert!(
            !stated.contains(&1900),
            "a decimal form never doubles as a smaller integer"
        );
    }

    #[test]
    fn vat_bounds_and_basis_are_enforced() {
        let too_high = VALID.replacen("\"vat_rate_bp\": 2100", "\"vat_rate_bp\": 3100", 1);
        assert!(matches!(
            parse_shop_config(DESCRIPTION, &too_high),
            Err(ShopConfigError::Invalid(detail)) if detail.contains("vat_rate_bp")
        ));

        let blank_basis = VALID.replacen(
            "\"vat_basis\": \"Belgian standard rate for workshop admission\"",
            "\"vat_basis\": \"\"",
            1,
        );
        assert!(matches!(
            parse_shop_config(DESCRIPTION, &blank_basis),
            Err(ShopConfigError::Invalid(detail)) if detail.contains("vat_basis")
        ));
    }

    /// The wire shape the approval screen will rely on: the VAT key itself
    /// says "guess", and prices/shipping are tagged states, so a renderer
    /// cannot read a flagged blank as a settled number by accident.
    #[test]
    fn the_serialized_proposal_carries_its_flags() {
        let proposal = parse_shop_config(DESCRIPTION, VALID).unwrap();
        let value = serde_json::to_value(&proposal).unwrap();
        assert_eq!(
            value["items"][0]["price"],
            serde_json::json!({"state": "stated", "cents": 2500})
        );
        assert_eq!(value["items"][0]["vat_guess"]["rate_bp"], 600);
        assert_eq!(
            value["shipping"],
            serde_json::json!({"state": "stated", "cents": 500})
        );

        let blank = VALID.replacen("\"price_cents\": 2500", "\"price_cents\": null", 1);
        let flagged = parse_shop_config(DESCRIPTION, &blank).unwrap();
        let flagged = serde_json::to_value(&flagged).unwrap();
        assert_eq!(
            flagged["items"][0]["price"],
            serde_json::json!({"state": "needs_input"})
        );
    }

    #[test]
    fn shipping_follows_the_goods_being_shippable() {
        let all_dated = VALID.replace("\"kind\": \"stock\"", "\"kind\": \"dated\"");
        let error = parse_shop_config(DESCRIPTION, &all_dated)
            .expect_err("a rate with nothing to ship must be refused");
        assert!(
            matches!(&error, ShopConfigError::Invalid(detail)
                if detail.contains("nothing in this proposal ships")),
            "{error}"
        );

        let nothing_ships = all_dated.replacen(
            "\"shipping_rate_cents\": 500",
            "\"shipping_rate_cents\": null",
            1,
        );
        let proposal = parse_shop_config(DESCRIPTION, &nothing_ships).unwrap();
        assert_eq!(proposal.shipping, ProposedShipping::NotNeeded);

        let unstated = VALID.replacen(
            "\"shipping_rate_cents\": 500",
            "\"shipping_rate_cents\": null",
            1,
        );
        let proposal = parse_shop_config(DESCRIPTION, &unstated).unwrap();
        assert_eq!(proposal.shipping, ProposedShipping::NeedsInput);

        let free = VALID.replacen(
            "\"shipping_rate_cents\": 500",
            "\"shipping_rate_cents\": 0",
            1,
        );
        let proposal = parse_shop_config(DESCRIPTION, &free).unwrap();
        assert_eq!(proposal.shipping, ProposedShipping::Stated { cents: 0 });
    }

    #[test]
    fn duplicate_names_are_refused() {
        let duplicate =
            VALID.replacen("\"name\": \"Wheel Notes\"", "\"name\": \"glaze basics\"", 1);
        assert!(matches!(
            parse_shop_config(DESCRIPTION, &duplicate),
            Err(ShopConfigError::Invalid(detail)) if detail.contains("unique")
        ));
    }

    #[test]
    fn fences_are_tolerated_but_versions_and_non_objects_are_not() {
        let fenced = format!("Here is the proposal:\n```json\n{VALID}\n```");
        assert!(parse_shop_config(DESCRIPTION, &fenced).is_ok());
        assert!(matches!(
            parse_shop_config(DESCRIPTION, "[]"),
            Err(ShopConfigError::MissingObject)
        ));
        let future = VALID.replacen("\"schema_version\": 1", "\"schema_version\": 9", 1);
        assert!(matches!(
            parse_shop_config(DESCRIPTION, &future),
            Err(ShopConfigError::UnsupportedVersion(9))
        ));
    }

    #[test]
    fn repair_conversation_keeps_the_base_and_names_the_refusal() {
        let base = shop_config_messages(DESCRIPTION);
        let refusal = parse_shop_config(DESCRIPTION, NEAR_MISS_PRICE).unwrap_err();
        let repaired = shop_config_repair_messages(&base, NEAR_MISS_PRICE, &refusal);

        assert_eq!(repaired.len(), 4);
        assert_eq!(repaired[0].content, base[0].content);
        assert_eq!(repaired[1].content, base[1].content);
        assert_eq!(repaired[2].role, "assistant");
        assert_eq!(repaired[2].content, NEAR_MISS_PRICE.trim());
        assert_eq!(repaired[3].role, "user");
        assert!(
            repaired[3]
                .content
                .contains("not stated in the business description")
        );
        assert!(repaired[3].content.contains("only repair attempt"));
    }

    #[tokio::test]
    async fn a_near_miss_gets_one_repair_and_returns_the_corrected_fixture() {
        let replies = RefCell::new(VecDeque::from([
            NEAR_MISS_PRICE.to_owned(),
            VALID.to_owned(),
        ]));
        let conversations = RefCell::new(Vec::new());

        let proposal = propose_shop_config_with(DESCRIPTION, |messages| {
            conversations.borrow_mut().push(messages);
            ready(Ok(replies.borrow_mut().pop_front().unwrap()))
        })
        .await
        .unwrap();

        assert_eq!(proposal.items.len(), 3);
        let conversations = conversations.into_inner();
        assert_eq!(conversations.len(), 2, "one repair, never two");
        assert_eq!(conversations[0].len(), 2);
        assert_eq!(conversations[1].len(), 4);
        assert!(replies.into_inner().is_empty());
    }

    #[tokio::test]
    async fn a_second_refusal_is_typed_and_never_gets_a_third_turn() {
        let replies = RefCell::new(VecDeque::from([
            NEAR_MISS_PRICE.to_owned(),
            NEAR_MISS_PRICE.to_owned(),
            VALID.to_owned(),
        ]));
        let turns = RefCell::new(0_u8);

        let error = propose_shop_config_with(DESCRIPTION, |_| {
            *turns.borrow_mut() += 1;
            ready(Ok(replies.borrow_mut().pop_front().unwrap()))
        })
        .await
        .unwrap_err();

        assert!(matches!(error, ShopConfigError::RepairFailed(_)));
        assert_eq!(turns.into_inner(), 2, "a third model turn is forbidden");
        assert_eq!(
            replies.into_inner().len(),
            1,
            "the third fixture stays unused"
        );
    }

    #[tokio::test]
    async fn inference_failures_are_not_retried() {
        let turns = RefCell::new(0_u8);
        let error = propose_shop_config_with(DESCRIPTION, |_| {
            *turns.borrow_mut() += 1;
            ready(Err(InferenceError::NotConfigured))
        })
        .await
        .unwrap_err();

        assert!(matches!(
            error,
            ShopConfigError::Inference(InferenceError::NotConfigured)
        ));
        assert_eq!(turns.into_inner(), 1);
    }
}
