//! Server-side mail filters (rules) and Block sender. The user edits structured
//! rules (conditions → actions); this module is the single owner of the rule
//! model, compiles the rules **together with any out-of-office vacation** into
//! one managed Sieve script, and installs + activates it through the store's
//! existing Sieve API. The delivery-time evaluator (`alo-smtp` →
//! `deliver_sieve`) then runs it on inbound mail.
//!
//! There is only ever one active script per account, so filters and vacation
//! MUST share it — both `PUT /filters` and `POST /settings/out-of-office` route
//! through [`rebuild_managed_script`]. A legacy standalone `out-of-office`
//! script (from before this unification) is cleaned up on the next rebuild.

use alo_store::StoreError;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::{Account, AppState, authenticate};

/// The single managed script name. Combines filter rules + vacation.
const MANAGED_SCRIPT: &str = "alo-mail-rules";
/// The pre-unification standalone out-of-office script, cleaned up on rebuild.
const LEGACY_OOO_SCRIPT: &str = "out-of-office";

/// Bounds (anti-abuse / keep the generated script sane).
const MAX_RULES: usize = 100;
const MAX_CONDITIONS: usize = 20;
const MAX_ACTIONS: usize = 10;
const MAX_VALUE_LEN: usize = 512;

/// Which header a condition tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Field {
    From,
    To,
    Cc,
    Subject,
}

impl Field {
    /// The header name matched in Sieve.
    fn header(self) -> &'static str {
        match self {
            Field::From => "from",
            Field::To => "to",
            Field::Cc => "cc",
            Field::Subject => "subject",
        }
    }
}

/// How a value is compared against the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Op {
    Contains,
    Is,
}

impl Op {
    fn tag(self) -> &'static str {
        match self {
            Op::Contains => ":contains",
            Op::Is => ":is",
        }
    }
}

/// One condition: `<field> <op> "<value>"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Condition {
    field: Field,
    #[serde(default = "default_op")]
    op: Op,
    value: String,
}

fn default_op() -> Op {
    Op::Contains
}

/// Whether all or any condition must hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum MatchMode {
    All,
    Any,
}

/// One action a matching rule performs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Action {
    /// File the message into a mailbox (by name/path). Cancels the implicit
    /// keep, so a rule that only files also removes the message from the Inbox.
    FileInto { mailbox: String },
    /// Mark it read.
    MarkRead,
    /// Star it (flagged).
    Star,
    /// Drop it (no delivery). Terminal: stops further rule processing.
    Delete,
}

/// One filter rule. `id` is client-assigned and opaque to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Rule {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(rename = "match", default = "default_match")]
    match_mode: MatchMode,
    conditions: Vec<Condition>,
    actions: Vec<Action>,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_match() -> MatchMode {
    MatchMode::All
}
fn default_true() -> bool {
    true
}

/// `GET /filters` → `{"rules": [...]}` — the stored structured rules.
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let rules = load_rules(&account).await?;
    Ok(Json(json!({ "rules": rules })))
}

/// `PUT /filters` — body `{"rules": [...]}`. Validates, stores, and rebuilds the
/// managed Sieve script. Returns the stored rules.
pub async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let raw = req.get("rules").cloned().unwrap_or_else(|| json!([]));
    let rules: Vec<Rule> = serde_json::from_value(raw)
        .map_err(|_| Problem::with(StatusCode::BAD_REQUEST, "malformed rules"))?;
    validate(&rules)?;
    store_and_rebuild(&account, &rules).await?;
    Ok(Json(json!({ "rules": rules })))
}

/// `POST /filters/block` — body `{"email": "..."}`. Appends a rule that files
/// mail from that address into Junk (idempotent), then rebuilds. This is the
/// one-click "Block sender".
pub async fn block(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let email = req
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|e| e.contains('@') && e.len() <= MAX_VALUE_LEN && !has_control(e))
        .ok_or_else(|| Problem::with(StatusCode::BAD_REQUEST, "a valid email is required"))?;
    let mut rules = load_rules(&account).await?;
    let already = rules.iter().any(|r| is_block_of(r, email));
    if !already {
        if rules.len() >= MAX_RULES {
            return Err(Problem::with(StatusCode::BAD_REQUEST, "too many rules"));
        }
        rules.push(block_rule(email));
        store_and_rebuild(&account, &rules).await?;
    }
    Ok(Json(json!({ "blocked": true })))
}

