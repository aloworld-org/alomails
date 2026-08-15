//! alo AI inference layer (ADR 0011).
//!
//! Model-agnostic by construction: it speaks one wire contract — the
//! OpenAI-compatible **Chat Completions** API (`{base}/v1/chat/completions`) —
//! which Ollama, vLLM, and every hosted provider we care about implement. The
//! backend is *configured, never bundled*: an operator supplies a base URL, a
//! model, and (optionally) an API key, per tenant.
//!
//! Privacy (constitution law #1): the only thing sent to the backend is the
//! text the user asked us to act on. Prompts and completions are **never
//! logged**, and errors carry status codes only — never response bodies.

use std::time::Duration;

use serde::{Deserialize, Serialize};

pub mod egress;
use egress::{is_blocked_ip, split_authority};

mod agent;
pub mod agent_agenda;
pub mod agent_billing;
pub mod agent_chat;
pub mod agent_contacts;
pub mod agent_crm;
pub mod agent_docs;
pub mod agent_drive;
pub mod agent_finance;
pub mod agent_hr;
pub mod agent_insights;
pub mod agent_inventory;
pub mod agent_mail;
pub mod agent_product;
pub mod agent_projects;
pub mod agent_sheets;
pub mod agent_sites;
pub mod agent_tasks;
mod agent_tool;
pub mod doc_blocks;
pub mod insights;
pub mod sheet_grid;
pub mod site_chat;
pub mod site_edits;
pub mod site_translation;
pub mod sites;
pub use agent::{
    AgentAsk, AgentDecision, AgentProduct, ProposedAction, after_read_messages, agent_messages,
    all_tools, is_agent_tool, is_read_tool, parse_decision, run_agent, run_agent_after_read,
    system_prompt_for,
};
pub use agent_agenda::AGENDA_TOOLS;
pub use agent_billing::BILLING_TOOLS;
pub use agent_chat::CHAT_TOOLS;
pub use agent_contacts::CONTACTS_TOOLS;
pub use agent_crm::CRM_TOOLS;
pub use agent_docs::DOCS_TOOLS;
pub use agent_drive::DRIVE_TOOLS;
pub use agent_finance::FINANCE_TOOLS;
pub use agent_hr::HR_TOOLS;
pub use agent_insights::INSIGHTS_TOOLS;
pub use agent_inventory::INVENTORY_TOOLS;
pub use agent_mail::MAIL_TOOLS;
pub use agent_product::{ToolSet, offers, tool_sets, tools_for};
pub use agent_projects::PROJECTS_TOOLS;
pub use agent_sheets::SHEETS_TOOLS;
pub use agent_sites::SITES_TOOLS;
pub use agent_tasks::TASKS_TOOLS;
pub use agent_tool::{AgentTool, Effect, find_tool};
pub use insights::{ChartReply, chart_messages, chart_turn, parse_chart_reply, repair_messages};
pub use site_chat::{
    MAX_QUESTION_CHARS, SiteChatCitation, SiteChatError, SiteChatRefusal, SiteChatReply,
    SiteChatSource, SiteChatVoice, answer_site_question, citation_path, parse_site_chat_reply,
    retrieve_site_sources, site_chat_messages,
};
pub use site_edits::{
    SITE_EDIT_SCHEMA_VERSION, SiteEditEnvelope, SiteEditError, SiteEditOperation,
    SiteSectionTarget, apply_site_edit, parse_site_edit, propose_site_edit, site_edit_messages,
};
pub use site_translation::{
    SITE_TRANSLATION_SCHEMA_VERSION, SiteTranslationEnvelope, SiteTranslationError,
    SiteTranslationPageProposal, SiteTranslationPageSnapshot, SiteTranslationPostProposal,
    SiteTranslationPostSnapshot, SiteTranslationSource, parse_site_translation,
    propose_site_translation, site_translation_messages, validate_site_translation,
};
pub use sites::{
    SITE_DRAFT_SCHEMA_VERSION, SiteDraft, SiteDraftError, SiteDraftPage, SiteDraftSite,
    generate_site_draft, parse_site_draft, site_generation_messages, site_repair_messages,
};

