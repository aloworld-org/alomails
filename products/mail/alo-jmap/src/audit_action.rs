//! What an audited request *was*, derived from the route it matched (ADR 0035,
//! wave B2.13). Pure: no state, no I/O, no knowledge of HTTP beyond a method
//! name — so the vocabulary the audit log speaks is a thing that can be read,
//! tested and reviewed in one place instead of being spread over fifty handlers.
//!
//! **Why derived rather than declared.** The alternative — each handler calling
//! `record_audit` with a hand-written verb — was rejected: it makes "every
//! mutating route writes exactly one entry" a promise kept fifty times by hand,
//! and a route added next year keeps it only if its author remembers. Deriving
//! the entry from the matched route makes coverage a property of the router
//! itself: a new `POST /billing/…` is audited the moment it is registered, and
//! `tests/audit_routes.rs` reads the router's own source to prove it.
//!
//! The derivation is deliberately mechanical. Given the matched *template*
//! (`/billing/invoices/{id}/payments/{payment_id}`) and the *actual* path, the
//! shape of a REST route already says everything an audit entry needs:
//!
//! | template | method | action | entity |
//! |---|---|---|---|
//! | `/billing/invoices` | POST | `billing.invoice.create` | id from the response |
//! | `/billing/invoices/{id}` | PATCH | `billing.invoice.update` | the path id |
//! | `/billing/invoices/{id}/issue` | POST | `billing.invoice.issue` | the path id |
//! | `/billing/invoices/{id}/payments` | POST | `billing.invoice.payment.create` | the path id |
//! | `/billing/invoices/{id}/payments/{pid}` | DELETE | `billing.invoice.payment.delete` | the path id |
//!
//! Note the last two: a sub-resource event is filed **against its parent
//! record**, which is what makes a record's history complete — a payment shows
//! up on the invoice it paid, not on a page of its own that nobody opens.

/// One audited act: which record, and what was done to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    /// The kind of record — `billing.invoice`, `crm.deal`.
    pub entity_type: String,
    /// The record's id, when the route names one. `None` for a create (the id
    /// exists only in the response) or for an act on a collection.
    pub entity_id: Option<String>,
    /// The dotted verb, always prefixed with `entity_type`.
    pub action: String,
}

/// The module prefixes whose mutations are audited. Everything else on this
/// service (mail, calendar, drive) has its own record of change and is out of
/// scope for the business audit trail.
///
/// `projects` joined at B3.04 (`docs/design/projects.md` § Audit): "who
/// approved my week, and when" is a question an employee is entitled to have
/// answered, and a timer somebody else stopped is the same kind of question.
///
/// `finance` joined at B4.05b (`docs/design/finance.md` § Tenancy) for the
/// sharper version of it: an expense claim is money somebody is owed, decided
/// by somebody else, and "who approved this, and when" is a question an auditor
/// asks as readily as the claimant does.
/// `inventory` joined at B5.04b (`docs/design/inventory.md` § Tenancy) with the
/// first route that writes a movement by hand. A stock adjustment is the most
/// abusable write in the business modules — it is the one that can make theft
/// look like paperwork — and "who adjusted this down by forty, and when" is
/// precisely the question this trail exists to answer.
/// `hr` joined at B6.02a (`docs/design/hr.md` § Audit), before the first
/// `/hr/*` route existed rather than after it — this module has the strongest
/// claim on the trail of any so far. "Who approved my leave, and when", "who
/// changed my pay", "who opened my record" and "who drew the payroll file" are
/// questions an employee, a works council, a data-protection officer and an
/// auditor each have standing to ask, and the answer must not be a
/// reconstruction. Listing the module first means the suite demands an audited
/// route from the moment one is registered.
const AUDITED_MODULES: [&str; 6] = ["billing", "crm", "projects", "finance", "inventory", "hr"];

/// Audited resources whose collection lives at the module root rather than at
/// `/module/collection`. A bare module route is not a resource unless it is
/// deliberately named here.
const ROOT_COLLECTIONS: [(&str, &str); 1] = [("projects", "project")];

