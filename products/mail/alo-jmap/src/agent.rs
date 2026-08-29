//! The "Ask alo" agent endpoints (ADR 0034) — the top-level agent that answers
//! or PROPOSES an action, plus the separate execute route that runs an APPROVED
//! action through the caller's tenant-scoped store.
//!
//! Two routes, one trust rule (ADR 0023/0034): `POST /ai/agent` **never acts** —
//! it returns an answer or a *proposed* action; `POST /ai/agent/execute` is the
//! **only** path that acts, and only for an action the user approved in the UI.
//! Both are authenticated and tenant-scoped: retrieval sees only what the caller
//! can see, and execution runs through `account.acc` (their tenant) — an agent
//! can never act outside the caller's own permissions.

use crate::agent_turn::{Turn, TurnContext, TurnResult, take_turn};
use alo_ai::{AiConfig, InferenceError, WorkspaceSource};
use alo_store::{
    CalendarEvent, ChatAgentId, ChatChannelId, EventId, MAX_PAGE, MailboxId, MessageId,
    NewAgentToolRun, Page,
};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use time::format_description::well_known::Rfc3339;

use crate::ai::MAX_ASK_BYTES;
use crate::error::Problem;
use crate::state::{Account, AppState, authenticate};

/// How many retrieved items ground one agent turn (mirrors `/ai/ask`).
const AGENT_SOURCES: i64 = 8;

/// Whether anybody approved this run (ADR 0047 §3).
///
/// This is the whole execution boundary. A reading tool may run either way; a
/// tool that **changes** something may run only under [`Approval::Asker`], and
/// that is checked here rather than asked for in the prompt — the model is the
/// untrusted party, and a prompt that asks nicely is not a permission system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Approval {
    /// Nobody approved it: it is running inside a turn, off the model's own
    /// choice, so it is a **read or nothing**.
    InTurn,
    /// The asker themselves asked for it — a `chat_proposals` row of their own
    /// that they approved, or their own tap in the command palette. Never
    /// anybody else's say-so: the run carries their reach.
    Asker,
}

/// One run of one tool: who approved it, and where it is happening.
///
/// The `agent` and `channel` are for the audit record (ADR 0047 §4) and are
/// `None` outside chat — the command palette's assistant is not a row in
/// `chat_agents` and is in no room.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolRun<'a> {
    /// Whose approval, if anybody's, this run carries.
    pub approval: Approval,
    /// The agent that ran it.
    pub agent: Option<&'a ChatAgentId>,
    /// The room it happened in.
    pub channel: Option<&'a ChatChannelId>,
}

impl ToolRun<'_> {
    /// A run the caller themselves asked for, outside any room — the command
    /// palette's execute route, where pressing the button *is* the approval.
    pub(crate) const fn approved() -> Self {
        Self {
            approval: Approval::Asker,
            agent: None,
            channel: None,
        }
    }
}

/// Said when a tool that changes something is reached from inside a turn.
///
/// Unreachable in practice — a turn only ever runs what the registry declares a
/// read — but the boundary states its own rule rather than trusting the layer
/// above to have applied it. It is phrased for the person, because both the
/// model and a room can end up reading it.
const NEEDS_APPROVAL: &str =
    "that changes something, so it waits for you to approve it — it cannot run on its own";

/// Said when an agent reaches for a tool belonging to another product (A1.2).
///
/// It names the product rather than apologising, because the model reads it
/// too and the useful correction is "that is somebody else's tool". Formatted
/// with the agent's own product and the tool it asked for; it carries nothing
/// about anybody's records.
fn out_of_product(product: alo_store::AgentProduct, tool: &str) -> String {
    format!(
        "{tool} is not a tool the {product} agent has — ask the agent whose product it belongs to"
    )
}

