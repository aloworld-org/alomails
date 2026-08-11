//! Letter templates, over HTTP (alo HR, ADR 0035, wave B6.09b) — over
//! [`alo_store::hr_letters`].
//!
//! Two decisions this file makes, both about a door rather than about a field:
//!
//! - **The whole surface is HR's.** What this company is willing to state about
//!   one of its people, on its own letterhead, is a company decision — the same
//!   reason a checklist template is HR's and unlike a leave policy, which staff
//!   must read to ask for time off. So every route here is behind
//!   [`crate::state::Account::require_hr`].
//! - **The vocabulary travels with the list.** `GET` answers with `fields`: the
//!   placeholders this build knows, so the editor can offer them instead of
//!   asking somebody to remember them (`docs/design/ux-principles.md`,
//!   recognition over recall). The *labels* are the client's, in the reader's
//!   own catalogue; what travels is the machine name that goes inside the
//!   braces.
//!
//! Filling one in is not here: that is the agent's `draft_letter_from_template`
//! ([`crate::agent_hr`]), which is subject to the door on the *person* as well
//! as this one on the text.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::hr_letters::{LetterTemplate, MergeField, NewLetterTemplate};
use alo_store::{HrLetterTemplateId, TenantStore};

use crate::billing::{iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// One template as JSON. The `fields` it names are derived from its own text by
/// the store, so a client never has to parse `{{…}}` to know what a letter needs.
fn template_json(template: &LetterTemplate) -> Value {
    json!({
        "id": template.id.as_str(),
        "name": template.name,
        "subject": template.subject,
        "body": template.body,
        "fields": template.fields.iter().map(|f| f.as_str()).collect::<Vec<_>>(),
        "createdBy": template.created_by,
        "createdAt": iso(template.created_at),
        "updatedAt": iso(template.updated_at),
    })
}

/// The whole merge vocabulary, for the editor's placeholder picker.
fn vocabulary_json() -> Value {
    json!(
        MergeField::ALL
            .iter()
            .map(|field| field.as_str())
            .collect::<Vec<_>>()
    )
}

/// The writable shape of a template. Every field optional so a `PATCH` may state
/// one of them; a create merges onto blanks, which the store then refuses by its
/// own rules rather than by a second copy of them here.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemplateBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

impl TemplateBody {
    /// Merges the stated fields onto `base`.
    fn apply(self, base: NewLetterTemplate) -> NewLetterTemplate {
        NewLetterTemplate {
            name: self.name.unwrap_or(base.name),
            subject: self.subject.unwrap_or(base.subject),
            body: self.body.unwrap_or(base.body),
        }
    }
}

/// The stored template as writable input — the base a `PATCH` merges onto.
fn editable(template: &LetterTemplate) -> NewLetterTemplate {
    NewLetterTemplate {
        name: template.name.clone(),
        subject: template.subject.clone(),
        body: template.body.clone(),
    }
}

/// Loads one of the tenant's templates, or the `404` an id from another tenant
/// gets — a `404` rather than a `403`, which would confirm the row exists.
async fn load(hr: &TenantStore, id: &HrLetterTemplateId) -> Result<LetterTemplate, Problem> {
    hr.hr_letter_template(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such letter template"))
}

/// `GET /hr/letter-templates` → `{"templates":[…],"fields":[…]}` — **HR only**:
/// the letters this company writes, and every placeholder they may carry.
///
/// # Errors
/// `401`/`403` per the HR door.
pub async fn list_templates(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let templates = state
        .store
        .for_tenant(account.tenant.clone())
        .hr_letter_templates()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "templates": templates.iter().map(template_json).collect::<Vec<_>>(),
        "fields": vocabulary_json(),
    })))
}

/// `POST /hr/letter-templates` `{name, subject, body}` → `{"template":{…}}` —
/// **HR only**.
///
/// # Errors
/// `401`/`403` per the HR door; `409` when a template already has the name;
/// `422` on a blank field or a placeholder outside the vocabulary — the refusal
/// names the whole vocabulary, because an editor nobody can guess at is an
/// editor nobody uses.
pub async fn create_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: TemplateBody = parse_body(&body)?;
    let input = req.apply(NewLetterTemplate::default());
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = hr
        .create_hr_letter_template(&input, &account.user)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "template": template_json(&load(&hr, &id).await?) }),
    ))
}

/// `GET /hr/letter-templates/{id}` → `{"template":{…}}` — **HR only**.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the id is not this tenant's.
pub async fn get_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let template = load(&hr, &HrLetterTemplateId::new(id)).await?;
    Ok(Json(json!({ "template": template_json(&template) })))
}

/// `PATCH /hr/letter-templates/{id}` `{name?, subject?, body?}` →
/// `{"template":{…}}` — **HR only**.
///
/// Letters already drafted are untouched: a draft is a message in somebody's
/// mailbox, a copy that owes nothing to this row.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the template is not this tenant's;
/// `409` on the name; `422` as for create.
pub async fn update_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: TemplateBody = parse_body(&body)?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrLetterTemplateId::new(id);
    let stored = load(&hr, &id).await?;
    let input = req.apply(editable(&stored));
    hr.update_hr_letter_template(&id, &input)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "template": template_json(&load(&hr, &id).await?) }),
    ))
}

/// `DELETE /hr/letter-templates/{id}` → `{"deleted":true}` — **HR only**.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the template is not this tenant's or
/// is already gone.
pub async fn delete_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    state
        .store
        .for_tenant(account.tenant.clone())
        .delete_hr_letter_template(&HrLetterTemplateId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "deleted": true })))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn body(value: Value) -> TemplateBody {
        serde_json::from_value(value).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn stored() -> NewLetterTemplate {
        NewLetterTemplate {
            name: "Werkgeversverklaring".to_owned(),
            subject: "Verklaring voor {{employee.name}}".to_owned(),
            body: "In dienst sinds {{employee.started_on}}.".to_owned(),
        }
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        let merged = body(json!({})).apply(stored());
        assert_eq!(merged.name, "Werkgeversverklaring");
        assert_eq!(merged.subject, "Verklaring voor {{employee.name}}");
        assert_eq!(merged.body, "In dienst sinds {{employee.started_on}}.");
    }

    #[test]
    fn one_stated_field_replaces_only_itself() {
        let merged = body(json!({ "body": "Kortere brief." })).apply(stored());
        assert_eq!(merged.body, "Kortere brief.");
        assert_eq!(merged.name, "Werkgeversverklaring");
        // An explicitly empty body reaches the store, which is what refuses it —
        // the rule lives in one place.
        let emptied = body(json!({ "body": "" })).apply(stored());
        assert!(emptied.body.is_empty());
    }

    #[test]
    fn a_create_merges_onto_blanks_so_the_store_owns_every_rule() {
        let created = body(json!({ "name": "Reference" })).apply(NewLetterTemplate::default());
        assert_eq!(created.name, "Reference");
        assert!(created.subject.is_empty());
        assert!(created.body.is_empty());
    }

    #[test]
    fn the_vocabulary_travels_with_the_list_and_names_nothing_about_pay() {
        let fields = vocabulary_json();
        let listed = fields.as_array().expect("an array");
        assert_eq!(listed.len(), MergeField::ALL.len());
        assert!(listed.iter().any(|f| f == "employee.name"));
        assert!(listed.iter().any(|f| f == "company.name"));
        for forbidden in ["salary", "pay", "iban", "national", "birth"] {
            assert!(
                !fields.to_string().contains(forbidden),
                "the editor offers {forbidden}"
            );
        }
    }
}
