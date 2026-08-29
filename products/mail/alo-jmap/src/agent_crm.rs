//! Executing the **CRM** write verbs of an approved agent proposal (ADR 0034,
//! ADR 0035 wave B2.10; since AA.1 the acting half of the writes
//! [`alo_ai::crm_intents`] describes, reached through
//! [`crate::crm_intents::dispatch`]).
//!
//! Called only from [`crate::agent::agent_execute`], which is the single acting
//! path: the user saw the proposal and approved it. Everything here therefore
//! runs through the caller's own tenant-scoped store handle — an agent can no
//! more reach another tenant's deal than the browser that asked it can.
//!
//! Four rules shape this module, and they are why it is not thin glue:
//!
//! - **A deal is found by its title.** An invoice has a number a person can
//!   quote; an opportunity has only what somebody called it. The title the user
//!   said is resolved against this tenant's own deals by the shared rule
//!   ([`crate::agent_args`]) — exact first, then a unique containment — and two
//!   matches is a refusal that lists them, never a guess.
//! - **The board is resolved, never invented.** A tenant with one board needs
//!   nobody to name it; a tenant with three is asked which. A tenant with
//!   *none* is told to open CRM, because seeding the first board is the list
//!   route's first-use rule ([`crate::crm_pipelines::list_pipelines`]) and it
//!   names the columns in the caller's language — a board raised through this
//!   door would be named in a language nobody chose.
//! - **The conversation comes from the sources, and only ever the user's own.**
//!   `create_deal` may carry the numbered source of an email; the message is
//!   read through the caller's own mailbox door, and the deal is linked to
//!   *that* conversation. A thread the user has no message in is unreachable
//!   here exactly as it is everywhere else.
//! - **Nothing here sends mail, and nothing here deletes a record.**
//!   `draft_followup` writes into the user's Drafts and stops; there is no
//!   delete tool at all, because a deal deleted by a misread sentence leaves no
//!   trace to argue with.
//!
//! Every store function called here is the same one the `/crm/*` routes call.
//! There is no agent-only write path, so there is no second place for the rules
//! of a deal to drift.

use axum::Json;
use serde_json::{Value, json};

use alo_store::crm_deals::{Deal, DealFilter, NewDeal, StageMove};
use alo_store::crm_pipelines::Pipeline;
use alo_store::crm_stages::Stage;
use alo_store::crm_thread_match::normalize_address;
use alo_store::{CrmDealId, CrmPipelineId, CrmStageId, MessageId, ThreadId};

use crate::agent_args::{integer, pick, string_arg, unprocessable};
use crate::billing::{map_store_err, parse_iso_date};
use crate::crm_deals::deal_json;
use crate::drafts;
use crate::error::Problem;
use crate::mime::{Addr, Outgoing};
use crate::state::{Account, AppState};

