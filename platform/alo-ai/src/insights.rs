//! Ask-to-chart (ADR 0037, wave BI1.07): a sentence in, **one proposed chart
//! specification** out — never SQL, never a query.
//!
//! This module owns the *shape of the conversation* and nothing else: the
//! system prompt that says what a reply must look like, the one repair turn a
//! refused reply earns, and the strict read of what came back. The
//! **vocabulary** a chart may be built from is handed in as `catalog`, rendered
//! by the store from the very enums its validator enforces
//! (`alo_store::insight_prompt`), because a copy of that list kept here would
//! drift from the real one the first time a measure is added.
//!
//! Three rules make the result safe, and none of them is this module's to
//! enforce alone:
//!
//! 1. **The model fills in a form; it does not write a query.** Every field it
//!    may set is an enum variant from the catalog, and the SQL those variants
//!    map to is written at compile time by us.
//! 2. **The server validates, not the prompt.** Whatever comes back is parsed
//!    by `ChartSpec::from_value` at the caller before it is evaluated. A prompt
//!    is guidance; the write gate is the rule.
//! 3. **One repair, then a refusal.** A reply the validator rejects earns
//!    exactly one more turn, carrying the validator's own sentence. A second
//!    failure is a typed refusal the user sees as "we could not build a chart
//!    from that" — with nothing stored, nothing pinned, and nothing guessed at.
//!
//! Propose-then-approve (ADR 0034) is the whole point: this produces a
//! *proposal*. A chart becomes a tile only when a person pins it.

use crate::agent::extract_json;
use crate::{AiConfig, ChatMessage, InferenceError, chat};

/// What the model is, and the single shape its reply may take.
const CHART_SYSTEM_HEAD: &str = "You turn a question about a business into ONE chart \
specification for its own workspace data. You reply with a SINGLE JSON object and nothing else: \
no prose, no explanation, no markdown, no code fences.\n\n";

/// The rules that hold whatever the question is. They come after the catalog so
/// the output contract is the last thing the model reads — the order the agent
/// prompt (ADR 0034) already settled on.
const CHART_SYSTEM_RULES: &str = "\nRules:\n\
- Use ONLY the names listed above, spelled exactly. Never invent a dataset, measure, breakdown, \
filter, grain or drawing, and never name a table, column or SQL of any kind.\n\
- A measure may only be broken down by the breakdowns listed for that measure, and a filter may \
only be used on the dataset that lists it.\n\
- Prefer the smallest specification that answers the question: no filters, sort or limit unless \
the question asks for them.\n\
- For \"how much did we bill/invoice\", measure net on billing.documents; for money actually \
received, measure amount on billing.payments; for money still owed, measure outstanding on \
billing.receivables.\n\
- Resolve relative periods (this year, last quarter, the last six months) against today's date \
given below, preferring a last_n period over an explicit range.\n\
- If the question cannot be answered with these names — it is about something this catalog does \
not hold, or it is not a question about data at all — reply with exactly \
{\"error\":\"<one short sentence saying what you cannot chart>\"} instead of a specification.\n\
Output ONLY the JSON object.";

/// The chat messages for one ask. Pure and exported so the prompt is testable
/// without a backend.
///
/// `catalog` is the rendered vocabulary (the store's `catalog_prompt()`),
/// `question` is the user's own sentence in their own language, and `today` is
/// the caller's date (`YYYY-MM-DD`) so "last quarter" resolves to a real one.
#[must_use]
pub fn chart_messages(catalog: &str, question: &str, today: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".to_owned(),
            content: format!("{CHART_SYSTEM_HEAD}{catalog}{CHART_SYSTEM_RULES}"),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: format!(
                "Today's date is {}.\nQuestion: {}",
                today.trim(),
                question.trim()
            ),
        },
    ]
}

/// The same conversation plus the one repair turn: what the model said, and the
/// server's own sentence for why it was refused.
///
/// The refusal text is the validator's ([`SpecError`](../../alo_store/insight_spec/enum.SpecError.html)),
/// which names the offending field and the rule it broke — written for exactly
/// this purpose. The model's previous reply is echoed back as its own turn so a
/// correction is a correction rather than a fresh guess.
#[must_use]
pub fn repair_messages(base: &[ChatMessage], reply: &str, refusal: &str) -> Vec<ChatMessage> {
    let mut messages = base.to_vec();
    messages.push(ChatMessage {
        role: "assistant".to_owned(),
        content: reply.trim().to_owned(),
    });
    messages.push(ChatMessage {
        role: "user".to_owned(),
        content: format!(
            "That was refused: {}\nReply with ONE corrected JSON object and nothing else. \
             If you cannot correct it, reply with {{\"error\":\"<one short sentence>\"}}.",
            refusal.trim()
        ),
    });
    messages
}

/// What one model reply turned out to be.
#[derive(Debug, Clone, PartialEq)]
pub enum ChartReply {
    /// A candidate specification — **not yet valid**: the caller parses it
    /// through the store's write gate, which is the only thing that decides.
    Spec(serde_json::Value),
    /// The model said it cannot chart the question, in its own words.
    Refused(String),
}

/// The longest refusal sentence we carry back from a model. A refusal is one
/// line for a user to read, and an unbounded string from a backend is not
/// something to put on a screen.
const MAX_REFUSAL_CHARS: usize = 300;