/// Rebuild the managed Sieve script from the account's stored filters + OOO. The
/// single entry point both the filters and out-of-office paths call.
pub async fn rebuild_managed_script(account: &Account) -> Result<(), Problem> {
    let rules = load_rules(account).await?;
    let ooo = account
        .acc
        .out_of_office()
        .await
        .map_err(|_| Problem::server_error())?;
    let script = generate_script(&rules, ooo.enabled, &ooo.subject, &ooo.message);
    match script {
        Some(text) => {
            account
                .acc
                .put_sieve_script(MANAGED_SCRIPT, &text)
                .await
                .map_err(sieve_problem)?;
            account
                .acc
                .activate_sieve_script(Some(MANAGED_SCRIPT))
                .await
                .map_err(|_| Problem::server_error())?;
        }
        None => {
            // Nothing to run: deactivate and drop the managed script.
            let _ = account.acc.activate_sieve_script(None).await;
            let _ = delete_if_present(account, MANAGED_SCRIPT).await;
        }
    }
    // Remove the pre-unification standalone OOO script (now folded into MANAGED).
    let _ = delete_if_present(account, LEGACY_OOO_SCRIPT).await;
    Ok(())
}

// ---- internals -------------------------------------------------------------

async fn load_rules(account: &Account) -> Result<Vec<Rule>, Problem> {
    let json = account
        .acc
        .filters()
        .await
        .map_err(|_| Problem::server_error())?;
    // A parse failure means legacy/corrupt data — treat as no rules rather than
    // erroring the whole settings screen.
    Ok(serde_json::from_str(&json).unwrap_or_default())
}

async fn store_and_rebuild(account: &Account, rules: &[Rule]) -> Result<(), Problem> {
    let json = serde_json::to_string(rules).map_err(|_| Problem::server_error())?;
    account
        .acc
        .set_filters(&json)
        .await
        .map_err(|_| Problem::server_error())?;
    rebuild_managed_script(account).await
}

/// Delete a script if it exists (ignoring a `NotFound`); surfaces other errors.
async fn delete_if_present(account: &Account, name: &str) -> Result<(), Problem> {
    match account.acc.delete_sieve_script(name).await {
        Ok(()) => Ok(()),
        Err(StoreError::NotFound) => Ok(()),
        Err(_) => Err(Problem::server_error()),
    }
}

fn validate(rules: &[Rule]) -> Result<(), Problem> {
    let bad = |m: &str| Problem::with(StatusCode::BAD_REQUEST, m.to_owned());
    if rules.len() > MAX_RULES {
        return Err(bad("too many rules"));
    }
    for rule in rules {
        if rule.conditions.is_empty() || rule.conditions.len() > MAX_CONDITIONS {
            return Err(bad("each rule needs 1..=20 conditions"));
        }
        if rule.actions.is_empty() || rule.actions.len() > MAX_ACTIONS {
            return Err(bad("each rule needs 1..=10 actions"));
        }
        for c in &rule.conditions {
            let v = c.value.trim();
            if v.is_empty() || v.len() > MAX_VALUE_LEN || has_control(v) {
                return Err(bad(
                    "a condition value is empty, too long, or has control characters",
                ));
            }
        }
        for a in &rule.actions {
            if let Action::FileInto { mailbox } = a {
                let m = mailbox.trim();
                if m.is_empty() || m.len() > MAX_VALUE_LEN || has_control(m) {
                    return Err(bad("a target mailbox name is empty, too long, or invalid"));
                }
            }
        }
    }
    Ok(())
}

fn has_control(s: &str) -> bool {
    s.chars().any(|c| c.is_control())
}

/// True if `rule` is exactly a Block-sender rule for `email`.
fn is_block_of(rule: &Rule, email: &str) -> bool {
    rule.conditions.len() == 1
        && rule.conditions[0].field == Field::From
        && rule.conditions[0].value.eq_ignore_ascii_case(email)
        && rule
            .actions
            .iter()
            .any(|a| matches!(a, Action::FileInto { mailbox } if mailbox == "Junk"))
}

/// A Block-sender rule: mail from `email` → Junk.
fn block_rule(email: &str) -> Rule {
    Rule {
        id: format!("block-{}", email.to_lowercase()),
        name: format!("Block {email}"),
        match_mode: MatchMode::Any,
        conditions: vec![Condition {
            field: Field::From,
            op: Op::Contains,
            value: email.to_owned(),
        }],
        actions: vec![Action::FileInto {
            mailbox: "Junk".to_owned(),
        }],
        enabled: true,
    }
}

