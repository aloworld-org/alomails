//! An **intent** — one verb of an app, defined once (ADR 0058).
//!
//! Everything that happens in an alo app happens through an intent: the web
//! client's button, the HTTP route and the app's agent all dispatch the same
//! verb. This module is the *description* of a verb — its name, what it is
//! for in the module's own words, its typed arguments, whether it reads or
//! writes, the questions it answers, the preview a person is shown before a
//! write runs, and the inverse verb when the domain has one. The *execution*
//! lives beside the module's routes in `alo-jmap`, which is where the record
//! is.
//!
//! Three renderings read this one definition, so they cannot disagree:
//!
//! - the agent's prompt (`- name: purpose args: {…}` lines, [`IntentSpec::doc_line`]),
//! - the tool set the execution boundary allows ([`IntentSpec::tool`]),
//! - the agent directory's account of what an agent may do.
//!
//! **Coverage is structural.** A module lists, beside its intents, every route
//! it deliberately keeps from agents ([`Excluded`]) with the reason; a route
//! that is neither an intent's nor excluded fails the module's coverage test.
//! "The agent can do everything the app can do" is then a property of the
//! build, not a hope.

use serde_json::Value;

use crate::agent_tool::{AgentTool, Effect};

/// One argument of an intent, as the model is told about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arg {
    /// The key in the model's `args` object.
    pub name: &'static str,
    /// `text`, `integer`, `number`, `date`, `array` — the word the prompt uses.
    pub kind: &'static str,
    /// Whether the intent can run without it.
    pub required: bool,
    /// What it is, in one clause.
    pub purpose: &'static str,
}

impl Arg {
    /// An argument the intent cannot run without.
    #[must_use]
    pub const fn required(name: &'static str, kind: &'static str, purpose: &'static str) -> Self {
        Self {
            name,
            kind,
            required: true,
            purpose,
        }
    }

    /// An argument the intent has a default for.
    #[must_use]
    pub const fn optional(name: &'static str, kind: &'static str, purpose: &'static str) -> Self {
        Self {
            name,
            kind,
            required: false,
            purpose,
        }
    }
}

/// One verb of an app.
#[derive(Debug, Clone, Copy)]
pub struct IntentSpec {
    /// The name the model calls it by, and the audit record keeps.
    pub name: &'static str,
    /// What it does, in the module's words, addressed to whoever wants it done.
    pub purpose: &'static str,
    /// Reads answer, writes propose (ADR 0047).
    pub effect: Effect,
    /// Its arguments.
    pub args: &'static [Arg],
    /// Questions or requests this is the answer to — read by the planner and
    /// grown into the evaluation set.
    pub answers: &'static [&'static str],
    /// For a write: what will change, in one sentence, with `{arg}` holes the
    /// resolved arguments fill ([`render_preview`]). Shown before anyone taps.
    pub preview: Option<&'static str>,
    /// The inverse verb, when the domain has one (`discard_invoice_draft` for
    /// `create_invoice_draft`); `None` when it says so in its purpose.
    pub undo: Option<&'static str>,
    /// The routes this intent is the verb behind, for the coverage test.
    pub routes: &'static [&'static str],
}

impl IntentSpec {
    /// The tool the execution boundary and the prompt know this intent as.
    #[must_use]
    pub const fn tool(&self) -> AgentTool {
        AgentTool {
            name: self.name,
            effect: self.effect,
        }
    }

    /// The `- name: …` line the model reads, ending in a newline so lines
    /// concatenate into a list.
    #[must_use]
    pub fn doc_line(&self) -> String {
        let args = if self.args.is_empty() {
            "none".to_owned()
        } else {
            let inner: Vec<String> = self
                .args
                .iter()
                .map(|arg| {
                    format!(
                        "\"{}\": {} ({}, {})",
                        arg.name,
                        arg.kind,
                        if arg.required { "required" } else { "optional" },
                        arg.purpose
                    )
                })
                .collect();
            format!("{{{}}}", inner.join(", "))
        };
        format!("- {}: {} args: {}.\n", self.name, self.purpose, args)
    }
}

/// A route a module keeps from its agent, and why — so the coverage test can
/// tell a decision from an omission.
#[derive(Debug, Clone, Copy)]
pub struct Excluded {
    /// The route, as the router names it (`/billing/quotes/{id}/print`).
    pub route: &'static str,
    /// The reason, in one sentence.
    pub why: &'static str,
}

/// One module's verbs, its exclusions, and the paragraph of guidance its
/// agent reads after the tool lines.
#[derive(Debug, Clone, Copy)]
pub struct IntentModule {
    /// The verbs.
    pub intents: &'static [IntentSpec],
    /// The routes deliberately without a verb.
    pub excluded: &'static [Excluded],
    /// The module's paragraph in the agent's general instructions.
    pub guidance: &'static str,
}

impl IntentModule {
    /// The prompt's tool lines for this module.
    #[must_use]
    pub fn doc(&self) -> String {
        self.intents.iter().map(IntentSpec::doc_line).collect()
    }

    /// The tools the boundary allows for this module.
    #[must_use]
    pub fn tools(&self) -> Vec<AgentTool> {
        self.intents.iter().map(IntentSpec::tool).collect()
    }