/// Read one model reply, tolerating a code fence or a line of preamble but
/// strict about what the object is.
///
/// # Errors
/// [`InferenceError::Empty`] when there is no JSON object in the text, or it is
/// not an object at all — the caller turns that into its one repair turn.
pub fn parse_chart_reply(text: &str) -> Result<ChartReply, InferenceError> {
    let json = extract_json(text).ok_or(InferenceError::Empty)?;
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| InferenceError::Empty)?;
    if !value.is_object() {
        return Err(InferenceError::Empty);
    }
    // A stated refusal wins over everything else in the object: a model that
    // says it cannot chart something and then guesses anyway is saying no.
    if let Some(reason) = value.get("error").and_then(serde_json::Value::as_str) {
        let reason: String = reason.trim().chars().take(MAX_REFUSAL_CHARS).collect();
        if !reason.is_empty() {
            return Ok(ChartReply::Refused(reason));
        }
        return Err(InferenceError::Empty);
    }
    Ok(ChartReply::Spec(value))
}

/// One turn of the ask conversation: call the backend and hand back its raw
/// text, so the caller can both parse it and — if it is refused — echo it into
/// the repair turn.
///
/// Temperature is low by design: this is a form to fill in, not prose to write.
///
/// # Errors
/// [`InferenceError`] for a disabled/unconfigured backend, a transport failure,
/// or an empty completion.
pub async fn chart_turn(
    config: &AiConfig,
    messages: &[ChatMessage],
) -> Result<String, InferenceError> {
    chat(config, messages, 0.1).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    const CATALOG: &str = "billing.documents — invoices.\n  measures:\n    net — money\n";

    #[test]
    fn the_prompt_carries_the_catalog_the_question_and_the_date() {
        let messages = chart_messages(CATALOG, "  revenue by month this year  ", "2026-08-07");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert!(messages[0].content.contains("billing.documents"));
        // The output contract is the last thing the model reads.
        assert!(
            messages[0]
                .content
                .ends_with("Output ONLY the JSON object.")
        );
        assert_eq!(messages[1].role, "user");
        assert!(messages[1].content.contains("revenue by month this year"));
        assert!(messages[1].content.contains("2026-08-07"));
    }

    #[test]
    fn the_repair_turn_echoes_the_reply_and_carries_the_servers_own_sentence() {
        let base = chart_messages(CATALOG, "revenue", "2026-08-07");
        let bad = r#"{"schema_version":1,"measure":{"id":"profit","agg":"sum"}}"#;
        let messages = repair_messages(&base, bad, "measure: unknown variant `profit`");

        assert_eq!(messages.len(), base.len() + 2);
        assert_eq!(messages[2].role, "assistant");
        assert_eq!(messages[2].content, bad);
        assert_eq!(messages[3].role, "user");
        assert!(messages[3].content.contains("unknown variant `profit`"));
        // The base conversation is untouched — a repair is an extra turn, never
        // a rewritten one.
        for (kept, original) in messages.iter().zip(&base) {
            assert_eq!(kept.role, original.role);
            assert_eq!(kept.content, original.content);
        }
    }

    #[test]
    fn a_reply_is_read_through_fences_and_preamble() {
        let text =
            "Sure!\n```json\n{\"schema_version\": 1, \"dataset\": \"billing.documents\"}\n```";
        assert_eq!(
            parse_chart_reply(text).unwrap(),
            ChartReply::Spec(json!({ "schema_version": 1, "dataset": "billing.documents" }))
        );
    }

    #[test]
    fn a_model_that_says_it_cannot_chart_something_is_believed() {
        let refused = parse_chart_reply(r#"{"error":"  I cannot chart the weather.  "}"#).unwrap();
        assert_eq!(
            refused,
            ChartReply::Refused("I cannot chart the weather.".to_owned())
        );

        // A refusal *and* a guess is still a refusal.
        let both = parse_chart_reply(r#"{"error":"no data for that","schema_version":1}"#).unwrap();
        assert!(matches!(both, ChartReply::Refused(_)));

        // A refusal is bounded: it goes on a screen.
        let long = format!(r#"{{"error":"{}"}}"#, "x".repeat(MAX_REFUSAL_CHARS * 2));
        match parse_chart_reply(&long).unwrap() {
            ChartReply::Refused(reason) => assert_eq!(reason.chars().count(), MAX_REFUSAL_CHARS),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn nothing_that_is_not_an_object_is_read_as_one() {
        for text in [
            "",
            "I think revenue rose last quarter.",
            "{not json}",
            "[{\"a\":1},{\"b\":2}]",
            r#"{"error":"   "}"#,
        ] {
            assert!(
                matches!(parse_chart_reply(text), Err(InferenceError::Empty)),
                "{text:?} must not parse"
            );
        }

        // One object wrapped in an array is still one object, and is read as
        // such — the same leniency the agent envelope has, for the same reason:
        // whatever comes out is validated by the write gate before it is used,
        // so refusing a recoverable reply would only cost the user a turn.
        assert_eq!(
            parse_chart_reply("[{\"schema_version\":1}]").unwrap(),
            ChartReply::Spec(json!({ "schema_version": 1 }))
        );
    }

    #[test]
    fn the_prompt_never_invites_sql_or_an_identifier() {
        let system = chart_messages(CATALOG, "q", "2026-08-07").remove(0).content;
        assert!(system.contains("never name a table, column or SQL"));
        assert!(system.contains("ONLY the names listed above"));
    }
}
