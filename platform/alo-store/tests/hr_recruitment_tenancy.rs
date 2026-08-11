//! Tenancy, area and lifecycle proofs for hiring (alo HR, B6.06a — Law 1:
//! isolation is tested, not assumed).
//!
//! An applicant is the most sensitive record this suite holds about somebody
//! who does not work here and never agreed to our terms: a name, an address, a
//! CV and what the people who met them wrote down. Six things are proven:
//!
//! - **wrong tenant** — tenant A's opening and everybody in its pipeline are
//!   unreachable from tenant B: not read, not listed, not edited, not
//!   published, not closed, not moved, not annotated and not erased. Every
//!   denial is the clean `NotFound`, and nothing B attempts leaves a row or a
//!   file behind in either tenant;
//! - **wrong area** — a CV must be a live node in *this* tenant's HR area, so a
//!   file in somebody's personal drive can never be attached to a candidate and
//!   read by whoever owns that drive;
//! - **the opening's life** — draft → open → closed, both transitions once
//!   each, a closed opening frozen against edits and against new applications;
//! - **the stage is a person's act** — an edit never writes one, a move writes
//!   every word in the vocabulary and nothing outside it, and moving backwards
//!   out of an outcome is ordinary;
//! - **notes** — written with their author on them, newest first, and gone with
//!   the candidate;
//! - **the retention deadline** — defaulted six months on, expired when the day
//!   has passed, and the erasure that acts on it takes the notes with it.
//!
//! What is deliberately *not* here, and never will be: a test of scoring,
//! ranking or shortlisting. There is no such function to test
//! (`docs/design/hr.md` § The EU AI Act posture).
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::hr_applicants::{APPLICANT_RETENTION_MONTHS, default_retain_until};
use alo_store::hr_openings::{NewOpening, OpeningStatus};
use alo_store::{
    AccountStore, ApplicantStage, ContractKind, DriveLocation, DriveNodeId, HrApplicantId,
    HrOpeningId, NewApplicant, NewDriveFile, Store, StoreError, TenantRole, TenantStore, UserId,
};
use time::{Date, Duration, OffsetDateTime};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
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

fn role(title: &str) -> NewOpening {
    NewOpening {
        title: title.to_owned(),
        team: "Platform".to_owned(),
        location: "Rotterdam".to_owned(),
        employment_kind: ContractKind::Permanent,
    }
}

fn candidate(name: &str) -> NewApplicant {
    NewApplicant {
        name: name.to_owned(),
        email: Some(format!(
            "{}@example.test",
            name.to_lowercase().replace(' ', ".")
        )),
        phone: "+31 6 1234 5678".to_owned(),
        source: "referral".to_owned(),
        ..Default::default()
    }
}

fn cv_file(name: &str) -> NewDriveFile {
    NewDriveFile {
        name: format!("{name}-cv.pdf"),
        blob_id: format!("blob-{name}"),
        size: 4_096,
        content_type: Some("application/pdf".to_owned()),
        ..Default::default()
    }
}

fn today() -> Date {
    OffsetDateTime::now_utc().date()
}

/// A tenant hiring for one published role, with an HR user who acts.
struct Company {
    ts: TenantStore,
    hr_acc: AccountStore,
    hr_user: UserId,
    opening: HrOpeningId,
}

async fn company(store: &Store, tag: &str) -> Company {
    let tenant = store
        .create_tenant(&format!("hr-hiring-{tag}"))
        .await
        .unwrap();
    let ts = store.for_tenant(tenant.clone());
    let hr_user = ts
        .create_user(&format!("{tag}-hr@people.test"))
        .await
        .unwrap();
    ts.grant_role(&hr_user, TenantRole::Hr, &hr_user)
        .await
        .unwrap();
    let opening = ts
        .create_hr_opening(&role("Backend engineer"), &hr_user)
        .await
        .unwrap();
    ts.publish_hr_opening(&opening).await.unwrap();
    Company {
        ts,
        hr_acc: store.for_account(tenant, hr_user.clone()),
        hr_user,
        opening,
    }
}