/// Per-tenant backend configuration (admin-set, ADR 0011).
#[derive(Debug, Clone)]
pub struct AiConfig {
    /// Base URL of an OpenAI-compatible endpoint, e.g. `http://localhost:11434`
    /// (Ollama) or `https://api.mistral.ai`.
    pub base_url: String,
    /// The model name to request, e.g. `llama3.2` or `mistral-small-latest`.
    pub model: String,
    /// Optional bearer key for hosted providers; `None`/empty for local Ollama.
    pub api_key: Option<String>,
    /// Whether AI is enabled for this tenant. When false, calls fail with
    /// [`InferenceError::Disabled`] and callers hide the feature.
    pub enabled: bool,
}

/// Why an inference call did not produce text. Deliberately coarse — it never
/// carries a backend response body (law #1).
#[derive(Debug, thiserror::Error)]
pub enum InferenceError {
    /// AI is switched off for this tenant.
    #[error("ai disabled for tenant")]
    Disabled,
    /// AI is on but no usable endpoint/model is configured.
    #[error("ai not configured")]
    NotConfigured,
    /// The backend answered but with no usable content.
    #[error("empty completion")]
    Empty,
    /// The backend returned a non-success status (code only, no body).
    #[error("inference backend status {0}")]
    Backend(u16),
    /// The backend could not be reached (DNS/TLS/timeout).
    #[error("inference backend unreachable")]
    Transport,
}

/// One chat message in the OpenAI Chat Completions shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    temperature: f32,
    stream: bool,
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: RespMessage,
}

#[derive(Deserialize)]
struct RespMessage {
    #[serde(default)]
    content: String,
}

const IMPROVE_SYSTEM: &str = "You are an editor for email drafts. Improve the \
draft you are given: fix grammar, spelling, clarity, and tone while preserving \
its original meaning and the language it is written in. Return only the \
improved text — no preamble, explanation, or quotation.";

/// The chat messages for an "improve this draft" request. Pure and exported so
/// the prompt is testable without a backend.
pub fn improve_messages(draft: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".to_owned(),
            content: IMPROVE_SYSTEM.to_owned(),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: draft.to_owned(),
        },
    ]
}

/// Extract the assistant's text from an OpenAI-compatible response body. Pure
/// and exported for testing.
///
/// # Errors
/// [`InferenceError::Empty`] if the body does not parse or yields no text.
pub fn parse_completion(body: &str) -> Result<String, InferenceError> {
    let resp: ChatResponse = serde_json::from_str(body).map_err(|_| InferenceError::Empty)?;
    let text = resp
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .unwrap_or_default();
    let text = text.trim().to_owned();
    if text.is_empty() {
        return Err(InferenceError::Empty);
    }
    Ok(text)
}

/// The largest inference response we will buffer. A hostile or broken backend
/// must not be able to exhaust memory (law #2, full path incl. error paths);
/// 4 MiB dwarfs any legitimate improved email draft.
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// Build an HTTP client for a backend at `url`, enforcing the egress policy.
/// Default `open` mode (self-hosted — the model runs on localhost or the
/// private LAN) allows any host. `restricted` mode (`ALO_AI_EGRESS=restricted`,
/// set on shared/hosted deployments) requires https and refuses any host that
/// resolves to a loopback/link-local/private/ULA address, then **pins** the
/// vetted address so a DNS rebind between check and connect cannot slip through.
/// Every rejection returns the same `Transport` error as a genuinely
/// unreachable host — no oracle that reveals what is internally reachable.
async fn build_client(url: &str, timeout: Duration) -> Result<reqwest::Client, InferenceError> {
    let restricted = std::env::var("ALO_AI_EGRESS")
        .map(|v| v.trim().eq_ignore_ascii_case("restricted"))
        .unwrap_or(false);
    let mut builder = reqwest::Client::builder().timeout(timeout);
    if restricted {
        let (https, host, port) = split_authority(url).ok_or(InferenceError::Transport)?;
        if !https {
            return Err(InferenceError::Transport);
        }
        let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|_| InferenceError::Transport)?
            .collect();
        if addrs.is_empty() || addrs.iter().any(|a| is_blocked_ip(a.ip())) {
            return Err(InferenceError::Transport);
        }
        if let Some(first) = addrs.first() {
            builder = builder.resolve(&host, *first);
        }
    }
    builder.build().map_err(|_| InferenceError::Transport)
}