/// Escape the two characters special to a Sieve quoted string.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Compile the rules (+ optional vacation) into one Sieve script, or `None` when
/// there is nothing to run (no enabled rules and OOO off) — the caller then
/// deactivates the managed script entirely.
fn generate_script(
    rules: &[Rule],
    ooo_on: bool,
    ooo_subject: &str,
    ooo_message: &str,
) -> Option<String> {
    let active: Vec<&Rule> = rules.iter().filter(|r| r.enabled).collect();
    if active.is_empty() && !ooo_on {
        return None;
    }

    // Collect the required extensions from what we actually emit.
    let mut requires: Vec<&str> = Vec::new();
    let uses_fileinto = active.iter().any(|r| {
        r.actions
            .iter()
            .any(|a| matches!(a, Action::FileInto { .. }))
    });
    let uses_flags = active.iter().any(|r| {
        r.actions
            .iter()
            .any(|a| matches!(a, Action::MarkRead | Action::Star))
    });
    if uses_fileinto {
        requires.push("fileinto");
    }
    if uses_flags {
        requires.push("imap4flags");
    }
    if ooo_on {
        requires.push("vacation");
    }

    let mut out = String::new();
    if !requires.is_empty() {
        let list = requires
            .iter()
            .map(|e| format!("\"{e}\""))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("require [{list}];\n"));
    }

    // Vacation first, so an auto-reply fires independently of any filing/stop.
    if ooo_on {
        let subject_arg = if ooo_subject.trim().is_empty() {
            String::new()
        } else {
            format!(" :subject \"{}\"", esc(ooo_subject))
        };
        // The handle is what delivery matches the date window against, and it
        // has to be the store's own constant rather than the same string
        // spelled again here: a copy that drifted would leave a reply that
        // looks scheduled on the settings screen and answers every day of the
        // year.
        out.push_str(&format!(
            "vacation :days 7 :handle \"{}\"{subject_arg} \"{}\";\n",
            alo_store::OOO_HANDLE,
            esc(ooo_message)
        ));
    }

    for rule in active {
        out.push_str(&render_rule(rule));
    }
    Some(out)
}

/// Render one rule as an `if <match> (<tests>) { <actions> }` block.
fn render_rule(rule: &Rule) -> String {
    let tests: Vec<String> = rule
        .conditions
        .iter()
        .map(|c| {
            format!(
                "header {} \"{}\" \"{}\"",
                c.op.tag(),
                c.field.header(),
                esc(c.value.trim())
            )
        })
        .collect();
    let test = match (rule.match_mode, tests.len()) {
        (_, 1) => tests[0].clone(),
        (MatchMode::All, _) => format!("allof ({})", tests.join(", ")),
        (MatchMode::Any, _) => format!("anyof ({})", tests.join(", ")),
    };

    // Flags accumulate onto the filing/keep statement (imap4flags :flags).
    let mut flags: Vec<&str> = Vec::new();
    if rule.actions.contains(&Action::MarkRead) {
        flags.push("\\\\Seen");
    }
    if rule.actions.contains(&Action::Star) {
        flags.push("\\\\Flagged");
    }
    let flags_arg = if flags.is_empty() {
        String::new()
    } else {
        format!(" :flags \"{}\"", flags.join(" "))
    };

    let mut body = String::new();
    if rule.actions.contains(&Action::Delete) {
        // Delete wins and is terminal.
        body.push_str("    discard;\n    stop;\n");
    } else {
        let folders: Vec<&str> = rule
            .actions
            .iter()
            .filter_map(|a| match a {
                Action::FileInto { mailbox } => Some(mailbox.trim()),
                _ => None,
            })
            .collect();
        if folders.is_empty() {
            // No filing: just keep (optionally flagged).
            body.push_str(&format!("    keep{flags_arg};\n"));
        } else {
            for folder in folders {
                body.push_str(&format!("    fileinto{flags_arg} \"{}\";\n", esc(folder)));
            }
        }
    }

    format!("if {test} {{\n{body}}}\n")
}