/// Tenant A's hiring is invisible, unreadable and unwritable from tenant B —
/// through every function this module has.
#[tokio::test]
async fn another_tenants_hiring_is_out_of_reach() {
    let store = common::test_store().await;
    let a = company(&store, "a").await;
    let b = company(&store, "b").await;

    let theirs =
        a.ts.record_hr_applicant(&a.opening, &candidate("Amara Diallo"))
            .await
            .unwrap();
    a.ts.add_hr_applicant_note(&theirs, &a.hr_user, "Strong on the systems round.")
        .await
        .unwrap();

    // The opening: unreadable, unlistable, unwritable, and neither transition
    // may be driven from the other tenant.
    assert!(
        b.ts.hr_opening(&a.opening).await.unwrap().is_none(),
        "another tenant's opening does not exist through this door"
    );
    let listed = b.ts.hr_openings(true).await.unwrap();
    assert_eq!(listed.len(), 1, "tenant B lists only their own opening");
    assert_eq!(listed[0].id.as_str(), b.opening.as_str());
    assert_not_found(b.ts.update_hr_opening(&a.opening, &role("Rewritten")).await);
    assert_not_found(b.ts.publish_hr_opening(&a.opening).await);
    assert_not_found(b.ts.close_hr_opening(&a.opening).await);

    // The pipeline: neither readable nor writable, and B cannot file somebody
    // against A's opening.
    assert_not_found(b.ts.hr_applicants(&a.opening).await);
    assert_not_found(
        b.ts.record_hr_applicant(&a.opening, &candidate("Intruder"))
            .await,
    );
    assert!(
        b.ts.hr_applicant(&theirs).await.unwrap().is_none(),
        "another tenant's candidate does not exist through this door"
    );
    assert_not_found(
        b.ts.update_hr_applicant(&theirs, &candidate("Renamed"))
            .await,
    );
    assert_not_found(
        b.ts.move_hr_applicant(&theirs, ApplicantStage::Rejected)
            .await,
    );
    assert_not_found(
        b.ts.add_hr_applicant_note(&theirs, &b.hr_user, "not mine to write on")
            .await,
    );
    assert_not_found(b.ts.hr_applicant_notes(&theirs).await);
    assert_not_found(b.ts.delete_hr_applicant(&theirs).await);

    // Nothing B attempted changed anything A holds.
    let mine =
        a.ts.hr_applicant(&theirs)
            .await
            .unwrap()
            .expect("still there");
    assert_eq!(mine.name, "Amara Diallo");
    assert_eq!(mine.stage, ApplicantStage::Applied);
    assert_eq!(a.ts.hr_applicant_notes(&theirs).await.unwrap().len(), 1);
    assert_eq!(
        a.ts.hr_opening(&a.opening).await.unwrap().unwrap().status,
        OpeningStatus::Open
    );
}

/// A CV must be a live node in **this** tenant's HR area: a file in somebody's
/// personal drive, or in another tenant's HR area, is the same `NotFound`.
#[tokio::test]
async fn a_cv_must_live_in_this_tenants_hr_area() {
    let store = common::test_store().await;
    let a = company(&store, "cv-a").await;
    let b = company(&store, "cv-b").await;

    let personal = a
        .hr_acc
        .drive_create_file(&DriveLocation::Personal, None, &cv_file("personal"))
        .await
        .unwrap();
    let theirs = b
        .hr_acc
        .drive_create_file(&DriveLocation::Hr, None, &cv_file("theirs"))
        .await
        .unwrap();
    let ours = a
        .hr_acc
        .drive_create_file(&DriveLocation::Hr, None, &cv_file("ours"))
        .await
        .unwrap();

    for node in [
        &personal,
        &theirs,
        &DriveNodeId::new("never-issued".to_owned()),
    ] {
        assert_not_found(
            a.ts.record_hr_applicant(
                &a.opening,
                &NewApplicant {
                    cv_node_id: Some(node.clone()),
                    ..candidate("Amara Diallo")
                },
            )
            .await,
        );
    }
    assert!(
        a.ts.hr_applicants(&a.opening).await.unwrap().is_empty(),
        "a refused application leaves no row"
    );

    let applicant =
        a.ts.record_hr_applicant(
            &a.opening,
            &NewApplicant {
                cv_node_id: Some(ours.clone()),
                ..candidate("Amara Diallo")
            },
        )
        .await
        .unwrap();
    let stored = a.ts.hr_applicant(&applicant).await.unwrap().unwrap();
    assert_eq!(
        stored.cv_node_id.as_ref().map(DriveNodeId::as_str),
        Some(ours.as_str())
    );
    assert_eq!(stored.cv_file_name.as_deref(), Some("ours-cv.pdf"));
    assert_eq!(stored.cv_size, Some(4_096));
    assert!(!stored.cv_trashed, "a fresh CV is not in the trash");

    // A CV that goes to Drive's trash is still the record of what was sent —
    // the row says so rather than pretending there was never a file.
    a.hr_acc.drive_trash_node(&ours).await.unwrap();
    let after = a.ts.hr_applicant(&applicant).await.unwrap().unwrap();
    assert!(after.cv_trashed, "the row reports the file's own state");

    // And an edit cannot move the CV to a file outside the area either.
    assert_not_found(
        a.ts.update_hr_applicant(
            &applicant,
            &NewApplicant {
                cv_node_id: Some(personal.clone()),
                ..candidate("Amara Diallo")
            },
        )
        .await,
    );
}