/// `create_deal` — raise an opportunity on the tenant's board, optionally
/// linking the conversation it came out of.
///
/// The order matters and is the same one the `POST /crm/deals` route uses for
/// the same reason: everything is resolved and validated **before** anything is
/// written, so a proposal naming a board that does not exist leaves no
/// half-made card behind. The email source, when there is one, is resolved
/// first of all — a deal raised from a conversation that turns out to be
/// unreadable would be a deal nobody asked for.
///
/// # Errors
/// `422` when the title is missing, when the board or column cannot be resolved
/// to exactly one record, when an amount is not a whole number of cents, when
/// `expectedClose` is not a plain `YYYY-MM-DD` day, or when the referenced
/// email is not one of the caller's own; the store's own `404`/`422` otherwise.
pub async fn execute_create_deal(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Result<Json<Value>, Problem> {
    let title = string_arg(args, "title").ok_or_else(|| unprocessable("a deal needs a title"))?;
    let pipeline = resolve_pipeline(account, string_arg(args, "pipeline").as_deref()).await?;
    let stage = resolve_stage(account, &pipeline, string_arg(args, "stage").as_deref()).await?;
    // The conversation, before the card: see the note above.
    let origin = source_thread(account, args).await?;
    // The caller's own address is worth a lookup only when there is a sender to
    // compare it against and nobody has stated one.
    let stated_email = string_arg(args, "contactEmail");
    let own = match (&stated_email, &origin) {
        (None, Some(_)) => own_address(account, state).await,
        _ => None,
    };

    let input = NewDeal {
        title,
        company_name: string_arg(args, "company").unwrap_or_default(),
        contact_name: string_arg(args, "contactName").unwrap_or_default(),
        contact_email: stated_email.unwrap_or_else(|| inherited_email(origin.as_ref(), &own)),
        value_cents: integer(args.get("valueCents"), "valueCents")
            .map_err(unprocessable)?
            .unwrap_or_default(),
        currency: string_arg(args, "currency")
            .unwrap_or_else(|| alo_store::billing_field::DEFAULT_CURRENCY.to_owned()),
        expected_close: expected_close(args)?,
        source: string_arg(args, "origin").unwrap_or_default(),
        ..NewDeal::default()
    };
    let id = account
        .acc
        .create_crm_deal(&pipeline.id, &stage.id, &input)
        .await
        .map_err(map_store_err)?;

    // The link is attempted after the card exists, because it needs the card's
    // id. A failure here answers `linkedThread: null` rather than failing the
    // whole tool: the deal *was* raised, and telling a user otherwise about a
    // record they can see would be the worse answer.
    let linked = match &origin {
        Some(origin) => account
            .acc
            .link_crm_deal_thread(&id, &origin.thread)
            .await
            .unwrap_or(false)
            .then(|| origin.thread.as_str().to_owned()),
        None => None,
    };
    // ADR 0058 §4 (A4.5): a deal raised out of an email carries that email as
    // its provenance, set here where the record is created — the specific
    // source beats the generic room stamp the execution funnel would leave.
    // The label is the subject the propose path resolved beside the id, the
    // words a person would cite the conversation by. Best-effort: the deal
    // was raised, and a pointer must not unraise it.
    if let Some(message) = string_arg(args, "message_id")
        && let Err(err) = account
            .acc
            .set_record_origin(
                "deal",
                id.as_str(),
                "email",
                &message,
                string_arg(args, "subject").as_deref(),
            )
            .await
    {
        tracing::warn!(error = %err, "email provenance not recorded on the deal");
    }
    let deal = load(account, &id).await?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "deal",
            "id": id.as_str(),
            "title": deal.title,
            "pipeline": pipeline.name,
            "stage": stage.name,
            "valueCents": deal.value_cents,
            "currency": deal.currency,
            "state": deal.state().as_str(),
            "linkedThread": linked,
            "deal": deal_json(&deal),
        }
    })))
}

/// `move_deal_stage` — move the named deal into another column of **its own**
/// board, which is also how a deal is won or lost.
///
/// The column is resolved among that board's own active columns, so a name that
/// belongs to another team's funnel is "no stage of yours is called …" rather
/// than the store's flatter refusal. Everything a move *means* — the history
/// row, the closing snapshot, the reason a losing column demands and every
/// other column refuses — is [`alo_store::AccountStore::move_crm_deal`]'s, in
/// one transaction, exactly as the `POST /crm/deals/{id}/stage` route has it.
///
/// # Errors
/// `422` when the deal or the column cannot be resolved to exactly one record;
/// the store's `422` when a losing column is named without a reason (or a
/// reason is given for a column that is not one).
pub async fn execute_move_deal_stage(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let deal = resolve_deal(account, args).await?;
    let wanted = string_arg(args, "stage")
        .ok_or_else(|| unprocessable("which column to move it to is required"))?;
    let stages = account
        .acc
        .crm_stages(&deal.pipeline_id, false)
        .await
        .map_err(map_store_err)?;
    let target = pick(
        &wanted,
        stages.iter().map(|s| (s.name.as_str(), s)).collect(),
        "stage",
    )?;
    let from = stages
        .iter()
        .find(|s| s.id.as_str() == deal.stage_id.as_str())
        .map(|s| s.name.clone());

    let mv = StageMove {
        stage_id: CrmStageId::new(target.id.as_str()),
        position: None,
        lost_reason: string_arg(args, "reason").or_else(|| string_arg(args, "lostReason")),
    };
    account
        .acc
        .move_crm_deal(&deal.id, &mv)
        .await
        .map_err(map_store_err)?;
    let moved = load(account, &deal.id).await?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "deal",
            "id": moved.id.as_str(),
            "title": moved.title,
            "fromStage": from,
            "stage": target.name,
            "state": moved.state().as_str(),
            "lostReason": moved.lost_reason,
            "deal": deal_json(&moved),
        }
    })))
}