/// `POST /ai/agent` — `{"q":"..."}` → `{"answer":str|null,
/// "action":{tool,args,say}|null, "sources":[...],
/// "reason":null|"unconfigured"|"unreachable"}`.
///
/// Runs access-scoped retrieval, then takes a turn: a **reading** tool the model
/// chooses runs inside this request and its result grounds the answer, while a
/// tool that **changes** something comes back as an `action` for the UI to have
/// the user approve, which then calls `/ai/agent/execute` (ADR 0047). So a
/// returned `action` is always a change and never a lookup — asking what is in
/// stock gets the figure, not a button. Unlike `/ai/ask` it still calls the model
/// when retrieval is empty — the agent can act (e.g. create a task) without any
/// sources.
pub async fn agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    if body.len() > MAX_ASK_BYTES {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "request too large",
        ));
    }
    let req: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let request = req
        .get("q")
        .or_else(|| req.get("request"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    // The browser's own zone, when it sends one. Optional on purpose: a
    // caller that omits it still works, and is told so in the prompt.
    let tz = req.get("tz").and_then(Value::as_str).map(str::to_owned);
    // Remembered on sight. A preference nobody is prompted for is right far
    // more often than one they have to go and find, and a chat agent turn has
    // no browser to ask.
    if let Some(zone) = tz.as_deref() {
        let _ = account.acc.set_user_timezone(zone).await;
    }
    if request.is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "q required"));
    }

    // Access-scoped retrieval — the only thing the agent may ever see. The
    // palette IS "Ask alo", the one agent that looks across every product
    // (ADR 0034), so this is the workspace-wide view by decision rather than by
    // default: `agent_ground` scopes every *other* product to its own records.
    let hits = account
        .acc
        .agent_ground(alo_store::AgentProduct::Workspace, &request, AGENT_SOURCES)
        .await
        .map_err(|_| Problem::server_error())?;
    let sources_json: Vec<Value> = hits
        .iter()
        .map(|h| json!({ "kind": h.kind, "id": h.id, "title": h.title, "space": h.space }))
        .collect();
    let ground: Vec<WorkspaceSource> = hits
        .iter()
        .enumerate()
        .map(|(i, h)| WorkspaceSource {
            index: i + 1,
            kind: h.kind.clone(),
            title: h.title.clone(),
            detail: String::new(),
        })
        .collect();

    let Some(row) = account
        .acc
        .default_ai_config()
        .await
        .map_err(|_| Problem::server_error())?
    else {
        return Ok(Json(json!({
            "answer": Value::Null, "action": Value::Null,
            "reason": "unconfigured", "sources": sources_json
        })));
    };
    let config = AiConfig {
        base_url: row.base_url,
        model: row.model,
        api_key: row.api_key,
        enabled: row.enabled,
    };
    // (kind, id, title) per retrieved item, so a proposed email action referring
    // to a source by its number can be resolved to the concrete message id here —
    // execute never re-searches, and the model never sees raw ids.
    let sources: Vec<(String, String, String)> = hits
        .iter()
        .map(|h| (h.kind.clone(), h.id.clone(), h.title.clone()))
        .collect();
    // The user's mail folders the agent may move a message into — real names
    // only, so it can't propose a folder that does not exist (internal roles
    // like Snoozed/Scheduled are excluded; Archive/Trash have their own tools).
    let folders = movable_folder_names(&account).await;
    // Whatever the caller sent, else whatever this person's browser told us
    // last time. Unknown stays unknown rather than becoming the server's zone.
    let known_tz = match tz {
        Some(zone) => Some(zone),
        None => account.acc.user_timezone().await.unwrap_or_default(),
    };
    let today = today_where(known_tz.as_deref());
    let turn = Turn {
        // The palette IS "Ask alo" — the top-level agent ADR 0034 scopes to no
        // product, because working across them is the whole job.
        product: alo_store::AgentProduct::Workspace,
        request: &request,
        sources: &ground,
        today: &today,
        folders: &folders,
        context: TurnContext::palette(),
        // No handoffs outside a room: there is nowhere for the handoff line to
        // be seen, and Ask alo's own delegation path is the planner.
        roster: &[],
    };
    // The palette has no room, so its turn feeds no channel memory: the read
    // list beside the result is dropped here on purpose (A6.1).
    match take_turn(&state, &account, &config, &turn)
        .await
        .map(|(result, _)| result)
    {
        Ok(TurnResult::Answer(answer)) => Ok(Json(json!({
            "answer": answer, "action": Value::Null,
            "reason": Value::Null, "sources": sources_json
        }))),
        Ok(TurnResult::Propose { mut action, say }) => {
            resolve_email_source(&mut action.args, &sources);
            Ok(Json(json!({
                "answer": Value::Null,
                "action": { "tool": action.tool, "args": action.args, "say": say },
                "reason": Value::Null, "sources": sources_json
            })))
        }
        // Unreachable in practice: the palette's turn is offered no roster, so
        // no delegate ever runs, let alone proposes. Stated for the compiler,
        // and shaped as "nothing came of it" rather than an invented sentence.
        Ok(TurnResult::DelegateProposed) => Ok(Json(json!({
            "answer": Value::Null, "action": Value::Null,
            "reason": Value::Null, "sources": sources_json
        }))),
        Err(InferenceError::Disabled | InferenceError::NotConfigured) => Ok(Json(json!({
            "answer": Value::Null, "action": Value::Null,
            "reason": "unconfigured", "sources": sources_json
        }))),
        Err(_) => Ok(Json(json!({
            "answer": Value::Null, "action": Value::Null,
            "reason": "unreachable", "sources": sources_json
        }))),
    }
}

/// `POST /ai/agent/execute` — `{"tool":"create_task","args":{...}}` →
/// `{"ok":true,"result":{...}}`.
///
/// The **only** acting path. Validates the tool against the allowlist
/// ([`alo_ai::AGENT_TOOLS`]) and its args, then runs it through the caller's
/// tenant-scoped store. Called only after the user approved the proposed action.
pub async fn agent_execute(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    if body.len() > MAX_ASK_BYTES {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "request too large",
        ));
    }
    let req: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let tool = req.get("tool").and_then(Value::as_str).unwrap_or("").trim();
    let args = req.get("args").cloned().unwrap_or(Value::Null);
    // Pressing the button in the palette IS the approval, and it is the
    // caller's own: this route is reached from their session and from nowhere
    // else.
    execute_tool(&state, &account, tool, &args, &ToolRun::approved()).await
}

/// Run one approved tool through the caller's own store.
///
/// The **only** acting path, shared by every surface that can approve
/// something: the command palette's execute route above, and chat's proposal
/// approval. One allowlist and one dispatcher, so a tool cannot be reachable
/// from one surface and unreachable from another, and a new tool cannot be
/// half-wired.
///
/// # Errors
/// 400 for a tool outside the allowlist, 422 for arguments that do not name
/// something real, whatever the executor itself raises otherwise.
pub(crate) async fn execute_tool(
    state: &AppState,
    account: &Account,
    tool: &str,
    args: &Value,
    run: &ToolRun<'_>,
) -> Result<Json<Value>, Problem> {
    let all = alo_ai::all_tools();
    let Some(entry) = alo_ai::find_tool(&all, tool).copied() else {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "unknown tool"));
    };
    // A1.2, ADR 0034: an agent may only use its own product's tools, and this
    // is where that is *true* rather than merely offered. The product is read
    // from the agent's own row, never taken from the caller — see [`scope`].
    let product = scope(account, run).await?;
    if !alo_ai::offers(product, tool) {
        record_run(account, run, &entry, args, false).await;
        return Err(Problem::with(
            StatusCode::FORBIDDEN,
            out_of_product(product, tool),
        ));
    }
    // ADR 0047 §3, and the reason "reads only" is true of a turn rather than
    // merely asked of it. The registry says what this tool does and the caller
    // says whose approval it carries; nothing the model returned is consulted.
    if must_wait_for_approval(entry, run.approval) {
        record_run(account, run, &entry, args, false).await;
        return Err(Problem::with(StatusCode::FORBIDDEN, NEEDS_APPROVAL));
    }
    let done = dispatch(state, account, tool, args).await;
    // ADR 0047 §4: both paths leave a row, and a refusal leaves one too.
    record_run(account, run, &entry, args, done.is_ok()).await;
    // ADR 0058 §5: every intent execution emits an event on the tenant's
    // stream. Executions only — a refusal is not something that happened.
    if let Ok(reply) = &done {
        emit_event(account, run, &entry, &reply.0).await;
    }
    done
}