/// Draft → open → closed, once each; a closed opening is frozen against edits
/// and against new applications, and its pipeline survives it.
#[tokio::test]
async fn an_openings_life_has_two_transitions_and_a_terminal_state() {
    let store = common::test_store().await;
    let a = company(&store, "life").await;

    let draft =
        a.ts.create_hr_opening(&role("Warehouse hand"), &a.hr_user)
            .await
            .unwrap();
    let stored = a.ts.hr_opening(&draft).await.unwrap().unwrap();
    assert_eq!(stored.status, OpeningStatus::Draft);
    assert!(stored.opened_on.is_none() && stored.closed_on.is_none());
    assert_eq!(stored.applicants, 0);

    // A referral before the advertisement goes out is ordinary.
    let early =
        a.ts.record_hr_applicant(&draft, &candidate("Early Bird"))
            .await
            .unwrap();

    a.ts.publish_hr_opening(&draft).await.unwrap();
    let published = a.ts.hr_opening(&draft).await.unwrap().unwrap();
    assert_eq!(published.status, OpeningStatus::Open);
    assert_eq!(published.opened_on, Some(today()));
    assert_eq!(published.applicants, 1, "the count comes with the record");

    let twice = conflict(a.ts.publish_hr_opening(&draft).await);
    assert!(
        twice.contains("open"),
        "the refusal names the state: {twice}"
    );

    // Editing is fine while the round is running…
    a.ts.update_hr_opening(
        &draft,
        &NewOpening {
            location: "Amsterdam".to_owned(),
            employment_kind: ContractKind::FixedTerm,
            ..role("Warehouse hand")
        },
    )
    .await
    .unwrap();
    let edited = a.ts.hr_opening(&draft).await.unwrap().unwrap();
    assert_eq!(edited.location, "Amsterdam");
    assert_eq!(edited.employment_kind, ContractKind::FixedTerm);
    assert_eq!(
        edited.opened_on,
        Some(today()),
        "an edit does not restate when the round started"
    );

    // …and refused once it is over, as is a new application to it.
    a.ts.close_hr_opening(&draft).await.unwrap();
    let closed = a.ts.hr_opening(&draft).await.unwrap().unwrap();
    assert_eq!(closed.status, OpeningStatus::Closed);
    assert_eq!(closed.closed_on, Some(today()));
    let frozen = conflict(a.ts.update_hr_opening(&draft, &role("Renamed")).await);
    assert!(frozen.contains("closed"), "the refusal says why: {frozen}");
    let refused = conflict(
        a.ts.record_hr_applicant(&draft, &candidate("Too Late"))
            .await,
    );
    assert!(
        refused.contains("closed"),
        "and so does this one: {refused}"
    );
    assert!(conflict(a.ts.close_hr_opening(&draft).await).contains("already"));
    assert!(
        conflict(a.ts.publish_hr_opening(&draft).await).contains("closed"),
        "a closed opening does not go back to being open"
    );

    // The people who applied are the record of what happened, and outlive it.
    let pipeline = a.ts.hr_applicants(&draft).await.unwrap();
    assert_eq!(pipeline.len(), 1);
    assert_eq!(pipeline[0].id.as_str(), early.as_str());

    // A live list hides a closed round; asking for it shows it.
    let live = a.ts.hr_openings(false).await.unwrap();
    assert!(
        live.iter().all(|o| o.status != OpeningStatus::Closed),
        "the live list is the ones still being run"
    );
    assert!(
        a.ts.hr_openings(true)
            .await
            .unwrap()
            .iter()
            .any(|o| o.id.as_str() == draft.as_str()),
        "and the closed round is there when asked for"
    );

    // A title is the one thing an opening cannot be without.
    let blank = invalid(a.ts.create_hr_opening(&role("   "), &a.hr_user).await);
    assert!(
        blank.contains("title"),
        "the message names the field: {blank}"
    );
}