/// `draft_followup` — write a follow-up to the named deal's contact into the
/// caller's Drafts.
///
/// Nothing is sent: the letter lands where the user reads it, edits it and
/// sends it themselves, which is the rule every agent draft tool follows
/// (ADR 0023/0034). **The recipient is never the proposal's to state** — it is
/// the deal's own contact address, or its customer's when the card carries no
/// address of its own, so a follow-up cannot be aimed at somebody the user
/// never put on the deal. The words are the model's, as they are for
/// `draft_email`: a letter about an opportunity has no template.
///
/// # Errors
/// `422` when the deal cannot be resolved, when the letter is empty, or when
/// neither the deal nor its customer has an address to write to.
pub async fn execute_draft_followup(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Result<Json<Value>, Problem> {
    let deal = resolve_deal(account, args).await?;
    let body = string_arg(args, "body")
        .ok_or_else(|| unprocessable("the follow-up needs something to say"))?;
    let to = recipient(account, &deal).await?;
    let subject = string_arg(args, "subject").unwrap_or_else(|| deal.title.clone());
    let from = drafts::from_address(account, state).await?;

    let outgoing = Outgoing {
        from: Addr {
            name: None,
            email: from.clone(),
        },
        to: vec![Addr {
            name: addressee(&deal.contact_name, &deal.company_name),
            email: to.clone(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: subject.clone(),
        in_reply_to: Vec::new(),
        references: Vec::new(),
        body_text: body,
        body_html: None,
        attachments: Vec::new(),
        message_id_domain: crate::api::domain_of(&from),
        message_id_token: crate::api::new_message_token(),
    };
    let saved = drafts::save(account, &outgoing).await?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "draft",
            "id": saved.as_str(),
            "deal": deal.title,
            "dealId": deal.id.as_str(),
            "to": to,
            "subject": subject,
        }
    })))
}

/// The conversation a proposal came out of: the thread of a numbered email
/// source, and that message's sender.
struct Origin {
    /// The conversation to link the new deal to.
    thread: ThreadId,
    /// Who wrote the message — the address a deal with no contact of its own
    /// inherits, read as a bare addr-spec.
    from_addr: Option<String>,
}

/// Reads the email source of a proposal, if it carries one.
///
/// The propose route has already turned `{"source": n}` into a concrete
/// `message_id` ([`crate::agent::agent_execute`]'s caller), so what arrives
/// here is an id, and it is read through **this user's own** mailbox door: a
/// message of another tenant, of a colleague, or one that never existed are the
/// same refusal.
async fn source_thread(account: &Account, args: &Value) -> Result<Option<Origin>, Problem> {
    let Some(id) = string_arg(args, "message_id") else {
        return Ok(None);
    };
    let message = account
        .acc
        .message(&MessageId::new(id))
        .await
        .map_err(|_| unprocessable("the email this deal comes from was not found"))?;
    Ok(Some(Origin {
        thread: message.thread_id,
        // A stored `from_addr` is the whole header value (`"Ada" <ada@acme.test>`),
        // and what a deal carries is the bare address. It is read by the CRM's
        // **own** address reader ([`alo_store::crm_thread_match`]) — the same one
        // that later matches this deal against conversations — so an address the
        // deal inherits is one the suggestions can find it by.
        from_addr: normalize_address(&message.from_addr),
    }))
}