/// Which product's tools this run may use (A1.2).
///
/// **Read from the agent's row through the caller's own store**, not passed in
/// by whoever built the [`ToolRun`]. A caller that could state its own scope
/// would be one refactor away from stating a wider one, and the mistake would
/// be invisible: every test would still pass, because the tools would still
/// run. The lookup is one indexed row in a path that is about to call a model.
///
/// No agent means the command palette's assistant, which **is** "Ask alo" —
/// the one agent ADR 0034 scopes to no product because working across them is
/// its job.
///
/// # Errors
/// 403 when the agent cannot be read at all. Fails closed: a deleted agent, a
/// wrong tenant, or a product word this binary does not know are all reasons
/// to run nothing, and the tools an unreadable row might have wanted are
/// exactly the ones worth refusing.
async fn scope(account: &Account, run: &ToolRun<'_>) -> Result<alo_store::AgentProduct, Problem> {
    let Some(id) = run.agent else {
        return Ok(alo_store::AgentProduct::Workspace);
    };
    account
        .acc
        .agent(id)
        .await
        .map(|agent| agent.product)
        .map_err(|_| Problem::with(StatusCode::FORBIDDEN, UNKNOWN_AGENT))
}

/// Said when the agent a run claims to be cannot be read.
const UNKNOWN_AGENT: &str = "that agent is not one of this workspace's, so nothing was run";

/// The execution boundary's whole rule (ADR 0047 §3): **a tool that changes
/// something may not run unless the asker approved it.**
///
/// A function rather than an inline condition so the rule can be checked against
/// the entire registry in one test, without a database and without a model. That
/// matters more here than anywhere else in the turn: it is the only thing
/// standing between an injected prompt and a write, and it is unreachable in
/// practice — [`crate::agent_turn`] never offers it a write — so nothing else
/// would notice if it were quietly inverted.
const fn must_wait_for_approval(entry: alo_ai::AgentTool, approval: Approval) -> bool {
    !entry.is_read() && matches!(approval, Approval::InTurn)
}

/// Write the audit row for one run (ADR 0047 §4).
///
/// **Best-effort on purpose.** A read the caller was entitled to must not fail
/// because a logline could not be written — that trades a working product for
/// an audit trail nobody asked to be strict. The failure is reported to the
/// operator's log instead, with the tool's name and never its arguments: those
/// carry the caller's own content.
async fn record_run(
    account: &Account,
    run: &ToolRun<'_>,
    entry: &alo_ai::AgentTool,
    args: &Value,
    ok: bool,
) {
    if let Err(err) = account
        .acc
        .record_tool_run(&NewAgentToolRun {
            agent: run.agent,
            channel: run.channel,
            tool: entry.name,
            effect: entry.effect.as_str(),
            args,
            ok,
        })
        .await
    {
        tracing::warn!(tool = entry.name, error = %err, "agent tool run not recorded");
    }
}

/// Emit one event onto the tenant's stream for an execution that happened
/// (ADR 0058 §5).
///
/// **Best-effort on purpose**, exactly like [`record_run`]: the execution has
/// already answered, and undoing an act because its event could not be
/// written would trade a working product for a stream nobody asked to be
/// strict. The failure is reported to the operator's log with the verb's name
/// and never its arguments or result.
async fn emit_event(
    account: &Account,
    run: &ToolRun<'_>,
    entry: &alo_ai::AgentTool,
    reply: &Value,
) {
    let record = event_record_ref(reply);
    let event = alo_store::NewDomainEvent {
        kind: entry.name,
        effect: entry.effect.as_str(),
        record_type: record.as_ref().map(|(kind, _)| kind.as_str()),
        record_id: record.as_ref().map(|(_, id)| id.as_str()),
        agent: run.agent,
    };
    if let Err(err) = account.acc.emit_event(&event).await {
        tracing::warn!(tool = entry.name, error = %err, "intent event not emitted");
    }
}

/// The record an execution's reply names, when it names exactly one.
///
/// Executors answer `{"ok":true,"result":…}` where a result about one record
/// carries its record word as `kind` and the record's id either at the top
/// (`{"kind":"task","id":…}`) or on the record filed under the word itself
/// (`{"kind":"quote","quote":{"id":…}}`). Both shapes are read; a result that
/// is not about one record — a list, a total, a report — yields `None`, which
/// is an event without a record reference rather than a wrong one.
fn event_record_ref(reply: &Value) -> Option<(String, String)> {
    let result = reply.get("result")?;
    let kind = result.get("kind")?.as_str()?;
    let id = result
        .get("id")
        .or_else(|| result.get(kind).and_then(|record| record.get("id")))
        .and_then(Value::as_str)?;
    Some((kind.to_owned(), id.to_owned()))
}

/// Run one tool, once it is allowed to run. Split out so
/// One verb's run, boxed so every module's dispatcher has the one shape.
pub(crate) type Dispatched<'a> =
    Pin<Box<dyn Future<Output = Result<Json<Value>, Problem>> + Send + 'a>>;

/// A module's dispatcher: its verbs by name, `None` for a verb that is not
/// its, so the loop below asks the next module.
pub(crate) type ModuleDispatcher =
    for<'a> fn(&'a AppState, &'a Account, &'a str, &'a Value) -> Option<Dispatched<'a>>;

/// The modules that have moved to intents (ADR 0058), one row each — the
/// whole of a module's registration in this file (A4.1c). A loop that lands a
/// module adds its row here and nothing to the match below; two loops landing
/// at once conflict on neighbouring lines, and the resolution is to keep both.
/// The registry's twin is `alo_ai::MOVED`, and a test in each module holds the
/// two lists to the same length.
pub(crate) const MODULES: &[ModuleDispatcher] = &[
    crate::agenda_intents::dispatch,
    crate::billing_intents::dispatch,
    crate::chat_intents::dispatch,
    crate::crm_intents::dispatch,
    crate::docs_intents::dispatch,
    crate::drive_intents::dispatch,
    crate::finance_intents::dispatch,
    crate::hr_intents::dispatch,
    crate::insights_intents::dispatch,
    crate::inventory_intents::dispatch,
    crate::mail_intents::dispatch,
    crate::meet_intents::dispatch,
    crate::projects_intents::dispatch,
    crate::sheets_intents::dispatch,
    crate::sites_intents::dispatch,
    crate::tasks_intents::dispatch,
];