/// The stage changes because a person moved it — never because a field was
/// edited — and every word in the vocabulary is a place somebody can be put.
#[tokio::test]
async fn a_stage_moves_only_through_the_move() {
    let store = common::test_store().await;
    let a = company(&store, "stages").await;
    let applicant =
        a.ts.record_hr_applicant(&a.opening, &candidate("Amara Diallo"))
            .await
            .unwrap();
    assert_eq!(
        a.ts.hr_applicant(&applicant).await.unwrap().unwrap().stage,
        ApplicantStage::Applied,
        "an application starts where applications start"
    );

    // An edit corrects the record and leaves the candidacy where it was.
    a.ts.update_hr_applicant(
        &applicant,
        &NewApplicant {
            phone: "+31 6 0000 0000".to_owned(),
            ..candidate("Amara Diallo")
        },
    )
    .await
    .unwrap();
    let edited = a.ts.hr_applicant(&applicant).await.unwrap().unwrap();
    assert_eq!(edited.phone, "+31 6 0000 0000");
    assert_eq!(
        edited.stage,
        ApplicantStage::Applied,
        "an edit is not a decision"
    );

    for stage in ApplicantStage::ALL {
        a.ts.move_hr_applicant(&applicant, stage).await.unwrap();
        assert_eq!(
            a.ts.hr_applicant(&applicant).await.unwrap().unwrap().stage,
            stage
        );
    }
    // Out of an outcome and back into the round: a rejection reversed is
    // ordinary, and the model does not argue with the company about it.
    a.ts.move_hr_applicant(&applicant, ApplicantStage::Interview)
        .await
        .unwrap();
    assert_eq!(
        a.ts.hr_applicant(&applicant).await.unwrap().unwrap().stage,
        ApplicantStage::Interview
    );

    let unknown = invalid(ApplicantStage::parse("shortlisted").map(|_| ()));
    for stage in ApplicantStage::ALL {
        assert!(
            unknown.contains(stage.as_str()),
            "the refusal lists {stage}"
        );
    }
}

/// A pipeline reads in board order — the stage vocabulary's own order, then the
/// order people applied in.
#[tokio::test]
async fn a_pipeline_reads_in_board_order() {
    let store = common::test_store().await;
    let a = company(&store, "board").await;

    let mut ids = Vec::new();
    for (name, stage) in [
        ("First Applied", ApplicantStage::Applied),
        ("An Offer", ApplicantStage::Offer),
        ("Second Applied", ApplicantStage::Applied),
        ("Turned Down", ApplicantStage::Rejected),
        ("In Interview", ApplicantStage::Interview),
    ] {
        let id =
            a.ts.record_hr_applicant(&a.opening, &candidate(name))
                .await
                .unwrap();
        a.ts.move_hr_applicant(&id, stage).await.unwrap();
        ids.push((name, id));
    }

    let pipeline = a.ts.hr_applicants(&a.opening).await.unwrap();
    let order: Vec<&str> = pipeline.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        order,
        vec![
            "First Applied",
            "Second Applied",
            "In Interview",
            "An Offer",
            "Turned Down",
        ],
        "stages in vocabulary order, and inside a stage the order people applied"
    );
    assert_eq!(
        a.ts.hr_opening(&a.opening)
            .await
            .unwrap()
            .unwrap()
            .applicants,
        5
    );
    assert!(ids.len() == 5);
}