/// The caller's own address, when they have one — best effort, because not
/// having one is not a reason to refuse to raise a deal (it only matters as the
/// address a deal must *not* inherit, below).
async fn own_address(account: &Account, state: &AppState) -> Option<String> {
    let address = drafts::from_address(account, state).await.ok()?;
    normalize_address(&address)
}

/// The address a deal inherits from the conversation it was raised out of —
/// that message's sender, which is the whole point of raising it there, and what
/// later makes the thread suggestions (B2.05) and `draft_followup` work without
/// anybody retyping it.
///
/// The one exception is the user's **own** address: raising a deal from
/// something they sent must not put them down as the customer's contact. A
/// proposal that states an address of its own never reaches here.
fn inherited_email(origin: Option<&Origin>, own: &Option<String>) -> String {
    origin
        .and_then(|origin| origin.from_addr.clone())
        .filter(|sender| own.as_deref() != Some(sender.as_str()))
        .unwrap_or_default()
}

/// The day a proposal expects the deal to close, refusing anything that is not
/// a plain `YYYY-MM-DD` — the same rule, and the same refusal, the `/crm/deals`
/// route edge applies, because a day a caller may write as a timestamp is a day
/// that lands on the wrong side of somebody's midnight.
fn expected_close(args: &Value) -> Result<Option<time::Date>, Problem> {
    let Some(raw) = string_arg(args, "expectedClose") else {
        return Ok(None);
    };
    parse_iso_date(&raw)
        .map(Some)
        .ok_or_else(|| unprocessable("expectedClose must be a date written YYYY-MM-DD"))
}