/// [`execute_tool`]'s boundary check and audit cannot be bypassed by a caller
/// reaching the dispatcher directly.
async fn dispatch(
    state: &AppState,
    account: &Account,
    tool: &str,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    for module in MODULES {
        if let Some(run) = module(state, account, tool, args) {
            return run.await;
        }
    }
    // With Agenda's move (AB.5) every module's verbs are dispatched by a row
    // in MODULES, and the per-tool match this function grew up as is gone.
    // Executors that predate the split stay in this file where their plumbing
    // lives — `execute_create_event` (Meet's `schedule_meeting` runs the same
    // shared calendar write), Mail's draft/submission/folder handlers — and
    // are reached through their modules' dispatchers. Unreachable given the
    // allowlist check, but the boundary states its own refusal.
    Err(Problem::with(StatusCode::BAD_REQUEST, "unknown tool"))
}

/// Replace `{"source": n}` in an action's args with the concrete email it refers
/// to (`message_id` + `subject`), from the retrieval results. Only resolves when
/// the referenced source is an email; leaves non-email or source-less args
/// untouched. Pure so the mapping is unit-tested.
///
/// Shared with every surface that stores a proposal: the palette resolves
/// before returning the action to the browser, and the room paths
/// ([`crate::chat_agent`], [`crate::agent_turn::delegate_turn`]) resolve
/// before `propose_action`, so an approval tap never meets a source number
/// the executor cannot read.
pub(crate) fn resolve_email_source(args: &mut Value, sources: &[(String, String, String)]) {
    let Some(n) = args.get("source").and_then(Value::as_u64) else {
        return;
    };
    let Some((kind, id, title)) = (n as usize).checked_sub(1).and_then(|i| sources.get(i)) else {
        return;
    };
    if kind != "message" {
        return;
    }
    if let Some(obj) = args.as_object_mut() {
        obj.remove("source");
        obj.insert("message_id".to_owned(), json!(id));
        obj.insert("subject".to_owned(), json!(title));
    }
}

/// Read the resolved `message_id` from an email action's args.
fn message_id_arg(args: &Value) -> Result<MessageId, Problem> {
    let id = args
        .get("message_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if id.is_empty() {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "message required",
        ));
    }
    Ok(MessageId::new(id.to_owned()))
}

/// Set or clear a keyword ($seen for read, $flagged for flag) on an email.
pub(crate) async fn execute_set_keyword(
    account: &Account,
    args: &Value,
    keyword: &str,
    flag_field: &str,
) -> Result<Json<Value>, Problem> {
    let msg = message_id_arg(args)?;
    // Default the boolean to true ("mark read" / "flag" without an explicit value).
    let on = args
        .get(flag_field)
        .and_then(Value::as_bool)
        .unwrap_or(true);
    account
        .acc
        .set_keyword(&msg, keyword, on)
        .await
        .map_err(|_| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "could not update the email",
            )
        })?;
    Ok(Json(
        json!({ "ok": true, "result": { "kind": "email", "id": msg.as_str() } }),
    ))
}

/// Move an email into a standard role mailbox (Archive or Trash) and take it out
/// of the places it would otherwise still show — the Inbox, and (when trashing)
/// the Archive — so it lives only in its new home. The destination mailbox is
/// created on first use, the same on-demand idiom every other standard role uses
/// (Inbox, Drafts, Snoozed, Scheduled): a first move on an account that never had
/// the folder should succeed, not fail.
pub(crate) async fn execute_move_to_role(
    account: &Account,
    args: &Value,
    role: &str,
    name: &str,
) -> Result<Json<Value>, Problem> {
    let msg = message_id_arg(args)?;
    let dest = crate::drafts::role_mailbox(account, role, name).await?;
    relocate_message(account, &msg, &dest).await?;
    Ok(Json(
        json!({ "ok": true, "result": { "kind": "email", "id": msg.as_str() } }),
    ))
}

/// Move an email into one of the user's named folders. Unlike the role moves,
/// the destination is resolved by name among the account's existing folders and
/// is never created — a folder the user did not name back is a clean error, not
/// a new empty folder from a typo.
pub(crate) async fn execute_move_to_folder(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let msg = message_id_arg(args)?;
    let wanted = args
        .get("folder")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if wanted.is_empty() {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "a folder name is required",
        ));
    }
    let dest = movable_folders(account)
        .await?
        .into_iter()
        .find(|m| m.name.eq_ignore_ascii_case(wanted))
        .map(|m| MailboxId::new(m.id.as_str()))
        .ok_or_else(|| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, "no folder by that name"))?;
    relocate_message(account, &msg, &dest).await?;
    Ok(Json(
        json!({ "ok": true, "result": { "kind": "email", "id": msg.as_str() } }),
    ))
}

/// Add `msg` to `dest`, then take it out of the "still visible" origins (Inbox,
/// Archive) other than `dest` itself, so a moved message lives only in its new
/// home. Skipping `dest` by id keeps a move *into* the Inbox or Archive from
/// removing what it just added. Removals are best-effort: if the message was not
/// in an origin, the move still stands.
async fn relocate_message(
    account: &Account,
    msg: &MessageId,
    dest: &MailboxId,
) -> Result<(), Problem> {
    account
        .acc
        .add_to_mailbox(msg, dest)
        .await
        .map_err(|_| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, "could not move the email"))?;
    for origin in ["inbox", "archive"] {
        if let Ok(Some(box_id)) = account.acc.mailbox_by_role(origin).await {
            if box_id.as_str() == dest.as_str() {
                continue;
            }
            match account.acc.remove_from_mailbox(msg, &box_id).await {
                Ok(()) | Err(_) => {}
            }
        }
    }
    Ok(())
}