/// Notes carry their author, read newest first, and are refused blank.
#[tokio::test]
async fn a_note_carries_the_person_who_wrote_it() {
    let store = common::test_store().await;
    let a = company(&store, "notes").await;
    let second_reader = a.ts.create_user("notes-second@people.test").await.unwrap();
    let applicant =
        a.ts.record_hr_applicant(&a.opening, &candidate("Amara Diallo"))
            .await
            .unwrap();

    a.ts.add_hr_applicant_note(&applicant, &a.hr_user, "  Phone screen: strong.  ")
        .await
        .unwrap();
    a.ts.add_hr_applicant_note(&applicant, &second_reader, "Systems round: hire.")
        .await
        .unwrap();

    let notes = a.ts.hr_applicant_notes(&applicant).await.unwrap();
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].body, "Systems round: hire.", "newest first");
    assert_eq!(notes[0].author.as_str(), second_reader.as_str());
    assert_eq!(notes[1].body, "Phone screen: strong.", "and it is trimmed");
    assert_eq!(notes[1].author.as_str(), a.hr_user.as_str());

    let blank = invalid(
        a.ts.add_hr_applicant_note(&applicant, &a.hr_user, "   ")
            .await,
    );
    assert!(
        blank.contains("note"),
        "the message names the field: {blank}"
    );
    assert_not_found(
        a.ts.add_hr_applicant_note(
            &HrApplicantId::new("never-issued".to_owned()),
            &a.hr_user,
            "about nobody",
        )
        .await,
    );
}

/// The retention deadline is defaulted, statable, reported when it has passed,
/// and the erasure that acts on it takes the notes with it.
#[tokio::test]
async fn the_retention_deadline_is_remembered_and_a_person_acts_on_it() {
    let store = common::test_store().await;
    let a = company(&store, "retention").await;

    let defaulted =
        a.ts.record_hr_applicant(&a.opening, &candidate("Amara Diallo"))
            .await
            .unwrap();
    let stored = a.ts.hr_applicant(&defaulted).await.unwrap().unwrap();
    assert_eq!(
        stored.retain_until,
        default_retain_until(),
        "{APPLICANT_RETENTION_MONTHS} months on unless the caller says otherwise"
    );
    assert!(
        !stored.retention_expired,
        "a fresh application is not past its date"
    );

    let expired =
        a.ts.record_hr_applicant(
            &a.opening,
            &NewApplicant {
                retain_until: Some(today() - Duration::days(1)),
                ..candidate("Older Application")
            },
        )
        .await
        .unwrap();
    let past = a.ts.hr_applicant(&expired).await.unwrap().unwrap();
    assert!(
        past.retention_expired,
        "yesterday's deadline is the screen's prompt to a person"
    );

    a.ts.add_hr_applicant_note(&expired, &a.hr_user, "Not this time.")
        .await
        .unwrap();
    a.ts.delete_hr_applicant(&expired).await.unwrap();
    assert!(
        a.ts.hr_applicant(&expired).await.unwrap().is_none(),
        "the record is gone, not archived"
    );
    assert_not_found(a.ts.hr_applicant_notes(&expired).await);
    assert_not_found(a.ts.delete_hr_applicant(&expired).await);
    assert_eq!(
        a.ts.hr_applicants(&a.opening).await.unwrap().len(),
        1,
        "and only that one"
    );
    assert_eq!(
        a.ts.hr_opening(&a.opening)
            .await
            .unwrap()
            .unwrap()
            .applicants,
        1,
        "the pipeline count follows the erasure"
    );

    // A deadline generations away is a slipped digit, not a policy.
    let refused = invalid(
        a.ts.record_hr_applicant(
            &a.opening,
            &NewApplicant {
                retain_until: today().replace_year(today().year() + 40).ok(),
                ..candidate("Far Future")
            },
        )
        .await,
    );
    assert!(refused.contains("10 years"), "the message names the rule");
}