/// The tenant's board this proposal is about: the one it names, the only one
/// there is, or a refusal that lists them.
async fn resolve_pipeline(account: &Account, wanted: Option<&str>) -> Result<Pipeline, Problem> {
    let mut boards = account
        .acc
        .crm_pipelines(false)
        .await
        .map_err(map_store_err)?;
    if boards.is_empty() {
        return Err(unprocessable(
            "you have no sales board yet — open CRM once and one is made for you",
        ));
    }
    if let Some(wanted) = wanted {
        let picked = pick(
            wanted,
            boards.iter().map(|p| (p.name.as_str(), p)).collect(),
            "pipeline",
        )?;
        return Ok(picked.clone());
    }
    if boards.len() > 1 {
        return Err(unprocessable(format!(
            "you have more than one pipeline: {} — say which",
            boards
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    Ok(boards.remove(0))
}

/// The column a new card is raised in: the one the proposal names, or the
/// board's first — where a new opportunity belongs, and the only column a
/// tenant certainly has.
async fn resolve_stage(
    account: &Account,
    pipeline: &Pipeline,
    wanted: Option<&str>,
) -> Result<Stage, Problem> {
    let mut columns = account
        .acc
        .crm_stages(&CrmPipelineId::new(pipeline.id.as_str()), false)
        .await
        .map_err(map_store_err)?;
    if columns.is_empty() {
        return Err(unprocessable(format!(
            "the board {} has no columns to raise a deal in",
            pipeline.name
        )));
    }
    match wanted {
        Some(wanted) => {
            let picked = pick(
                wanted,
                columns.iter().map(|s| (s.name.as_str(), s)).collect(),
                "stage",
            )?;
            Ok(picked.clone())
        }
        None => Ok(columns.remove(0)),
    }
}

/// The deal a proposal names, resolved by title among the tenant's own.
async fn resolve_deal(account: &Account, args: &Value) -> Result<Deal, Problem> {
    let wanted = string_arg(args, "deal")
        .or_else(|| string_arg(args, "title"))
        .ok_or_else(|| unprocessable("which deal this is about is required"))?;
    let deals = account
        .acc
        .crm_deals(&DealFilter::default())
        .await
        .map_err(map_store_err)?;
    let picked = pick(
        &wanted,
        deals.iter().map(|d| (d.title.as_str(), d)).collect(),
        "deal",
    )?;
    Ok(picked.clone())
}

/// Re-reads a deal after a write, so the answer carries the stored record
/// rather than what was asked for.
async fn load(account: &Account, id: &CrmDealId) -> Result<Deal, Problem> {
    account
        .acc
        .crm_deal(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::server_error)
}

/// Who a follow-up goes to: the deal's own contact address, or the linked
/// customer's when the card carries none — the same two addresses the thread
/// suggestions match on, in the same order of preference.
async fn recipient(account: &Account, deal: &Deal) -> Result<String, Problem> {
    let stored = deal.contact_email.trim();
    if !stored.is_empty() {
        return Ok(stored.to_owned());
    }
    let customer = match &deal.customer_id {
        Some(id) => account
            .acc
            .billing_customer(id)
            .await
            .map_err(map_store_err)?,
        None => None,
    };
    if let Some(email) = customer
        .and_then(|customer| customer.email)
        .filter(|email| !email.trim().is_empty())
    {
        return Ok(email.trim().to_owned());
    }
    Err(unprocessable(
        "this deal has no email address to write to — add one to the deal first",
    ))
}

/// The display name on the follow-up's `To:` — the person if the deal names
/// one, else the company, else nothing at all (a bare address, never an
/// invented name).
fn addressee(contact_name: &str, company_name: &str) -> Option<String> {
    for candidate in [contact_name, company_name] {
        let trimmed = candidate.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn origin(from: &str) -> Origin {
        Origin {
            thread: ThreadId::new("thr_1"),
            from_addr: normalize_address(from),
        }
    }

    #[test]
    fn a_deal_raised_from_a_conversation_inherits_its_sender() {
        assert_eq!(
            inherited_email(Some(&origin("ada@acme.test")), &None),
            "ada@acme.test"
        );
        // A stored `From` is a whole header value; the deal carries the bare
        // address out of it, lower-cased, and nothing else.
        assert_eq!(
            inherited_email(Some(&origin("\"Ada\" <Ada@Acme.test>")), &None),
            "ada@acme.test"
        );
        // A `From` that is not an address at all leaves the deal without one,
        // rather than putting a display name in an address field.
        assert_eq!(inherited_email(Some(&origin("Mailer Daemon")), &None), "");
        // …and never the user's own address: a deal raised from something they
        // sent must not record them as the customer's contact.
        let own = Some("me@alo.test".to_owned());
        assert_eq!(inherited_email(Some(&origin("Me <ME@alo.test>")), &own), "");
        // No conversation at all — an unnamed contact, not a guess.
        assert_eq!(inherited_email(None, &own), "");
        assert_eq!(inherited_email(Some(&origin("")), &own), "");
    }

    #[test]
    fn an_expected_close_is_a_plain_day_or_a_refusal() {
        assert_eq!(expected_close(&json!({})).unwrap(), None);
        assert_eq!(
            expected_close(&json!({ "expectedClose": "  " })).unwrap(),
            None
        );
        assert_eq!(
            expected_close(&json!({ "expectedClose": "2026-09-30" })).unwrap(),
            parse_iso_date("2026-09-30")
        );
        for bad in ["2026-09-30T00:00:00Z", "30/09/2026", "20260930", "soon"] {
            let problem = expected_close(&json!({ "expectedClose": bad }))
                .err()
                .unwrap_or_else(|| panic!("accepted {bad}"));
            assert_eq!(
                problem.status,
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "{bad}"
            );
        }
    }

    #[test]
    fn the_letter_is_addressed_to_a_person_a_company_or_to_nobody_invented() {
        assert_eq!(addressee("", ""), None);
        assert_eq!(addressee("  ", " "), None, "blanks are not a name");
        assert_eq!(addressee("", "Acme GmbH"), Some("Acme GmbH".to_owned()));
        assert_eq!(
            addressee("  Ada  ", "Acme GmbH"),
            Some("Ada".to_owned()),
            "the person wins over the company"
        );
    }
}
