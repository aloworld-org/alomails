//! End-of-turn memory extraction (ADR 0057 §6, queue item A6.1): what one
//! answered turn is worth remembering in its channel.
//!
//! The model is shown the question, the answer, and the numbered sources the
//! turn actually read — nothing else — and asked for at most
//! [`MEMORY_FACTS_MAX`] durable facts as a JSON array of strings. An empty
//! array is the expected common case: most turns teach nothing, and a prompt
//! that flatters every exchange into a "fact" fills a room's memory with
//! noise the next turn then has to read past.
//!
//! Facts, never transcripts: the parser cuts anything long enough to be one,
//! and the store's own cap ([`MEMORY_FACT_LIMIT`] mirrors it) refuses what
//! slips through. Whether to remember at all is the caller's question — the
//! per-channel switch lives in the store, and this module is never consulted
//! when it is off.

use crate::{AiConfig, ChatMessage, InferenceError, WorkspaceSource, chat, render_sources};

/// The most facts one turn may leave behind. A turn that "learned" ten things
/// read a document, and the document is already in the record.
pub const MEMORY_FACTS_MAX: usize = 3;

/// The longest fact the parser will pass on, in characters — the same number
/// as the store's `MEMORY_FACT_MAX`, restated here so the crates stay
/// dependency-free of each other. A reply element beyond it is dropped, not
/// truncated: a cut-off sentence stored as a fact would be read back as one.
pub const MEMORY_FACT_LIMIT: usize = 400;

const MEMORY_SYSTEM: &str = "You decide what one exchange with a workspace agent is worth \
remembering for later conversations in the same channel. Return ONLY a JSON array of strings, \
like [\"Northstar Foods invoices are net 30\"]. Each string is ONE durable fact or decision, \
a short standalone sentence under 30 words, in the exchange's own language. Only include what \
was actually said or read in THIS exchange and will still matter next month: agreements, \
decisions, standing preferences, corrections. Never include the question itself, pleasantries, \
transient numbers that a record already holds (stock levels, totals, dates of one meeting), \
speculation, or anything resembling a password, key or credential. Most exchanges teach \
nothing: then return [].";

/// The chat messages for one extraction. Pure and exported so the prompt is
/// testable without a backend.
#[must_use]
pub fn memory_messages(
    request: &str,
    answer: &str,
    sources: &[WorkspaceSource],
) -> Vec<ChatMessage> {
    let user = format!(
        "The person asked: {}\n\nThe agent answered: {}\n\nWhat the agent read to answer:\n{}",
        request.trim(),
        answer.trim(),
        render_sources(sources)
    );
    vec![
        ChatMessage {
            role: "system".to_owned(),
            content: MEMORY_SYSTEM.to_owned(),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: user,
        },
    ]
}

/// Parse the model's reply into facts: the JSON array of strings, tolerant of
/// code fences and surrounding prose, strict about the shape inside. Anything
/// that is not a non-empty string under [`MEMORY_FACT_LIMIT`] characters is
/// dropped, and at most [`MEMORY_FACTS_MAX`] survive.
#[must_use]
pub fn parse_memories(text: &str) -> Vec<String> {
    let Some(start) = text.find('[') else {
        return Vec::new();
    };
    let Some(end) = text.rfind(']') else {
        return Vec::new();
    };
    if end <= start {
        return Vec::new();
    }
    let Ok(serde_json::Value::Array(items)) =
        serde_json::from_str::<serde_json::Value>(&text[start..=end])
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.as_str())
        .map(str::trim)
        .filter(|fact| !fact.is_empty() && fact.chars().count() <= MEMORY_FACT_LIMIT)
        .map(str::to_owned)
        .take(MEMORY_FACTS_MAX)
        .collect()
}

/// Ask the model what this turn taught. An empty vec is a normal answer, not
/// an error — only the transport and configuration failures of every other
/// inference call are errors here.
///
/// # Errors
/// [`InferenceError`] for disabled/unconfigured/unreachable/backend/empty.
pub async fn extract_memories(
    config: &AiConfig,
    request: &str,
    answer: &str,
    sources: &[WorkspaceSource],
) -> Result<Vec<String>, InferenceError> {
    let text = chat(config, &memory_messages(request, answer, sources), 0.2).await?;
    Ok(parse_memories(&text))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_shows_question_answer_and_what_was_read() {
        let sources = vec![WorkspaceSource {
            index: 1,
            kind: "tool result".to_owned(),
            title: "open_quotes".to_owned(),
            detail: "Q-31 Northstar Foods €1,200".to_owned(),
        }];
        let messages = memory_messages("are we in contact?", "Yes — see [1].", &sources);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert!(messages[0].content.contains("JSON array"));
        assert!(messages[0].content.contains("return []"));
        let user = &messages[1].content;
        assert!(user.contains("are we in contact?"));
        assert!(user.contains("Yes — see [1]."));
        assert!(user.contains("open_quotes"));
        assert!(user.contains("Northstar Foods"));
    }

    #[test]
    fn facts_are_parsed_out_of_fences_and_prose() {
        let reply = "Sure — here is what I kept:\n```json\n[\"Northstar invoices are net 30\", \
                     \"Anna approves discounts over 10%\"]\n```";
        assert_eq!(
            parse_memories(reply),
            vec![
                "Northstar invoices are net 30",
                "Anna approves discounts over 10%"
            ]
        );
    }

    #[test]
    fn an_empty_array_no_array_and_junk_all_mean_nothing_learned() {
        assert!(parse_memories("[]").is_empty());
        assert!(parse_memories("nothing worth remembering").is_empty());
        assert!(parse_memories("{\"kind\":\"answer\"}").is_empty());
        assert!(
            parse_memories("[1, 2, 3]").is_empty(),
            "numbers are not facts"
        );
    }

    #[test]
    fn the_cap_and_the_transcript_guard_hold() {
        let five = "[\"a\",\"b\",\"c\",\"d\",\"e\"]";
        assert_eq!(parse_memories(five).len(), MEMORY_FACTS_MAX);
        let transcript = format!("[\"{}\"]", "x".repeat(MEMORY_FACT_LIMIT + 1));
        assert!(
            parse_memories(&transcript).is_empty(),
            "a transcript-length element is dropped whole, not truncated into a broken fact"
        );
        let blank = "[\"   \", \"kept\"]";
        assert_eq!(parse_memories(blank), vec!["kept"]);
    }
}