/// `POST` routes that mutate nothing — a dry run whose whole point is to answer
/// "what *would* this do". Auditing them would file a paper trail for looking.
/// Kept as an explicit, short list: the default is that a `POST` writes.
///
/// `/finance/receipts` joined at B4.06b: it reads a file the caller already has
/// in Drive and answers with fields for them to confirm. The claim that follows
/// is an ordinary `POST /finance/expenses`, and that is the event worth a line.
/// `/finance/imports/bank/preview` joined at B4.08c for the same reason: the
/// store reads the file with a pure function that cannot write, and the import
/// that may follow is the event worth a line.
const READ_ONLY_POSTS: [&str; 3] = [
    "/crm/imports/leads/preview",
    "/finance/receipts",
    "/finance/imports/bank/preview",
];

/// Whether the route matched as `template` writes nothing despite its method —
/// a dry run whose whole point is to answer "what *would* this do".
///
/// Public because two layers need the same list and a second copy of it would
/// drift: the audit trail must not file a line for looking, and
/// [`crate::scoped_roles`]' read-only gate must not refuse a preview to a
/// reader who is allowed to look.
#[must_use]
pub fn writes_nothing(template: &str) -> bool {
    READ_ONLY_POSTS.contains(&template)
}

/// Whether `method` can change stored state at all. `GET`/`HEAD`/`OPTIONS`
/// never reach the audit log.
#[must_use]
pub fn is_mutating(method: &str) -> bool {
    matches!(method, "POST" | "PUT" | "PATCH" | "DELETE")
}

/// Derives the audit event for a request that matched `template` at `path`.
///
/// Returns `None` when the request is not an audited business mutation: a read
/// method, a route outside [`AUDITED_MODULES`], a listed dry run, or a template
/// too short to name a kind of record (`/billing` alone).
#[must_use]
pub fn event_for(method: &str, template: &str, path: &str) -> Option<AuditEvent> {
    if !is_mutating(method) || writes_nothing(template) {
        return None;
    }
    let template_segments: Vec<&str> = segments(template);
    let path_segments: Vec<&str> = segments(path);
    let module = *template_segments.first()?;
    if !AUDITED_MODULES.contains(&module) {
        return None;
    }
    // A bare module route (`POST /projects`) names no collection segment, so
    // the entity it acts on has to be declared rather than derived — see
    // `ROOT_COLLECTIONS`. `let … else` rather than an `is_none` test followed by
    // an `expect`: the compiler then knows the collection exists below, where
    // the previous shape only promised it in a comment and paid for the promise
    // with a panic the lint refuses.
    let Some(collection) = template_segments.get(1).copied() else {
        let root_entity = ROOT_COLLECTIONS
            .iter()
            .find_map(|(root, entity)| (*root == module).then_some(*entity))?;
        let entity_type = format!("{module}.{root_entity}");
        return Some(AuditEvent {
            action: format!("{entity_type}.{}", verb(method, &[])),
            entity_type,
            entity_id: None,
        });
    };

    // Which segment names the collection, and which would name the record.
    //
    // Almost every route reads `/module/collection/{id}/...`, so the collection
    // is segment 1 and the id segment 2. **`/projects/{id}` is the exception**:
    // there the record hangs directly off the module name, which is therefore
    // its own collection and shifts the id one segment earlier.
    //
    // Returning `None` for that shape — which this did — meant a mutating route
    // wrote nothing to the audit trail, silently. `server.rs` records the
    // convention beside the projects routes and shapes them
    // `/projects/clients/{id}` to respect it; the record route itself cannot be
    // reshaped the same way without breaking every client that addresses a
    // project by id, so the derivation learns the case instead.
    let (collection_name, id_at) = if is_param(collection) {
        (module, 1_usize)
    } else {
        (collection, 2_usize)
    };
    let entity_type = format!("{}.{}", module, name_of(singular(collection_name)));

    // The record's own id, when the template names one. Read from the actual
    // path at the same index — the router matched them segment for segment, so
    // the positions line up.
    let names_record = template_segments.get(id_at).copied().is_some_and(is_param);
    let entity_id = if names_record {
        path_segments.get(id_at).map(|s| (*s).to_owned())
    } else {
        None
    };

    let tail: Vec<&str> = template_segments
        .into_iter()
        .skip(if names_record { id_at + 1 } else { id_at })
        .collect();
    let action = format!("{entity_type}.{}", verb(method, &tail));
    Some(AuditEvent {
        entity_type,
        entity_id,
        action,
    })
}