/// Map a store error from `put_sieve_script` to a client problem. A compile
/// failure (`Conflict`) is a 500 here — the *server* generated the script, so a
/// compile error is our bug, never the client's input format.
fn sieve_problem(_e: StoreError) -> Problem {
    Problem::server_error()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(json: &str) -> Rule {
        serde_json::from_str(json).unwrap_or_else(|e| panic!("bad rule json: {e}"))
    }

    #[test]
    fn generates_compilable_script_for_each_action() {
        let rules = vec![
            rule(
                r#"{"conditions":[{"field":"from","value":"spam@x.com"}],"actions":[{"type":"fileInto","mailbox":"Junk"}],"match":"any"}"#,
            ),
            rule(
                r#"{"conditions":[{"field":"subject","op":"contains","value":"invoice"},{"field":"from","value":"billing@acme.eu"}],"actions":[{"type":"fileInto","mailbox":"Finance"},{"type":"markRead"},{"type":"star"}],"match":"all"}"#,
            ),
            rule(
                r#"{"conditions":[{"field":"from","value":"noise@list.eu"}],"actions":[{"type":"delete"}],"match":"any"}"#,
            ),
        ];
        let script =
            generate_script(&rules, false, "", "").unwrap_or_else(|| panic!("expected a script"));
        // It must compile in the real engine.
        alo_sieve::compile(&script, alo_sieve::Limits::default())
            .unwrap_or_else(|e| panic!("did not compile: {e}\n{script}"));
        assert!(script.contains("fileinto :flags \"\\\\Seen \\\\Flagged\" \"Finance\""));
        assert!(script.contains("discard;"));
    }

    #[test]
    fn combines_filters_with_vacation_and_compiles() {
        let rules = vec![rule(
            r#"{"conditions":[{"field":"from","value":"a@b.eu"}],"actions":[{"type":"fileInto","mailbox":"Team"}],"match":"any"}"#,
        )];
        let script = generate_script(&rules, true, "Away", "Back Monday")
            .unwrap_or_else(|| panic!("expected a script"));
        alo_sieve::compile(&script, alo_sieve::Limits::default())
            .unwrap_or_else(|e| panic!("did not compile: {e}\n{script}"));
        assert!(script.contains(":subject \"Away\" \"Back Monday\""));
        assert!(script.contains("require [\"fileinto\", \"vacation\"]"));
    }

    #[test]
    fn the_vacation_line_carries_the_handle_delivery_gates_on() {
        // Without it the reply is just another `vacation`, and delivery has
        // nothing to match the user's date window against: the window is
        // stored, displayed on the settings screen, and gates nothing — the
        // reply answers every day of the year. That failure is invisible from
        // the screen, which is why it is pinned here.
        let script = generate_script(&[], true, "Away", "Back Monday")
            .unwrap_or_else(|| panic!("expected a script"));
        assert!(
            script.contains(&format!(":handle \"{}\"", alo_store::OOO_HANDLE)),
            "the managed reply must be identifiable at delivery:\n{script}",
        );
        alo_sieve::compile(&script, alo_sieve::Limits::default())
            .unwrap_or_else(|e| panic!("did not compile: {e}\n{script}"));
    }

    #[test]
    fn empty_and_off_yields_no_script() {
        assert!(generate_script(&[], false, "", "").is_none());
        // A disabled rule doesn't count.
        let disabled = vec![rule(
            r#"{"enabled":false,"conditions":[{"field":"from","value":"x@y.eu"}],"actions":[{"type":"delete"}],"match":"any"}"#,
        )];
        assert!(generate_script(&disabled, false, "", "").is_none());
    }

    #[test]
    fn escapes_quotes_and_backslashes_in_values() {
        let rules = vec![rule(
            r#"{"conditions":[{"field":"subject","value":"a\"b\\c"}],"actions":[{"type":"delete"}],"match":"any"}"#,
        )];
        let script =
            generate_script(&rules, false, "", "").unwrap_or_else(|| panic!("expected a script"));
        alo_sieve::compile(&script, alo_sieve::Limits::default())
            .unwrap_or_else(|e| panic!("did not compile: {e}\n{script}"));
    }

    #[test]
    fn block_rule_is_detected_idempotently() {
        let r = block_rule("bad@evil.com");
        assert!(is_block_of(&r, "bad@evil.com"));
        assert!(is_block_of(&r, "BAD@evil.com"));
        assert!(!is_block_of(&r, "other@evil.com"));
    }
}