/// The account's folders that a message may be moved into: every mailbox except
/// the internal, non-user-facing roles (Snoozed, Scheduled).
async fn movable_folders(account: &Account) -> Result<Vec<alo_store::Mailbox>, Problem> {
    let boxes = account
        .acc
        .mailboxes(Page::first(MAX_PAGE))
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(boxes
        .into_iter()
        .filter(|m| !matches!(m.role.as_deref(), Some("snoozed" | "scheduled")))
        .collect())
}

/// Just the names of [`movable_folders`], for grounding the agent's proposal.
/// Best-effort: a lookup failure yields no folders (the agent then declines a
/// move rather than guessing), never an error on the propose path.
async fn movable_folder_names(account: &Account) -> Vec<String> {
    movable_folders(account)
        .await
        .map(|v| v.into_iter().map(|m| m.name).collect())
        .unwrap_or_default()
}

/// Snooze an email until a chosen time: hide it from the Inbox into Snoozed; the
/// store's sweeper returns it to the Inbox (unread) once the wake time passes.
pub(crate) async fn execute_snooze(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let msg = message_id_arg(args)?;
    let epoch = args
        .get("until")
        .and_then(Value::as_str)
        .and_then(|s| parse_wake_time(s, time::OffsetDateTime::now_utc()))
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "a future RFC 3339 wake time is required",
            )
        })?;
    // Snooze hides the message from the Inbox, so that is the mailbox to move it
    // out of; ensure it exists for the rare account that has none yet.
    let inbox = crate::drafts::role_mailbox(account, "inbox", "Inbox").await?;
    account
        .acc
        .snooze(std::slice::from_ref(&msg), &inbox, epoch)
        .await
        .map_err(|_| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "could not snooze the email",
            )
        })?;
    Ok(Json(
        json!({ "ok": true, "result": { "kind": "email", "id": msg.as_str() } }),
    ))
}

/// Draft a NEW email from approved args and save it to the user's Drafts (marked
/// `$draft`) for them to review and send — this path never sends. The visible
/// `From` is always the caller's own canonical address (resolved server-side, so
/// a draft can never impersonate another author, even before it is sent); only
/// the recipient, subject, and body come from the approved proposal.
pub(crate) async fn execute_draft_email(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Result<Json<Value>, Problem> {
    let to = args
        .get("to")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    if !crate::submission::valid_addr(&to) {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "a valid recipient address is required",
        ));
    }
    let body = draft_body_arg(args)?;
    let subject = args
        .get("subject")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();

    let from = crate::drafts::from_address(account, state).await?;
    let outgoing = compose(&from, vec![to], subject, body, Vec::new(), Vec::new());
    let id = crate::drafts::save(account, &outgoing).await?;
    Ok(Json(
        json!({ "ok": true, "result": { "kind": "draft", "id": id.as_str() } }),
    ))
}

/// Draft a REPLY to an email in the sources and save it to Drafts (`$draft`),
/// never sent. The reply is addressed to the original sender, keeps the subject
/// in the same thread (`Re:` once, not stacked), and carries `In-Reply-To` +
/// `References` so mail clients thread it. The body is the approved text; `From`
/// is the caller's own address, exactly as for a new draft.
pub(crate) async fn execute_draft_reply(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Result<Json<Value>, Problem> {
    let mid = message_id_arg(args)?;
    let body = draft_body_arg(args)?;
    // The account door scopes this: a foreign or absent id is a clean not-found,
    // never another tenant's message.
    let bytes = account.acc.message_bytes(&mid).await.map_err(|_| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "the email to reply to was not found",
        )
    })?;
    let to = crate::submission::extract_from_addr(&bytes)
        .filter(|a| crate::submission::valid_addr(a))
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "the original email has no address to reply to",
            )
        })?;
    let parsed = alo_store::message::parse(&bytes);
    let subject = reply_subject(&parsed.subject);
    let in_reply_to: Vec<String> = parsed
        .message_id
        .as_deref()
        .map(strip_brackets)
        .filter(|s| !s.is_empty())
        .into_iter()
        .collect();
    let references = reply_references(&parsed.referenced_ids, parsed.message_id.as_deref());

    let from = crate::drafts::from_address(account, state).await?;
    let outgoing = compose(&from, vec![to], subject, body, in_reply_to, references);
    let id = crate::drafts::save(account, &outgoing).await?;
    Ok(Json(
        json!({ "ok": true, "result": { "kind": "draft", "id": id.as_str() } }),
    ))
}

/// The required, non-empty `body` of a draft/reply.
fn draft_body_arg(args: &Value) -> Result<String, Problem> {
    let body = args
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    if body.is_empty() {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "the email body is required",
        ));
    }
    Ok(body)
}

/// Assemble a plain-text outgoing message. The `mime` builder CR/LF-sanitizes
/// every header and RFC 2047-encodes non-ASCII, so composed fields carry no
/// header-injection path.
fn compose(
    from: &str,
    to: Vec<String>,
    subject: String,
    body: String,
    in_reply_to: Vec<String>,
    references: Vec<String>,
) -> crate::mime::Outgoing {
    crate::mime::Outgoing {
        from: crate::mime::Addr {
            name: None,
            email: from.to_owned(),
        },
        to: to
            .into_iter()
            .map(|email| crate::mime::Addr { name: None, email })
            .collect(),
        cc: Vec::new(),
        bcc: Vec::new(),
        subject,
        in_reply_to,
        references,
        body_text: body,
        body_html: None,
        attachments: Vec::new(),
        message_id_domain: crate::api::domain_of(from),
        message_id_token: crate::api::new_message_token(),
    }
}