/// Build `{base}/v1/{path}`, tolerating a base that already ends in `/v1` — the
/// form hosted providers print in their docs (`https://api.openai.com/v1`) — or
/// carries a trailing slash. Without this, such a base doubles the segment into
/// `/v1/v1/...` and every request 404s.
fn endpoint(base_url: &str, path: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    let base = base.strip_suffix("/v1").unwrap_or(base);
    format!("{base}/v1/{path}")
}

/// Read a response body, refusing anything larger than [`MAX_RESPONSE_BYTES`].
/// Streams chunk-by-chunk so an over-large (or lying `Content-Length`) backend
/// is rejected without first buffering it whole.
async fn read_body_capped(mut response: reqwest::Response) -> Result<String, InferenceError> {
    let mut buf = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| InferenceError::Transport)?
    {
        if buf.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(InferenceError::Empty);
        }
        buf.extend_from_slice(&chunk);
    }
    String::from_utf8(buf).map_err(|_| InferenceError::Empty)
}

/// The system prompt for summarizing an email thread (ADR 0011).
const SUMMARIZE_SYSTEM: &str = "You summarize an email thread for its recipient. \
In one or two short sentences, say what the thread is about and any action or \
decision the recipient needs to make, in the thread's own language. Be concrete. \
Return only the summary — no preamble, heading, or quotation.";

/// The chat messages for a "summarize this thread" request. Pure and exported
/// so the prompt is testable without a backend.
pub fn summarize_messages(thread: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".to_owned(),
            content: SUMMARIZE_SYSTEM.to_owned(),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: thread.to_owned(),
        },
    ]
}

/// The system prompt for suggesting quick replies to a thread (ADR 0011).
const SMART_REPLY_SYSTEM: &str = "You suggest ready-to-send reply options for the recipient of \
an email thread. Read it and propose exactly three brief replies (each under 12 words) a busy \
professional might send, in the thread's own language; cover a range (e.g. agree/acknowledge, \
ask a clarifying question, decline politely) when it fits. Return each reply on its own line, \
with no numbering, bullets, quotes, or preamble.";

/// The chat messages for a "suggest replies" request. Pure and exported so the
/// prompt is testable without a backend.
pub fn smart_reply_messages(thread: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".to_owned(),
            content: SMART_REPLY_SYSTEM.to_owned(),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: thread.to_owned(),
        },
    ]
}

/// Parse the model's reply-suggestion text into up to three clean lines,
/// stripping any list markers, numbering, or wrapping quotes it added.
pub fn parse_replies(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(|c: char| {
                    c.is_ascii_digit() || matches!(c, '.' | ')' | '-' | '*' | '•' | ' ')
                })
                .trim()
                .trim_matches(['"', '\''])
                .to_owned()
        })
        .filter(|l| !l.is_empty())
        .take(3)
        .collect()
}

/// The system prompt for extracting action items from an email (ADR 0024). The
/// result is fed to the propose-then-approve flow, never created directly.
const EXTRACT_TASKS_SYSTEM: &str = "You extract concrete action items from an email — things the \
reader must do. Return ONLY a JSON array of objects like [{\"title\":\"Send the report\"}], each \
a short imperative task under 12 words, in the email's own language. Include only real, actionable \
items; if there are none, return []. No prose and no code fences — just the JSON array.";

/// A candidate task the AI extracted from text.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtractedTask {
    pub title: String,
}

/// The chat messages for [`extract_tasks`].
pub fn extract_tasks_messages(text: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".to_owned(),
            content: EXTRACT_TASKS_SYSTEM.to_owned(),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: text.to_owned(),
        },
    ]
}

/// Parse the model's JSON array of `{title}` objects, leniently (models sometimes
/// wrap it in prose). Titles only; the user sets due/assignee on accept.
pub fn parse_extracted_tasks(text: &str) -> Vec<ExtractedTask> {
    let (Some(start), Some(end)) = (text.find('['), text.rfind(']')) else {
        return Vec::new();
    };
    if end <= start {
        return Vec::new();
    }
    let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(&text[start..=end]) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let title = item.get("title")?.as_str()?.trim().to_owned();
            (!title.is_empty()).then_some(ExtractedTask { title })
        })
        .take(8)
        .collect()
}