/// The verb part of an action, from the method and whatever the template had
/// after the record (empty for a route addressing the record itself).
fn verb(method: &str, tail: &[&str]) -> String {
    let literals: Vec<&str> = tail.iter().copied().filter(|s| !is_param(s)).collect();
    let Some((last, leading)) = literals.split_last() else {
        // The route addresses the collection or the record itself.
        return match method {
            "POST" => "create",
            "DELETE" => "delete",
            _ => "update",
        }
        .to_owned();
    };
    // A sub-resource is "addressed as one member" either because the template
    // ends in that member's id (`…/payments/{payment_id}`) or because a POST to
    // a plural collection creates one (`…/payments`). Both name a payment, so
    // both singularise; a literal that is not a collection at all (`issue`,
    // `accept`, `sepa.xml`) is already the verb and is left alone.
    let addresses_member =
        tail.last().copied().is_some_and(is_param) || (method == "POST" && is_plural(last));
    let last_name = if addresses_member {
        singular(last)
    } else {
        last
    };
    let mut parts: Vec<String> = leading.iter().copied().map(name_of).collect();
    parts.push(name_of(last_name));
    let suffix = match method {
        "DELETE" => Some("delete"),
        "PATCH" | "PUT" => Some("update"),
        "POST" if addresses_member => Some("create"),
        _ => None,
    };
    if let Some(suffix) = suffix {
        parts.push(suffix.to_owned());
    }
    parts.join(".")
}

fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

fn is_param(segment: &str) -> bool {
    segment.starts_with('{') || segment.starts_with(':')
}

