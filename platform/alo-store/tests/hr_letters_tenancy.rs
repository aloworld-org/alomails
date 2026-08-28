//! Tenancy and lifecycle proofs for letter templates (alo HR, B6.09b — Law 1:
//! isolation is tested, not assumed).
//!
//! A letter template is the text a company is willing to put its name to about
//! one of its people, and the HR agent can write nothing else. Four things are
//! proven here:
//!
//! - **wrong tenant** — tenant A's template cannot be read, listed, edited or
//!   deleted from tenant B, every denial is the clean `NotFound`, and B's
//!   attempts leave A's row exactly as it was;
//! - **the lifecycle** — create, read every field back, the name rule, edit,
//!   delete, and delete-twice as a clean denial;
//! - **the vocabulary is enforced at the door** — a template naming a field this
//!   build does not know never reaches the table, whether in the subject or the
//!   body, so stored text is always mergeable;
//! - **the merge is the directory** — a letter filled in from a real person and
//!   the tenant's own letterhead states the facts the directory shows and
//!   refuses the ones nobody recorded.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::hr_letters::LETTER_BODY_MAX_CHARS;
use alo_store::{
    AccountStore, HrEmployeeId, HrLetterTemplateId, LetterFacts, MergeField, NewBillingSettings,
    NewEmployee, NewEmployment, NewLetterTemplate, Store, StoreError, TenantStore, UserId,
    render_letter,
};
use time::{Date, Month};

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

fn conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got {other:?}"),
    }
}

fn invalid<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(message)) => message,
        other => panic!("expected Validation, got {other:?}"),
    }
}

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).expect("a real date")
}

/// The letter a Dutch employee asks for when they rent a flat: it states who
/// they are, what they do and since when — and nothing about what they earn.
fn confirmation() -> NewLetterTemplate {
    NewLetterTemplate {
        name: "Werkgeversverklaring".to_owned(),
        subject: "Verklaring voor {{employee.name}}".to_owned(),
        body: "{{company.name}}, {{company.address}} ({{company.country}})\n\
               {{letter.date}}\n\n\
               Hierbij verklaren wij dat {{employee.name}} sinds \
               {{employee.started_on}} bij ons in dienst is als \
               {{employee.job_title}} ({{employee.team}}).\n"
            .to_owned(),
    }
}

/// A tenant with an HR user and one employee on real terms.
struct Company {
    ts: TenantStore,
    /// The same HR person's account door — the letterhead a letter is written
    /// on lives with the tenant's billing identity, not with its people.
    acc: AccountStore,
    hr_user: UserId,
    employee: HrEmployeeId,
}

async fn company(store: &Store, tag: &str) -> Company {
    let tenant = store
        .create_tenant(&format!("hr-letters-{tag}"))
        .await
        .unwrap();
    let ts = store.for_tenant(tenant.clone());
    let hr_user = ts
        .create_user(&format!("{tag}-hr@people.test"))
        .await
        .unwrap();
    let employee = ts
        .create_hr_employee(
            &NewEmployee {
                given_name: "Adelheid".to_owned(),
                preferred_name: "Ada".to_owned(),
                family_name: "Byron".to_owned(),
                work_email: Some(format!("{tag}-ada@people.test")),
                ..Default::default()
            },
            &hr_user,
        )
        .await
        .unwrap();
    ts.append_hr_employment(
        &employee,
        &NewEmployment {
            job_title: "Systeembeheerder".to_owned(),
            team: "Techniek".to_owned(),
            started_on: day(2024, Month::March, 4),
            ..Default::default()
        },
        &hr_user,
    )
    .await
    .unwrap();
    Company {
        acc: store.for_account(tenant, hr_user.clone()),
        ts,
        hr_user,
        employee,
    }
}

/// The letterhead a letter is written on: the tenant's own billing identity,
/// which is where every other document in the suite takes its name from.
fn letterhead() -> NewBillingSettings {
    NewBillingSettings {
        legal_name: "Voorbeeld BV".to_owned(),
        address_line1: "Kade 1".to_owned(),
        postal_code: "1011 AB".to_owned(),
        city: "Amsterdam".to_owned(),
        country: "NL".to_owned(),
        ..Default::default()
    }
}