    /// The intent behind a tool name.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&'static IntentSpec> {
        self.intents.iter().find(|intent| intent.name == name)
    }

    /// Whether a route is accounted for — by an intent or by an exclusion.
    #[must_use]
    pub fn covers(&self, route: &str) -> bool {
        self.intents
            .iter()
            .any(|intent| intent.routes.contains(&route))
            || self.excluded.iter().any(|excluded| excluded.route == route)
    }

    /// The routes under `prefix` in a router's source that this module does
    /// **not** account for — the coverage test's answer, empty when complete.
    #[must_use]
    pub fn uncovered(&self, router_source: &str, prefix: &str) -> Vec<String> {
        let mut missing: Vec<String> = routes_in(router_source, prefix)
            .into_iter()
            .filter(|route| !self.covers(route))
            .collect();
        missing.dedup();
        missing
    }
}

/// Every distinct string literal in `router_source` that begins with `prefix`
/// — the paths a router registers, read from its source because a router does
/// not enumerate itself.
#[must_use]
pub fn routes_in(router_source: &str, prefix: &str) -> Vec<String> {
    let mut routes: Vec<String> = router_source
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|literal| literal.starts_with(prefix))
        .map(ToOwned::to_owned)
        .collect();
    routes.sort();
    routes.dedup();
    routes
}

/// A preview with its `{arg}` holes filled from `args`. A hole with no value
/// is shown as `?` rather than as the hole, so a person sees what is unknown.
#[must_use]
pub fn render_preview(template: &str, args: &Value) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let key = &after[..close];
        let value = args.get(key).map_or_else(
            || "?".to_owned(),
            |v| match v {
                Value::String(s) => s.clone(),
                Value::Null => "?".to_owned(),
                other => other.to_string(),
            },
        );
        out.push_str(&value);
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUOTE: Arg = Arg::required("quote", "text", "the quote's number");
    const NOTE: Arg = Arg::optional("note", "text", "one extra sentence");

    const SEND: IntentSpec = IntentSpec {
        name: "send_quote",
        purpose: "Send an offer.",
        effect: Effect::Write,
        args: &[QUOTE, NOTE],
        answers: &["send the quote"],
        preview: Some("Quote {quote} for {customer} will be sent."),
        undo: None,
        routes: &["/billing/quotes/{id}/send"],
    };
    const OPEN: IntentSpec = IntentSpec {
        name: "open_quotes",
        purpose: "The offers still open.",
        effect: Effect::Read,
        args: &[],
        answers: &["which quotes are open"],
        preview: None,
        undo: None,
        routes: &["/billing/quotes"],
    };
    static MODULE: IntentModule = IntentModule {
        intents: &[OPEN, SEND],
        excluded: &[Excluded {
            route: "/billing/quotes/{id}/print",
            why: "serves a file",
        }],
        guidance: "Be exact.\n",
    };

    #[test]
    fn a_doc_line_says_the_arguments_and_ends_in_a_newline() {
        assert_eq!(
            SEND.doc_line(),
            "- send_quote: Send an offer. args: {\"quote\": text (required, the quote's number), \
             \"note\": text (optional, one extra sentence)}.\n"
        );
        assert_eq!(
            OPEN.doc_line(),
            "- open_quotes: The offers still open. args: none.\n"
        );
        let doc = MODULE.doc();
        assert!(doc.starts_with("- ") && doc.ends_with('\n'));
        assert_eq!(doc.matches("\n- ").count() + 1, MODULE.intents.len());
    }

    #[test]
    fn the_tool_carries_the_declared_effect() {
        assert_eq!(SEND.tool().effect, Effect::Write);
        assert_eq!(OPEN.tool().effect, Effect::Read);
        assert_eq!(MODULE.tools().len(), 2);
        assert!(MODULE.find("send_quote").is_some());
        assert!(MODULE.find("delete_everything").is_none());
    }

    #[test]
    fn coverage_is_intents_plus_exclusions_and_nothing_else() {
        let router = r#"
            .route("/billing/quotes", get(list).post(create))
            .route("/billing/quotes/{id}/send", post(send))
            .route("/billing/quotes/{id}/print", get(print))
            .route("/billing/quotes/{id}/pdf", get(pdf))
            .route("/chat/agents", get(agents))
        "#;
        assert_eq!(
            routes_in(router, "/billing/"),
            [
                "/billing/quotes",
                "/billing/quotes/{id}/pdf",
                "/billing/quotes/{id}/print",
                "/billing/quotes/{id}/send"
            ]
        );
        assert_eq!(
            MODULE.uncovered(router, "/billing/"),
            ["/billing/quotes/{id}/pdf"]
        );
    }

    #[test]
    fn a_preview_fills_what_it_knows_and_shows_what_it_does_not() {
        let args = serde_json::json!({ "quote": "QUO-2026-00007", "amount": 12 });
        assert_eq!(
            render_preview(
                "Quote {quote} for {customer} will be sent ({amount}).",
                &args
            ),
            "Quote QUO-2026-00007 for ? will be sent (12)."
        );
        assert_eq!(render_preview("no holes", &args), "no holes");
        assert_eq!(render_preview("broken {hole", &args), "broken {hole");
    }
}