/// SEND a draft the user already has. This is the one agent action that leaves
/// the server, so it is deliberately narrow: it runs only after the user approves
/// the proposal, and it reuses the JMAP `EmailSubmission/set` path verbatim —
/// which submits only a `$draft` through the trusted DKIM-signing listener and
/// then moves it to Sent. The agent therefore can never send an arbitrary
/// message (a non-draft is refused there) and there is no second send path to
/// drift from the audited one.
pub(crate) async fn execute_send(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Result<Json<Value>, Problem> {
    let mid = message_id_arg(args)?;
    // Build the send envelope from the draft's own recipients (To/Cc/Bcc), the
    // way a compose client does. The account door scopes this fetch: a foreign or
    // absent id is a clean not-found, never another tenant's message.
    let bytes = account.acc.message_bytes(&mid).await.map_err(|_| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "the draft to send was not found",
        )
    })?;
    let parsed = alo_store::message::parse(&bytes);
    let mut rcpts: Vec<String> = Vec::new();
    for header in [&parsed.to_addrs, &parsed.cc_addrs, &parsed.bcc_addrs] {
        for addr in addr_specs(header) {
            if !rcpts.contains(&addr) {
                rcpts.push(addr);
            }
        }
    }
    if rcpts.is_empty() {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "the draft has no recipient to send to",
        ));
    }
    let rcpt_to: Vec<Value> = rcpts.iter().map(|e| json!({ "email": e })).collect();
    let sub_args = json!({
        "accountId": account.account_id(),
        "create": { "a": { "emailId": mid.as_str(), "envelope": { "rcptTo": rcpt_to } } },
    });
    let res = crate::submission::set(account, &sub_args, state)
        .await
        .map_err(|_| Problem::server_error())?;
    if let Some(created) = res.get("created").and_then(|c| c.get("a")) {
        let sent = created
            .get("emailId")
            .and_then(Value::as_str)
            .unwrap_or_else(|| mid.as_str());
        return Ok(Json(
            json!({ "ok": true, "result": { "kind": "sent", "id": sent } }),
        ));
    }
    // notCreated carries the specific reason (not a draft, no send listener,
    // notFound, forbidden) — surface it, without any recipient/body detail.
    let reason = res
        .get("notCreated")
        .and_then(|n| n.get("a"))
        .and_then(|e| e.get("description"))
        .and_then(Value::as_str)
        .unwrap_or("the message could not be sent")
        .to_owned();
    Err(Problem::with(StatusCode::UNPROCESSABLE_ENTITY, reason))
}

/// The addr-specs in an address-list header value (`To`/`Cc`/`Bcc`), for building
/// a send envelope. Handles `Name <a@b>, c@d` forms: takes the address inside
/// angle brackets, else a bare token that looks like an address. A stray comma
/// inside a quoted display name only yields a junk fragment with no `@`, which is
/// dropped — the real address in the same entry is still recovered.
pub(crate) fn addr_specs(header_value: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in header_value.split(',') {
        let part = part.trim();
        let addr = match (part.rfind('<'), part.rfind('>')) {
            (Some(lt), Some(gt)) if lt < gt => part[lt + 1..gt].trim(),
            _ => part,
        };
        if addr.contains('@') && !addr.contains(' ') {
            out.push(addr.to_lowercase());
        }
    }
    out
}

/// Strip the surrounding angle brackets from a message-id, leaving the bare id
/// the `mime` builder expects (it re-wraps in `<…>`).
fn strip_brackets(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim()
        .to_owned()
}

/// The subject for a reply: prefix `Re: ` unless one is already present
/// (case-insensitive), so replies-to-replies don't stack `Re: Re:`.
fn reply_subject(original: &str) -> String {
    let t = original.trim();
    if t.is_empty() {
        "Re:".to_owned()
    } else if t.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("re:")) {
        t.to_owned()
    } else {
        format!("Re: {t}")
    }
}

/// The reply's `References` chain (RFC 5322 §3.6.4): the parent's own referenced
/// ids followed by the parent's `Message-ID`, brackets stripped and duplicates
/// removed, order preserved.
fn reply_references(parent_refs: &[String], parent_id: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in parent_refs.iter().map(String::as_str).chain(parent_id) {
        let bare = strip_brackets(raw);
        if !bare.is_empty() && !out.contains(&bare) {
            out.push(bare);
        }
    }
    out
}

/// Schedule a calendar event from approved args, on the caller's personal
/// calendar. Reuses the same tenant-scoped `create_event` the `/calendar/events`
/// route uses (which checks edit permission on the calendar) — no new path.
pub(crate) async fn execute_create_event(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let title = args
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    if title.is_empty() {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "title required",
        ));
    }
    let starts_at = args
        .get("start")
        .and_then(Value::as_str)
        .and_then(parse_rfc3339)
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "a valid RFC 3339 start is required",
            )
        })?;
    // End defaults to one hour after start; a given end before start is ignored.
    let ends_at = args
        .get("end")
        .and_then(Value::as_str)
        .and_then(parse_rfc3339)
        .filter(|e| *e >= starts_at)
        .unwrap_or_else(|| starts_at + time::Duration::hours(1));
    let clean = |k: &str| {
        args.get(k)
            .and_then(Value::as_str)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    };

    let calendar_id = account
        .acc
        .ensure_personal_calendar()
        .await
        .map_err(|_| Problem::server_error())?;
    let event = CalendarEvent {
        id: EventId::generate(),
        calendar_id,
        summary: title.clone(),
        description: clean("notes"),
        location: clean("location"),
        starts_at,
        ends_at,
        all_day: false,
        recurrence: None,
        attendees: Vec::new(),
        exdates: Vec::new(),
        timezone: None,
        rdates: Vec::new(),
        recurrence_id: None,
        reminder_minutes: None,
        attendee_status: Vec::new(),
    };
    let id = account
        .acc
        .create_event(&event)
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(json!({
        "ok": true,
        "result": { "kind": "event", "id": id.as_str(), "title": title }
    })))
}

/// Parse an RFC 3339 datetime to UTC, or `None` if malformed.
fn parse_rfc3339(s: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(s.trim(), &Rfc3339)
        .ok()
        .map(|t| t.to_offset(time::UtcOffset::UTC))
}