/// A path segment as an action name component: lowercase, and anything that is
/// not a letter or digit folded to `_` so the dotted action stays one token per
/// component (`credit-note` → `credit_note`, `sepa.xml` → `sepa_xml`).
fn name_of(segment: &str) -> String {
    segment
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// Whether a path segment reads as a collection of things. Deliberately narrow:
/// a trailing `s` that is not part of `ss`/`us`/`is`, which covers every
/// collection this service actually routes and leaves `status`, `address` and
/// the rest alone.
fn is_plural(segment: &str) -> bool {
    let lower = segment.to_ascii_lowercase();
    lower.len() > 2
        && lower.ends_with('s')
        && !lower.ends_with("ss")
        && !lower.ends_with("us")
        && !lower.ends_with("is")
}

/// The singular of a collection segment, for the same narrow set of shapes
/// [`is_plural`] recognises. Not a general English pluraliser and not trying to
/// be — it sees a fixed, reviewed list of route segments.
fn singular(segment: &str) -> &str {
    if !is_plural(segment) {
        return segment;
    }
    if segment.ends_with("ies") {
        // `-ies` → `-y` cannot be spelled as a borrow of the input, so the
        // handful of such segments this service routes are named outright. An
        // unknown one is left plural rather than mangled into a stem.
        return match segment {
            "activities" => "activity",
            "companies" => "company",
            "entries" => "entry",
            "categories" => "category",
            "deliveries" => "delivery",
            "leave-policies" => "leave-policy",
            other => other,
        };
    }
    for suffix in ["ches", "shes", "xes", "sses"] {
        if segment.ends_with(suffix) {
            return segment.strip_suffix("es").unwrap_or(segment);
        }
    }
    segment.strip_suffix('s').unwrap_or(segment)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn action(method: &str, template: &str) -> String {
        event_for(method, template, &template.replace("{id}", "rec-1"))
            .unwrap_or_else(|| panic!("{method} {template} produced no audit event"))
            .action
    }

    #[test]
    fn a_record_route_names_the_record_and_the_verb() {
        assert_eq!(
            action("POST", "/billing/invoices"),
            "billing.invoice.create"
        );
        assert_eq!(
            action("PATCH", "/billing/invoices/{id}"),
            "billing.invoice.update"
        );
        assert_eq!(
            action("DELETE", "/billing/invoices/{id}"),
            "billing.invoice.delete"
        );
        assert_eq!(
            action("POST", "/billing/invoices/{id}/issue"),
            "billing.invoice.issue"
        );
        assert_eq!(
            action("POST", "/billing/invoices/{id}/credit-note"),
            "billing.invoice.credit_note"
        );
        assert_eq!(action("POST", "/crm/deals/{id}/stage"), "crm.deal.stage");
        assert_eq!(
            action("POST", "/finance/expenses/{id}/approve"),
            "finance.expense.approve"
        );
        assert_eq!(
            action("POST", "/billing/bills/sepa.xml"),
            "billing.bill.sepa_xml"
        );
    }

    #[test]
    fn a_sub_resource_is_filed_against_its_parent_record() {
        let created = event_for(
            "POST",
            "/billing/invoices/{id}/payments",
            "/billing/invoices/inv-9/payments",
        )
        .expect("event");
        assert_eq!(created.entity_type, "billing.invoice");
        assert_eq!(created.entity_id.as_deref(), Some("inv-9"));
        assert_eq!(created.action, "billing.invoice.payment.create");

        let removed = event_for(
            "DELETE",
            "/billing/invoices/{id}/payments/{payment_id}",
            "/billing/invoices/inv-9/payments/pay-3",
        )
        .expect("event");
        assert_eq!(removed.entity_id.as_deref(), Some("inv-9"));
        assert_eq!(removed.action, "billing.invoice.payment.delete");

        assert_eq!(
            action("POST", "/crm/deals/{id}/activities"),
            "crm.deal.activity.create"
        );
        assert_eq!(
            action("POST", "/crm/deals/{id}/next-steps"),
            "crm.deal.next_step.create"
        );
        assert_eq!(
            action("DELETE", "/crm/deals/{id}/threads/{threadId}"),
            "crm.deal.thread.delete"
        );
        assert_eq!(
            action("POST", "/crm/pipelines/{id}/stages"),
            "crm.pipeline.stage.create"
        );
    }

    #[test]
    fn the_record_id_comes_from_the_path_not_the_template() {
        let event =
            event_for("POST", "/crm/deals/{id}/stage", "/crm/deals/deal-77/stage").expect("event");
        assert_eq!(event.entity_id.as_deref(), Some("deal-77"));
        // A create has no id in the path — the caller reads it from the response.
        assert!(
            event_for("POST", "/crm/deals", "/crm/deals")
                .expect("event")
                .entity_id
                .is_none()
        );
    }

    #[test]
    fn reads_dry_runs_and_other_modules_are_not_audited() {
        assert!(event_for("GET", "/billing/invoices/{id}", "/billing/invoices/i").is_none());
        assert!(event_for("HEAD", "/crm/deals", "/crm/deals").is_none());
        assert!(
            event_for(
                "POST",
                "/crm/imports/leads/preview",
                "/crm/imports/leads/preview"
            )
            .is_none()
        );
        assert!(event_for("POST", "/tasks", "/tasks").is_none());
        assert!(event_for("POST", "/calendar/events", "/calendar/events").is_none());
        assert!(event_for("POST", "/billing", "/billing").is_none());
    }

    #[test]
    fn audits_the_projects_collection_at_its_module_root() {
        let created = event_for("POST", "/projects", "/projects").expect("projects create");
        assert_eq!(created.entity_type, "projects.project");
        assert_eq!(created.entity_id, None);
        assert_eq!(created.action, "projects.project.create");
    }

    #[test]
    fn singularisation_only_touches_collections() {
        assert_eq!(singular("invoices"), "invoice");
        assert_eq!(singular("activities"), "activity");
        assert_eq!(singular("deliveries"), "delivery");
        assert_eq!(singular("next-steps"), "next-step");
        assert_eq!(singular("issue"), "issue");
        assert_eq!(singular("status"), "status");
        assert_eq!(singular("fx"), "fx");
        assert_eq!(singular("sepa.xml"), "sepa.xml");
    }
}
