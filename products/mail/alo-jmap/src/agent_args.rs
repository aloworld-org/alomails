//! The arguments of an approved agent proposal, and the one rule every product
//! agent needs before it can act: **the model speaks names, the store speaks
//! ids** (ADR 0034).
//!
//! A proposal carries "the customer Acme", "the deal Renewal", "the board
//! Sales" — whatever the user said. Turning one of those into exactly one of
//! the tenant's records is the same problem in billing, in CRM, and in every
//! wave after them, and getting it wrong means acting on the wrong record.
//! Doing it once here is what keeps the two answers from drifting apart:
//!
//! - an **exact** name (case- and blank-insensitive) always wins, so a customer
//!   literally called "Acme" is reachable even when "Acme Holding BV" exists;
//! - failing that, a name that appears inside exactly **one** record's name is
//!   taken — people say "Acme", not "Acme Handelsgesellschaft mbH";
//! - two matches is a refusal that **lists them**, never a guess: an agent that
//!   picked one would eventually invoice the wrong company, and a document sent
//!   to the wrong party cannot be unsent.
//!
//! Nothing here reads or writes anything; it is the vocabulary the executors
//! ([`crate::agent_billing`], [`crate::agent_crm`]) share.

use axum::http::StatusCode;
use serde_json::Value;

use crate::error::Problem;

/// A trimmed, non-empty string argument, or `None` when the proposal did not
/// state one (a blank string is not a statement).
pub fn string_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// The `422` a proposal that cannot be carried out earns.
///
/// A `422` rather than a `404` throughout: the route exists and the request is
/// well-formed — it is the *name in it* that resolves to nothing, and answering
/// two ways would make the same class of mistake read as two different faults.
pub fn unprocessable(detail: impl Into<String>) -> Problem {
    Problem::with(StatusCode::UNPROCESSABLE_ENTITY, detail.into())
}

/// A whole-number argument, or the reason it is not one. Absent is `None`;
/// present but fractional, textual or out of range is an error, **never a
/// rounding** — a price is not a thing to round on the way in, and neither is
/// what somebody thinks a deal is worth.
pub fn integer(value: Option<&Value>, key: &str) -> Result<Option<i64>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_i64()
            .map(Some)
            .ok_or_else(|| format!("{key} must be a whole number of cents, not {n}")),
        Some(other) => Err(format!("{key} must be a whole number, not {other}")),
    }
}

/// Resolves a name the user said to exactly one of the tenant's records, or the
/// `422` that says why it could not.
pub fn pick<T>(wanted: &str, candidates: Vec<(&str, T)>, kind: &str) -> Result<T, Problem> {
    pick_name(wanted, candidates, kind).map_err(unprocessable)
}

/// The same resolution, reporting plain text — the form a line's error takes,
/// so a caller can prefix it with the line's position.
///
/// See the module docs for the rule; the tests below are its statement.
pub fn pick_name<T>(wanted: &str, candidates: Vec<(&str, T)>, kind: &str) -> Result<T, String> {
    let needle = wanted.trim().to_lowercase();
    if needle.is_empty() {
        return Err(format!("which {kind} was meant is required"));
    }
    let mut exact = Vec::new();
    let mut partial = Vec::new();
    for (name, value) in candidates {
        let hay = name.trim().to_lowercase();
        if hay == needle {
            exact.push((name, value));
        } else if hay.contains(&needle) {
            partial.push((name, value));
        }
    }
    let mut found = if exact.is_empty() { partial } else { exact };
    match found.len() {
        0 => Err(format!("no {kind} of yours is called {wanted}")),
        1 => Ok(found.remove(0).1),
        _ => Err(format!(
            "more than one {kind} matches {wanted}: {} — say which",
            found
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use serde_json::json;

    fn named() -> Vec<(&'static str, u8)> {
        vec![("Consulting", 1), ("Consulting retainer", 2), ("Travel", 3)]
    }

    #[test]
    fn a_stated_argument_is_trimmed_and_a_blank_one_is_not_stated() {
        let args = json!({ "deal": "  Renewal  ", "blank": "   ", "number": 7, "nothing": null });
        assert_eq!(string_arg(&args, "deal").as_deref(), Some("Renewal"));
        assert_eq!(string_arg(&args, "blank"), None);
        assert_eq!(string_arg(&args, "number"), None, "a number is not a name");
        assert_eq!(string_arg(&args, "nothing"), None);
        assert_eq!(string_arg(&args, "absent"), None);
    }

    #[test]
    fn a_name_resolves_to_one_record_or_to_a_refusal_that_lists_them() {
        // Exact wins, whatever the case or the blanks — so a record called
        // "Consulting" is reachable even though "Consulting retainer" contains
        // its whole name.
        assert_eq!(pick_name("  CONSULTING ", named(), "product").unwrap(), 1);
        // A fragment that only one record contains is that record.
        assert_eq!(pick_name("retainer", named(), "product").unwrap(), 2);
        // A fragment two records share is a question, not a guess — and it
        // names them both.
        let why = pick_name("consult", named(), "product").unwrap_err();
        assert!(why.contains("more than one product"), "{why}");
        assert!(why.contains("Consulting, Consulting retainer"), "{why}");
        // Nothing at all is a refusal that repeats what was asked for.
        let why = pick_name("Hovercraft", named(), "product").unwrap_err();
        assert!(
            why.contains("no product of yours is called Hovercraft"),
            "{why}"
        );
        assert!(pick_name("   ", named(), "product").is_err());
        // Two records with the same name are ambiguous even on an exact match.
        let twins = vec![("Acme", 1), ("Acme", 2)];
        assert!(pick_name("acme", twins, "customer").is_err());
    }

    #[test]
    fn money_arrives_whole_or_not_at_all() {
        assert_eq!(
            integer(Some(&json!(12_000)), "unitPriceCents"),
            Ok(Some(12_000))
        );
        assert_eq!(
            integer(Some(&json!(-500)), "unitPriceCents"),
            Ok(Some(-500))
        );
        assert_eq!(integer(None, "valueCents"), Ok(None));
        assert_eq!(integer(Some(&Value::Null), "valueCents"), Ok(None));
        // An amount with a decimal point is a mistake about the unit, and the
        // refusal says which unit we meant.
        let why = integer(Some(&json!(119.99)), "unitPriceCents").unwrap_err();
        assert!(why.contains("whole number of cents"), "{why}");
        assert!(integer(Some(&json!("12000")), "unitPriceCents").is_err());
        assert!(integer(Some(&json!(true)), "valueCents").is_err());
    }

    #[test]
    fn the_http_form_of_a_failed_resolution_is_a_422() {
        let problem = pick("Hovercraft", named(), "product").expect_err("no such product");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            problem.detail.as_deref(),
            Some("no product of yours is called Hovercraft")
        );
        assert_eq!(pick("travel", named(), "product").ok(), Some(3));
    }
}