/// Extract action items from an email's text. Soft-degrades like the other AI
/// helpers (disabled/unconfigured → an error the caller turns into "AI is off").
///
/// # Errors
/// [`InferenceError`] on a disabled/unconfigured backend or transport failure.
pub async fn extract_tasks(
    config: &AiConfig,
    text: &str,
) -> Result<Vec<ExtractedTask>, InferenceError> {
    let out = chat(config, &extract_tasks_messages(text), 0.2).await?;
    Ok(parse_extracted_tasks(&out))
}

/// Suggest up to three short replies to a thread. Soft-degrades like the other
/// AI helpers; returns [`InferenceError::Empty`] when nothing usable comes back.
///
/// # Errors
/// [`InferenceError`] on a disabled/unconfigured backend, transport failure, or
/// an empty result.
pub async fn suggest_replies(
    config: &AiConfig,
    thread: &str,
) -> Result<Vec<String>, InferenceError> {
    let text = chat(config, &smart_reply_messages(thread), 0.4).await?;
    let replies = parse_replies(&text);
    if replies.is_empty() {
        return Err(InferenceError::Empty);
    }
    Ok(replies)
}

/// One chat-completions round-trip to the configured backend, returning the
/// assistant's text. Shared by [`improve`] and [`summarize`]; enforces the
/// enabled/configured gates and the egress policy, and never logs content.
pub(crate) async fn chat(
    config: &AiConfig,
    messages: &[ChatMessage],
    temperature: f32,
) -> Result<String, InferenceError> {
    if !config.enabled {
        return Err(InferenceError::Disabled);
    }
    if config.base_url.trim().is_empty() || config.model.trim().is_empty() {
        return Err(InferenceError::NotConfigured);
    }
    let url = endpoint(&config.base_url, "chat/completions");
    let body = ChatRequest {
        model: config.model.trim(),
        messages,
        temperature,
        stream: false,
    };
    let client = build_client(&url, Duration::from_secs(60)).await?;
    let mut request = client.post(&url).json(&body);
    if let Some(key) = &config.api_key
        && !key.trim().is_empty()
    {
        request = request.bearer_auth(key.trim());
    }
    let response = request
        .send()
        .await
        .map_err(|_| InferenceError::Transport)?;
    let status = response.status();
    if !status.is_success() {
        return Err(InferenceError::Backend(status.as_u16()));
    }
    let text = read_body_capped(response).await?;
    parse_completion(&text)
}

/// Improve an email draft via the configured backend. User-invoked only.
///
/// # Errors
/// [`InferenceError`] variants for disabled/unconfigured/unreachable/backend/
/// empty — all safe to surface (no message content leaks).
pub async fn improve(config: &AiConfig, draft: &str) -> Result<String, InferenceError> {
    chat(config, &improve_messages(draft), 0.3).await
}

/// Summarize an email thread via the configured backend (ADR 0011). The reading
/// pane calls this when a conversation opens.
///
/// # Errors
/// [`InferenceError`] variants for disabled/unconfigured/unreachable/backend/
/// empty — all safe to surface (no message content leaks).
pub async fn summarize(config: &AiConfig, thread: &str) -> Result<String, InferenceError> {
    chat(config, &summarize_messages(thread), 0.2).await
}

/// Translate one caption without adding, removing, or summarizing content.
pub async fn translate_text(
    config: &AiConfig,
    text: &str,
    target_language: &str,
) -> Result<String, InferenceError> {
    let messages = vec![
        ChatMessage {
            role: "system".to_owned(),
            content: format!(
                "Translate the supplied meeting caption into {target_language}. Preserve names, numbers, links, tone, and meaning. Return only the translation, with no commentary or quotation."
            ),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: text.to_owned(),
        },
    ];
    chat(config, &messages, 0.0).await
}

/// One retrieved item offered to the model as grounding for a workspace answer
/// (ADR 0029). The retrieval layer has already applied the caller's access, so
/// every source here is something they could open themselves — the model is
/// never shown more than the user can see.
#[derive(Debug, Clone)]
pub struct WorkspaceSource {
    /// 1-based citation number the model refers to (e.g. `[1]`).
    pub index: usize,
    /// `file` | `doc` | `base` | `folder` | `task` | `message`, plus the
    /// product-scoped kinds A1.3 grounds an agent in: `contact` | `event` |
    /// `chat`.
    pub kind: String,
    /// The item's name/title/subject.
    pub title: String,
    /// A short extra line (e.g. a task's description); empty when there is none.
    pub detail: String,
}