/// **Wrong tenant.** Tenant A's template is unreachable from tenant B by every
/// verb the module has, and nothing B attempts changes A's row.
#[tokio::test]
async fn a_letter_template_is_unreachable_from_another_tenant() {
    let store = common::test_store().await;
    let a = company(&store, "iso-a").await;
    let b = company(&store, "iso-b").await;
    let id =
        a.ts.create_hr_letter_template(&confirmation(), &a.hr_user)
            .await
            .unwrap();

    assert!(b.ts.hr_letter_template(&id).await.unwrap().is_none());
    assert!(b.ts.hr_letter_templates().await.unwrap().is_empty());
    assert_not_found(
        b.ts.update_hr_letter_template(
            &id,
            &NewLetterTemplate {
                body: "Rewritten by a stranger.".to_owned(),
                ..confirmation()
            },
        )
        .await,
    );
    assert_not_found(b.ts.delete_hr_letter_template(&id).await);

    let stored = a.ts.hr_letter_template(&id).await.unwrap().unwrap();
    assert_eq!(stored.body, confirmation().body.trim());
    assert_eq!(a.ts.hr_letter_templates().await.unwrap().len(), 1);

    // B may write a template of its own with the same name — one company's
    // vocabulary is not another's.
    b.ts.create_hr_letter_template(&confirmation(), &b.hr_user)
        .await
        .unwrap();
    assert_eq!(b.ts.hr_letter_templates().await.unwrap().len(), 1);
}

/// The lifecycle: every field round-trips, the name binds within the tenant, the
/// edit replaces the text, and delete means delete.
#[tokio::test]
async fn a_letter_template_round_trips_and_its_edit_replaces_the_text() {
    let store = common::test_store().await;
    let c = company(&store, "life").await;
    let id =
        c.ts.create_hr_letter_template(&confirmation(), &c.hr_user)
            .await
            .unwrap();

    let stored = c.ts.hr_letter_template(&id).await.unwrap().unwrap();
    assert_eq!(stored.name, "Werkgeversverklaring");
    assert_eq!(stored.subject, "Verklaring voor {{employee.name}}");
    assert!(stored.body.contains("{{employee.job_title}}"));
    assert_eq!(stored.created_by, c.hr_user.as_str());
    // The fields are derived from the text, subject first, each named once.
    assert_eq!(
        stored.fields,
        [
            MergeField::EmployeeName,
            MergeField::CompanyName,
            MergeField::CompanyAddress,
            MergeField::CompanyCountry,
            MergeField::LetterDate,
            MergeField::EmployeeStartedOn,
            MergeField::EmployeeJobTitle,
            MergeField::EmployeeTeam,
        ]
    );

    // Two templates may not share a name: "the employment confirmation" has to
    // resolve to one letter.
    assert!(
        conflict(
            c.ts.create_hr_letter_template(&confirmation(), &c.hr_user)
                .await
        )
        .contains("name")
    );
    let reference =
        c.ts.create_hr_letter_template(
            &NewLetterTemplate {
                name: "Reference".to_owned(),
                subject: "Reference for {{employee.name}}".to_owned(),
                body: "{{employee.name}} worked at {{company.name}}.".to_owned(),
            },
            &c.hr_user,
        )
        .await
        .unwrap();
    let listed = c.ts.hr_letter_templates().await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].name, "Reference", "listed by name");

    // The edit replaces name, subject and body together.
    c.ts.update_hr_letter_template(
        &id,
        &NewLetterTemplate {
            name: "Werkgeversverklaring 2026".to_owned(),
            subject: "Verklaring — {{employee.family_name}}".to_owned(),
            body: "In dienst als {{employee.job_title}}.".to_owned(),
        },
    )
    .await
    .unwrap();
    let after = c.ts.hr_letter_template(&id).await.unwrap().unwrap();
    assert_eq!(after.name, "Werkgeversverklaring 2026");
    assert_eq!(after.subject, "Verklaring — {{employee.family_name}}");
    assert_eq!(
        after.fields,
        [MergeField::EmployeeFamilyName, MergeField::EmployeeJobTitle]
    );
    assert!(after.updated_at >= after.created_at);

    // Delete, and deleting twice is a clean denial.
    c.ts.delete_hr_letter_template(&reference).await.unwrap();
    assert!(c.ts.hr_letter_template(&reference).await.unwrap().is_none());
    assert_not_found(c.ts.delete_hr_letter_template(&reference).await);
    assert_eq!(c.ts.hr_letter_templates().await.unwrap().len(), 1);
    assert_not_found(
        c.ts.delete_hr_letter_template(&HrLetterTemplateId::new("no-such".to_owned()))
            .await,
    );
}