/// Parse an RFC 3339 wake time and require it to be strictly after `now`,
/// returning its Unix-second epoch. `None` if malformed or not in the future — a
/// past wake time would un-snooze the message immediately, so it is rejected.
fn parse_wake_time(s: &str, now: time::OffsetDateTime) -> Option<i64> {
    let until = parse_rfc3339(s)?;
    (until > now).then(|| until.unix_timestamp())
}

/// The current UTC date as `YYYY-MM-DD`, given to the agent so it can resolve
/// relative dates ("tomorrow") into absolute ones.
fn today_utc() -> String {
    let d = time::OffsetDateTime::now_utc().date();
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

/// Today's date **and the clock the asker is on**, for the prompt.
///
/// A model given a bare UTC date reads "Thursday at 10" as 10:00Z. For anyone
/// not on UTC that is the wrong hour — a wrong answer from `am_i_free`, and a
/// meeting in the wrong slot from `create_event`. The browser knows the zone,
/// so it sends it; when it does not, the model is told the times it produces
/// will be read as UTC, so it can say so rather than quietly assume.
fn today_where(tz: Option<&str>) -> String {
    let today = today_utc();
    match tz {
        Some(zone) if !zone.trim().is_empty() => format!(
            "{today}, and the person asking is in the {} timezone. Every datetime you              produce must be an instant that means the time THEY said on THEIR clock.",
            zone.trim()
        ),
        _ => format!(
            "{today}. The person's timezone is unknown, so any datetime you produce is              read as UTC — say which hour you assumed in your `say` line."
        ),
    }
}

/// Parse a `YYYY-MM-DD` due date to midnight UTC, or `None` if malformed (a bad
/// date drops the due, never fails the task — the title is the essential part).
/// `pub(crate)` because `create_task`'s executor moved out with its module
/// (AB.4) and still parses a due date exactly this way.
pub(crate) fn parse_due(s: &str) -> Option<time::OffsetDateTime> {
    let mut it = s.trim().split('-');
    let year: i32 = it.next()?.parse().ok()?;
    let month: u8 = it.next()?.parse().ok()?;
    let day: u8 = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    let month = time::Month::try_from(month).ok()?;
    let date = time::Date::from_calendar_date(year, month, day).ok()?;
    Some(date.midnight().assume_utc())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{
        Approval, addr_specs, event_record_ref, must_wait_for_approval, parse_due, parse_rfc3339,
        parse_wake_time, reply_references, reply_subject, resolve_email_source,
    };

    /// The stream's record reference comes from the executor's own reply —
    /// both shapes in use — and a result that is not about one record yields
    /// none rather than a wrong one.
    #[test]
    fn an_events_record_is_read_from_the_reply_that_named_it() {
        let wrapped = serde_json::json!({ "ok": true, "result": {
            "kind": "quote", "quote": { "id": "q-1", "number": "QUO-1" } } });
        assert_eq!(
            event_record_ref(&wrapped),
            Some(("quote".to_owned(), "q-1".to_owned()))
        );
        let flat = serde_json::json!({ "ok": true, "result": {
            "kind": "task", "id": "t-9", "title": "call" } });
        assert_eq!(
            event_record_ref(&flat),
            Some(("task".to_owned(), "t-9".to_owned()))
        );
        // A list read names no single record; neither does a kindless result.
        let list = serde_json::json!({ "ok": true, "result": { "open": [], "openCount": 0 } });
        assert_eq!(event_record_ref(&list), None);
        let bare = serde_json::json!({ "ok": true });
        assert_eq!(event_record_ref(&bare), None);
        // A kind whose record object carries no id is not half-referenced.
        let no_id = serde_json::json!({ "ok": true, "result": {
            "kind": "payment", "payment": { "amountCents": 100 } } });
        assert_eq!(event_record_ref(&no_id), None);
    }

    /// The boundary rule, over **every** tool that exists rather than a sample:
    /// a write is refused from inside a turn and allowed with the asker's own
    /// approval; a read runs either way. The registry is the only thing
    /// consulted, so a tool added tomorrow is covered by this the moment it is
    /// declared.
    #[test]
    fn a_write_is_refused_from_inside_a_turn_and_a_read_never_is() {
        for entry in alo_ai::all_tools() {
            assert_eq!(
                must_wait_for_approval(entry, Approval::InTurn),
                !entry.is_read(),
                "{} is wrong at the boundary with nobody's approval",
                entry.name
            );
            assert!(
                !must_wait_for_approval(entry, Approval::Asker),
                "{} was refused despite the asker approving it",
                entry.name
            );
        }
    }

    /// **The boundary test A1.2 exists for**, over the whole registry rather
    /// than a sample: an agent may use its own product's tools and no others,
    /// and approving a proposal does not change that.
    ///
    /// `offers` is what `execute_tool` asks, before anything is dispatched and
    /// before the approval question is even reached — so this is the condition
    /// the route runs, checked directly, with no database and no model. What
    /// this pins is that the refusal is a property of the **pair** (product,
    /// tool): the asker's approval widens who may run a tool, never which
    /// product's tools an agent has.
    #[test]
    fn an_agent_may_use_only_its_own_products_tools() {
        for product in alo_store::ALL_AGENT_PRODUCTS {
            let mine = alo_ai::tools_for(product);
            for entry in alo_ai::all_tools() {
                let ours = mine.iter().any(|tool| tool.name == entry.name);
                assert_eq!(
                    alo_ai::offers(product, entry.name),
                    ours,
                    "{product} and {}",
                    entry.name
                );
                // An approval says who asked, not what an agent is. A tool of
                // another product's stays refused with the asker's own tap.
                if !ours {
                    for approval in [Approval::InTurn, Approval::Asker] {
                        assert!(
                            !alo_ai::offers(product, entry.name),
                            "{product} got {} under {approval:?}",
                            entry.name
                        );
                    }
                }
            }
        }
        // Named plainly, because these are the sentences the item is about.
        assert!(!alo_ai::offers(
            alo_store::AgentProduct::Inventory,
            "send_email"
        ));
        assert!(!alo_ai::offers(
            alo_store::AgentProduct::Hr,
            "create_invoice_draft"
        ));
        assert!(alo_ai::offers(
            alo_store::AgentProduct::Inventory,
            "stock_answer"
        ));
        // Ask alo is the one agent that spans them, and even it refuses a name
        // no product declares.
        assert!(alo_ai::offers(
            alo_store::AgentProduct::Workspace,
            "send_email"
        ));
        assert!(!alo_ai::offers(
            alo_store::AgentProduct::Workspace,
            "delete_everything"
        ));
    }

    /// The refusal says which agent could have done it, and carries nothing
    /// about anybody's records — both the model and a room can read it.
    #[test]
    fn the_refusal_names_the_product_and_the_tool_and_nothing_else() {
        let said = super::out_of_product(alo_store::AgentProduct::Inventory, "send_email");
        assert!(said.contains("send_email"));
        assert!(said.contains("inventory"));
        assert!(said.contains("ask the agent"));
    }

    #[test]
    fn addr_specs_extracts_recipients_from_a_header_list() {
        // Bare, display-name, and mixed forms all yield the addr-spec, lowercased.
        assert_eq!(addr_specs("bob@example.com"), vec!["bob@example.com"]);
        assert_eq!(
            addr_specs("Bob <Bob@Example.com>, alice@x.eu"),
            vec!["bob@example.com", "alice@x.eu"]
        );
        // A comma inside a quoted display name still recovers the real address.
        assert_eq!(addr_specs("\"Doe, John\" <j@x.eu>"), vec!["j@x.eu"]);
        // No address present → nothing.
        assert!(addr_specs("").is_empty());
        assert!(addr_specs("not an address").is_empty());
    }

    #[test]
    fn reply_subject_prefixes_re_once() {
        assert_eq!(reply_subject("Lunch Thursday?"), "Re: Lunch Thursday?");
        // An existing Re: (any case, with or without a space) is not stacked.
        assert_eq!(reply_subject("Re: Lunch"), "Re: Lunch");
        assert_eq!(reply_subject("RE:Lunch"), "RE:Lunch");
        assert_eq!(reply_subject("  Re: Lunch  "), "Re: Lunch");
        // An empty subject still yields a valid reply subject.
        assert_eq!(reply_subject(""), "Re:");
    }

    #[test]
    fn reply_references_chains_parent_then_strips_and_dedupes() {
        let parent_refs = vec!["<a@x>".to_owned(), "<b@x>".to_owned()];
        let refs = reply_references(&parent_refs, Some("<c@x>"));
        assert_eq!(refs, vec!["a@x", "b@x", "c@x"]); // brackets stripped, parent last
        // A parent id already in the chain is not duplicated.
        let dup = reply_references(&["<c@x>".to_owned()], Some("<c@x>"));
        assert_eq!(dup, vec!["c@x"]);
        // No parent Message-ID: just the referenced ids, bare.
        assert_eq!(reply_references(&["<a@x>".to_owned()], None), vec!["a@x"]);
        assert!(reply_references(&[], None).is_empty());
    }

    #[test]
    fn resolves_an_email_source_number_to_its_message_id() {
        let sources = vec![
            ("file".to_owned(), "f1".to_owned(), "Report".to_owned()),
            ("message".to_owned(), "m2".to_owned(), "Re: Acme".to_owned()),
        ];
        // An email source number becomes the concrete message id + subject.
        let mut args = serde_json::json!({ "source": 2, "read": true });
        resolve_email_source(&mut args, &sources);
        assert_eq!(args["message_id"], "m2");
        assert_eq!(args["subject"], "Re: Acme");
        assert!(args.get("source").is_none());
        assert_eq!(args["read"], true);
        // A non-email source (a file) is left as-is — execute then rejects it.
        let mut file = serde_json::json!({ "source": 1 });
        resolve_email_source(&mut file, &sources);
        assert!(file.get("message_id").is_none());
        // No "source" at all (e.g. create_task) — untouched.
        let mut task = serde_json::json!({ "title": "x" });
        resolve_email_source(&mut task, &sources);
        assert_eq!(task, serde_json::json!({ "title": "x" }));
    }

    #[test]
    fn parses_rfc3339_to_utc() {
        let t = parse_rfc3339("2026-08-07T14:30:00Z").unwrap();
        assert_eq!(t.hour(), 14);
        assert_eq!(t.minute(), 30);
        // An offset time normalises to UTC.
        let z = parse_rfc3339("2026-08-07T16:30:00+02:00").unwrap();
        assert_eq!(z.hour(), 14);
        assert!(parse_rfc3339("2026-08-07").is_none()); // date only, not a datetime
        assert!(parse_rfc3339("not-a-time").is_none());
    }

    #[test]
    fn wake_time_must_be_a_future_rfc3339() {
        let now = parse_rfc3339("2026-08-06T12:00:00Z").unwrap();
        // A future time yields its epoch; the same instant or a past one is rejected.
        let future = parse_wake_time("2026-08-07T09:00:00Z", now).unwrap();
        assert_eq!(
            future,
            parse_rfc3339("2026-08-07T09:00:00Z")
                .unwrap()
                .unix_timestamp()
        );
        assert!(parse_wake_time("2026-08-06T12:00:00Z", now).is_none()); // now, not future
        assert!(parse_wake_time("2026-08-05T09:00:00Z", now).is_none()); // in the past
        assert!(parse_wake_time("not-a-time", now).is_none()); // malformed
    }

    #[test]
    fn parses_iso_date_to_utc_midnight() {
        let d = parse_due("2026-08-07").unwrap();
        assert_eq!(d.year(), 2026);
        assert_eq!(d.month() as u8, 8);
        assert_eq!(d.day(), 7);
        assert_eq!(d.hour(), 0);
    }

    #[test]
    fn rejects_malformed_dates() {
        assert!(parse_due("not-a-date").is_none());
        assert!(parse_due("2026-13-01").is_none()); // month 13
        assert!(parse_due("2026-02-30").is_none()); // invalid day
        assert!(parse_due("2026-08").is_none()); // too few parts
        assert!(parse_due("2026-08-07-01").is_none()); // too many parts
    }
}