/// The system prompt for answering a question across the user's own workspace
/// (ADR 0029). It is strict about grounding: answer only from the listed
/// sources, cite them, and never invent — the product's trust promise.
const ASK_WORKSPACE_SYSTEM: &str = "You are the assistant inside the user's own private workspace. \
Answer their question using ONLY the numbered sources below — the files, tasks, and emails of theirs \
that matched a search. Cite every source you use by its number in square brackets, like [1] or [2]. \
Be concise and concrete, and answer in the question's language. If the sources do not contain the \
answer, say you could not find it in their workspace — never invent files, people, facts, or details \
that are not in the sources. Return only the answer — no preamble or heading.";

/// Renders the retrieved sources into the grounding block the model reads.
pub(crate) fn render_sources(sources: &[WorkspaceSource]) -> String {
    let mut out = String::new();
    for source in sources {
        let kind = match source.kind.as_str() {
            "message" => "email",
            "doc" => "document",
            // The product-scoped kinds (A1.3), said the way a person would say
            // them — a bare "chat" or "event" beside a title reads as a label
            // rather than as what the thing is.
            "chat" => "chat message",
            "event" => "calendar event",
            other => other,
        };
        out.push_str(&format!("[{}] {} \"{}\"", source.index, kind, source.title));
        if !source.detail.trim().is_empty() {
            out.push_str(" — ");
            out.push_str(source.detail.trim());
        }
        out.push('\n');
    }
    out
}

/// The chat messages for an "ask across my workspace" request. Pure and exported
/// so the prompt is testable without a backend.
#[must_use]
pub fn ask_workspace_messages(question: &str, sources: &[WorkspaceSource]) -> Vec<ChatMessage> {
    let user = format!(
        "Question: {}\n\nSources:\n{}",
        question.trim(),
        render_sources(sources)
    );
    vec![
        ChatMessage {
            role: "system".to_owned(),
            content: ASK_WORKSPACE_SYSTEM.to_owned(),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: user,
        },
    ]
}

/// Answer a question grounded in the caller's retrieved workspace items (ADR
/// 0029). The caller assembles `sources` from access-scoped search; this only
/// builds the prompt and calls the backend.
///
/// # Errors
/// [`InferenceError`] variants for disabled/unconfigured/unreachable/backend/
/// empty — all safe to surface (they carry no content).
pub async fn ask_workspace(
    config: &AiConfig,
    question: &str,
    sources: &[WorkspaceSource],
) -> Result<String, InferenceError> {
    chat(config, &ask_workspace_messages(question, sources), 0.2).await
}

/// The system prompt for document authoring (ADR 0029 §3). The result is always
/// a *proposal* the user approves before it enters their document — this prompt
/// only shapes the text; the caller never applies it silently.
const COMPOSE_SYSTEM: &str = "You help write a document. Follow the user's instruction to produce the \
text they asked for, using the current document (if given) only as context. Write in the document's \
language. Return ONLY the text to add or the revised text, formatted as plain Markdown — no preamble, \
explanation, or surrounding quotes.";

/// The chat messages for a "draft/continue/revise this document" request. Pure
/// and exported so the prompt is testable without a backend. `context` is the
/// current document text (may be empty for a from-scratch draft).
#[must_use]
pub fn compose_doc_messages(instruction: &str, context: &str) -> Vec<ChatMessage> {
    let user = if context.trim().is_empty() {
        format!("Instruction: {}", instruction.trim())
    } else {
        format!(
            "Current document:\n{}\n\nInstruction: {}",
            context.trim(),
            instruction.trim()
        )
    };
    vec![
        ChatMessage {
            role: "system".to_owned(),
            content: COMPOSE_SYSTEM.to_owned(),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: user,
        },
    ]
}