/// The vocabulary is enforced at the door: nothing unmergeable is ever stored,
/// and the reach for a pay field is refused in the editor with the whole
/// vocabulary in the message.
#[tokio::test]
async fn a_template_naming_a_field_we_do_not_know_never_reaches_the_table() {
    let store = common::test_store().await;
    let c = company(&store, "vocab").await;

    let refused = invalid(
        c.ts.create_hr_letter_template(
            &NewLetterTemplate {
                body: "Verdient {{employee.salary}} per maand.".to_owned(),
                ..confirmation()
            },
            &c.hr_user,
        )
        .await,
    );
    assert!(refused.contains("employee.salary"), "{refused}");
    assert!(refused.contains("{{employee.job_title}}"), "{refused}");

    // The subject is checked too, and so is an unclosed placeholder.
    assert!(
        invalid(
            c.ts.create_hr_letter_template(
                &NewLetterTemplate {
                    subject: "For {{employee.iban}}".to_owned(),
                    ..confirmation()
                },
                &c.hr_user,
            )
            .await
        )
        .contains("knows no merge field")
    );
    assert!(
        invalid(
            c.ts.create_hr_letter_template(
                &NewLetterTemplate {
                    body: "Dear {{employee.name".to_owned(),
                    ..confirmation()
                },
                &c.hr_user,
            )
            .await
        )
        .contains("never closed")
    );
    assert!(
        invalid(
            c.ts.create_hr_letter_template(
                &NewLetterTemplate {
                    body: "x".repeat(LETTER_BODY_MAX_CHARS + 1),
                    ..confirmation()
                },
                &c.hr_user,
            )
            .await
        )
        .contains("at most")
    );
    assert!(c.ts.hr_letter_templates().await.unwrap().is_empty());

    // The same refusals guard the edit, and leave the stored text alone.
    let id =
        c.ts.create_hr_letter_template(&confirmation(), &c.hr_user)
            .await
            .unwrap();
    assert!(
        invalid(
            c.ts.update_hr_letter_template(
                &id,
                &NewLetterTemplate {
                    body: "Betaalt {{employee.pay}}".to_owned(),
                    ..confirmation()
                },
            )
            .await
        )
        .contains("knows no merge field")
    );
    assert_eq!(
        c.ts.hr_letter_template(&id).await.unwrap().unwrap().body,
        confirmation().body.trim(),
        "a refused edit leaves the stored letter exactly as it was"
    );
}

/// The merge is the directory: a stored template filled in from a real person
/// and the tenant's own letterhead states what the directory shows, and refuses
/// what nobody recorded.
#[tokio::test]
async fn a_stored_template_merges_the_directory_and_the_letterhead() {
    let store = common::test_store().await;
    let c = company(&store, "merge").await;
    let id =
        c.ts.create_hr_letter_template(&confirmation(), &c.hr_user)
            .await
            .unwrap();
    let template = c.ts.hr_letter_template(&id).await.unwrap().unwrap();

    let person =
        c.ts.hr_directory(false)
            .await
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == c.employee)
            .expect("the employee is in the directory");
    let today = day(2026, Month::August, 11);

    // A tenant that has not stated who it is cannot write a letter in its own
    // name — the refusal is the same one a missing job title earns.
    let unstated = c.acc.billing_settings().await.unwrap();
    assert!(
        invalid(render_letter(
            &template,
            &LetterFacts::of(&person, &unstated, today)
        ))
        .contains("company.name")
    );

    let company_settings = c.acc.save_billing_settings(&letterhead()).await.unwrap();

    let letter = render_letter(
        &template,
        &LetterFacts::of(&person, &company_settings, today),
    )
    .expect("every fact this letter names is on record");
    assert_eq!(letter.subject, "Verklaring voor Ada Byron");
    assert!(letter.body.contains("sinds 2024-03-04"), "{}", letter.body);
    assert!(
        letter.body.contains("als Systeembeheerder (Techniek)"),
        "{}",
        letter.body
    );
    assert!(letter.body.contains("2026-08-11"), "{}", letter.body);
    assert!(
        !letter.body.contains("{{"),
        "nothing is left unmerged: {}",
        letter.body
    );

    // Somebody with no employment on record has no job title, and the letter is
    // refused rather than signed with a gap in it.
    let newcomer =
        c.ts.create_hr_employee(
            &NewEmployee {
                given_name: "Joris".to_owned(),
                family_name: "Claes".to_owned(),
                ..Default::default()
            },
            &c.hr_user,
        )
        .await
        .unwrap();
    let unemployed =
        c.ts.hr_directory(false)
            .await
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == newcomer)
            .expect("in the directory");
    let refused = invalid(render_letter(
        &template,
        &LetterFacts::of(&unemployed, &company_settings, today),
    ));
    assert!(refused.contains("employee.started_on"), "{refused}");
    assert!(refused.contains("Joris Claes"), "{refused}");
}