/// Draft or revise document text from an instruction and the current document
/// (ADR 0029 §3). Returns a *proposal*; the caller shows it and only writes it
/// to the document on the user's approval — never silently.
///
/// # Errors
/// [`InferenceError`] variants for disabled/unconfigured/unreachable/backend/
/// empty — all safe to surface (they carry no content).
pub async fn compose_doc(
    config: &AiConfig,
    instruction: &str,
    context: &str,
) -> Result<String, InferenceError> {
    chat(config, &compose_doc_messages(instruction, context), 0.4).await
}

/// A lightweight connectivity check for the admin "Test connection" action:
/// `GET {base}/v1/models`. Returns the number of models the endpoint reports.
/// Unlike [`improve`] it does not gate on `enabled` — the admin is testing a
/// config that may not be saved yet.
///
/// # Errors
/// [`InferenceError::NotConfigured`] for an empty base URL; `Backend`/`Transport`
/// on an HTTP failure; `Empty` if the response is not the expected shape.
pub async fn check(base_url: &str, api_key: Option<&str>) -> Result<usize, InferenceError> {
    if base_url.trim().is_empty() {
        return Err(InferenceError::NotConfigured);
    }
    let url = endpoint(base_url, "models");
    let client = build_client(&url, Duration::from_secs(20)).await?;
    let mut request = client.get(&url);
    if let Some(key) = api_key
        && !key.trim().is_empty()
    {
        request = request.bearer_auth(key.trim());
    }
    let response = request
        .send()
        .await
        .map_err(|_| InferenceError::Transport)?;
    if !response.status().is_success() {
        return Err(InferenceError::Backend(response.status().as_u16()));
    }
    let body = read_body_capped(response).await?;
    let parsed: serde_json::Value =
        serde_json::from_str(&body).map_err(|_| InferenceError::Empty)?;
    Ok(parsed
        .get("data")
        .and_then(|d| d.as_array())
        .map(Vec::len)
        .unwrap_or(0))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn cfg(enabled: bool, base: &str, model: &str) -> AiConfig {
        AiConfig {
            base_url: base.to_owned(),
            model: model.to_owned(),
            api_key: None,
            enabled,
        }
    }

    #[test]
    fn parse_replies_strips_markers_and_caps_at_three() {
        let out = parse_replies(
            "1. Sounds good, thanks!\n- Can you send the file?\n* No thanks\nExtra line",
        );
        assert_eq!(
            out,
            vec![
                "Sounds good, thanks!",
                "Can you send the file?",
                "No thanks"
            ]
        );

        let quoted = parse_replies("\"Yes, works for me\"\n'Let me check and revert'");
        assert_eq!(quoted, vec!["Yes, works for me", "Let me check and revert"]);

        assert!(parse_replies("   \n\n  ").is_empty());
    }

    #[test]
    fn improve_messages_are_system_then_user() {
        let m = improve_messages("Hey, wanna meet tmrw?");
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].role, "system");
        assert_eq!(m[1].role, "user");
        assert_eq!(m[1].content, "Hey, wanna meet tmrw?");
    }

    /// A1.3's product-scoped kinds are said the way a person says them: a bare
    /// "chat" or "event" beside a title reads as a label rather than as what
    /// the thing is, and the model is being told what it is looking at.
    #[test]
    fn the_product_scoped_kinds_are_rendered_as_words() {
        let sources = vec![
            WorkspaceSource {
                index: 1,
                kind: "chat".to_owned(),
                title: "the X100 shipped today".to_owned(),
                detail: String::new(),
            },
            WorkspaceSource {
                index: 2,
                kind: "event".to_owned(),
                title: "Acme review".to_owned(),
                detail: String::new(),
            },
            WorkspaceSource {
                index: 3,
                kind: "contact".to_owned(),
                title: "Acme Ltd".to_owned(),
                detail: String::new(),
            },
        ];
        let rendered = render_sources(&sources);
        assert!(rendered.contains("[1] chat message \"the X100 shipped today\""));
        assert!(rendered.contains("[2] calendar event \"Acme review\""));
        // A contact is already the word for what it is.
        assert!(rendered.contains("[3] contact \"Acme Ltd\""));
    }

    #[test]
    fn ask_workspace_prompt_lists_numbered_cited_sources() {
        let sources = vec![
            WorkspaceSource {
                index: 1,
                kind: "file".to_owned(),
                title: "Acme proposal.docx".to_owned(),
                detail: String::new(),
            },
            WorkspaceSource {
                index: 2,
                kind: "task".to_owned(),
                title: "Acme kickoff".to_owned(),
                detail: "Prepare the deck".to_owned(),
            },
            WorkspaceSource {
                index: 3,
                kind: "message".to_owned(),
                title: "Re: pricing".to_owned(),
                detail: String::new(),
            },
        ];
        let m = ask_workspace_messages("where is the acme proposal?", &sources);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].role, "system");
        // The system prompt enforces grounding + citation.
        assert!(m[0].content.contains("ONLY the numbered sources"));
        assert!(m[0].content.contains("square brackets"));
        // The user turn carries the question and each numbered source.
        let u = &m[1].content;
        assert!(u.contains("where is the acme proposal?"));
        assert!(u.contains("[1] file \"Acme proposal.docx\""));
        assert!(u.contains("[2] task \"Acme kickoff\" — Prepare the deck"));
        // 'message' is rendered as the user-facing "email".
        assert!(u.contains("[3] email \"Re: pricing\""));
    }

    #[test]
    fn compose_doc_prompt_includes_context_then_instruction() {
        let m = compose_doc_messages("continue the intro", "# Title\nSome text.");
        assert_eq!(m[0].role, "system");
        assert!(m[0].content.contains("proposal") || m[0].content.contains("ONLY"));
        let u = &m[1].content;
        assert!(u.contains("Current document:"));
        assert!(u.contains("Some text."));
        assert!(u.contains("Instruction: continue the intro"));
        // From-scratch: no context section.
        let blank = compose_doc_messages("write a haiku about the sea", "   ");
        assert!(!blank[1].content.contains("Current document:"));
        assert!(
            blank[1]
                .content
                .contains("Instruction: write a haiku about the sea")
        );
    }

    #[test]
    fn endpoint_appends_single_v1_regardless_of_base_shape() {
        // Ollama / custom: host root, no /v1.
        assert_eq!(
            endpoint("http://localhost:11434", "chat/completions"),
            "http://localhost:11434/v1/chat/completions"
        );
        // Hosted providers print the base *with* /v1 — must not double it.
        assert_eq!(
            endpoint("https://api.openai.com/v1", "chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            endpoint("https://api.anthropic.com/v1", "models"),
            "https://api.anthropic.com/v1/models"
        );
        // Trailing slashes (with or without /v1) are tolerated too.
        assert_eq!(
            endpoint("https://api.openai.com/v1/", "models"),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            endpoint("http://localhost:11434/", "models"),
            "http://localhost:11434/v1/models"
        );
    }

    #[test]
    fn parse_completion_extracts_and_trims_content() {
        let body =
            r#"{"choices":[{"message":{"role":"assistant","content":"  Improved text.  "}}]}"#;
        assert_eq!(parse_completion(body).unwrap(), "Improved text.");
    }

    #[test]
    fn parse_completion_rejects_empty_and_garbage() {
        assert!(matches!(parse_completion("{}"), Err(InferenceError::Empty)));
        assert!(matches!(
            parse_completion("not json"),
            Err(InferenceError::Empty)
        ));
        let no_text = r#"{"choices":[{"message":{"role":"assistant","content":"   "}}]}"#;
        assert!(matches!(
            parse_completion(no_text),
            Err(InferenceError::Empty)
        ));
    }

    #[tokio::test]
    async fn disabled_config_short_circuits_without_network() {
        let out = improve(&cfg(false, "http://localhost:11434", "llama3.2"), "hi").await;
        assert!(matches!(out, Err(InferenceError::Disabled)));
    }

    #[tokio::test]
    async fn check_requires_base_url() {
        assert!(matches!(
            check("", None).await,
            Err(InferenceError::NotConfigured)
        ));
    }

    #[tokio::test]
    async fn enabled_but_unconfigured_is_not_configured() {
        let out = improve(&cfg(true, "", "llama3.2"), "hi").await;
        assert!(matches!(out, Err(InferenceError::NotConfigured)));
        let out = improve(&cfg(true, "http://localhost:11434", "  "), "hi").await;
        assert!(matches!(out, Err(InferenceError::NotConfigured)));
    }
}
