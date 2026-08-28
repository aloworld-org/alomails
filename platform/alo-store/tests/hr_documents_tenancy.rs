//! Tenancy, role and area proofs for the papers on a person's file (alo HR,
//! B6.02b — Law 1: isolation is tested, not assumed).
//!
//! `docs/design/hr.md` files documents behind the HR door and says the bytes
//! live "in the HR-only area". Both halves are asserted here, because either
//! one alone would be a promise the other breaks:
//!
//! - **wrong tenant** — tenant A's filing, its document list and the file
//!   itself are all unreachable from tenant B, whatever role B's caller holds
//!   in their own tenant;
//! - **wrong role** — inside one tenant, a member without
//!   [`TenantRole::Hr`] cannot read, list, write or even learn the existence of
//!   a node in the HR area: `NotFound`, not `Forbidden`;
//! - **wrong area** — a node in somebody's personal files or in a Space cannot
//!   be filed as an HR document at all, so a filing row can never claim
//!   "HR-only" over a file a colleague can open;
//! - **one file, one person** — a node already filed is a clean conflict, and a
//!   detach leaves the file where it is.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::hr_documents::HrDocumentKind;
use alo_store::{
    AccountStore, DriveLocation, DriveNodeId, HrDocumentId, HrEmployeeId, NewDriveFile,
    NewEmployee, Store, StoreError, TenantId, TenantRole, TenantStore, UserId,
};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

fn person(given: &str, family: &str) -> NewEmployee {
    NewEmployee {
        given_name: given.to_owned(),
        family_name: family.to_owned(),
        ..Default::default()
    }
}

fn contract_file(name: &str) -> NewDriveFile {
    NewDriveFile {
        name: name.to_owned(),
        blob_id: format!("blob-{name}"),
        size: 42,
        content_type: Some("application/pdf".to_owned()),
        ..Default::default()
    }
}

/// A tenant with one user who holds the HR role, and one who does not.
///
/// Returns the tenant door, the HR member's account door, the ordinary
/// member's account door, and the HR member's id.
async fn tenant_with_hr(
    store: &Store,
    tag: &str,
) -> (TenantStore, AccountStore, AccountStore, UserId) {
    let tenant: TenantId = store.create_tenant(&format!("hrdocs-{tag}")).await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let hr_user = ts
        .create_user(&format!("{tag}-hr@people.test"))
        .await
        .unwrap();
    let member = ts
        .create_user(&format!("{tag}-member@people.test"))
        .await
        .unwrap();
    ts.grant_role(&hr_user, TenantRole::Hr, &hr_user)
        .await
        .unwrap();
    (
        ts,
        store.for_account(tenant.clone(), hr_user.clone()),
        store.for_account(tenant, member),
        hr_user,
    )
}

/// Files one contract on a fresh employee, returning the tenant door, the HR
/// door, the employee, the node and the filing.
async fn tenant_with_a_filed_contract(
    store: &Store,
    tag: &str,
) -> (
    TenantStore,
    AccountStore,
    AccountStore,
    UserId,
    HrEmployeeId,
    DriveNodeId,
    HrDocumentId,
) {
    let (ts, hr_acc, member_acc, hr_user) = tenant_with_hr(store, tag).await;
    let employee = ts
        .create_hr_employee(&person("Inès", "Dupont"), &hr_user)
        .await
        .unwrap();
    let node = hr_acc
        .drive_create_file(&DriveLocation::Hr, None, &contract_file(tag))
        .await
        .unwrap();
    let filing = ts
        .file_hr_document(
            &employee,
            &node,
            HrDocumentKind::Contract,
            "contract of employment",
            &hr_user,
        )
        .await
        .unwrap();
    (ts, hr_acc, member_acc, hr_user, employee, node, filing)
}

/// **Wrong tenant.** Neither the filing nor the file behind it is reachable
/// from another tenant, and holding HR there changes nothing.
#[tokio::test]
async fn another_tenants_hr_file_is_unreachable_by_every_path() {
    let store = common::test_store().await;
    let (ts_a, _hr_a, _member_a, _user_a, employee, node, filing) =
        tenant_with_a_filed_contract(&store, "own").await;
    let (ts_b, hr_b, _member_b, user_b) = tenant_with_hr(&store, "other").await;

    // The filing: every read and every write, one denial.
    assert_not_found(ts_b.hr_documents(&employee).await);
    assert_not_found(ts_b.hr_document(&employee, &filing).await);
    assert_not_found(ts_b.detach_hr_document(&employee, &filing).await);
    assert_not_found(
        ts_b.file_hr_document(
            &employee,
            &node,
            HrDocumentKind::Letter,
            "not yours",
            &user_b,
        )
        .await,
    );

    // The file: tenant B's HR sees no such node, and their own HR area is
    // empty — the areas are per tenant, not one shared drawer.
    assert!(hr_b.drive_node(&node).await.unwrap().is_none());
    assert!(
        hr_b.drive_list(&DriveLocation::Hr, None)
            .await
            .unwrap()
            .is_empty()
    );

    // And tenant A is untouched by the attempt.
    assert_eq!(ts_a.hr_documents(&employee).await.unwrap().len(), 1);
}

/// **Wrong role.** Inside one tenant, a member without the HR role learns
/// nothing at all about the HR area: not the file, not the folder, not that
/// there is one.
#[tokio::test]
async fn a_member_without_the_hr_role_cannot_see_the_area_at_all() {
    let store = common::test_store().await;
    let (_ts, hr_acc, member_acc, _hr_user, _employee, node, _filing) =
        tenant_with_a_filed_contract(&store, "role").await;

    // Reads: `None`/`NotFound`, never `Forbidden` — on this location the
    // existence of a file is part of what is being kept.
    assert!(member_acc.drive_node(&node).await.unwrap().is_none());
    assert_not_found(member_acc.drive_list(&DriveLocation::Hr, None).await);
    assert_not_found(member_acc.drive_versions(&node).await);
    assert_not_found(member_acc.drive_writable(&node).await);

    // Writes: the same denial, including creating a file of their own there.
    assert_not_found(member_acc.drive_rename(&node, "mine now").await);
    assert_not_found(member_acc.drive_trash_node(&node).await);
    assert_not_found(
        member_acc
            .drive_create_file(&DriveLocation::Hr, None, &contract_file("sneaked"))
            .await,
    );
    assert_not_found(
        member_acc
            .drive_create_folder(&DriveLocation::Hr, None, "mine")
            .await,
    );

    // The HR member, on the same node, is answered normally — the denial is
    // about the role, not about the node being broken.
    assert!(hr_acc.drive_node(&node).await.unwrap().is_some());
    assert_eq!(
        hr_acc
            .drive_list(&DriveLocation::Hr, None)
            .await
            .unwrap()
            .len(),
        1
    );
}

/// **Wrong area.** A node in personal files or in a Space cannot be filed as an
/// HR document, however senior the caller — the filing row can never claim
/// "HR-only" over a file somebody else can open.
#[tokio::test]
async fn only_a_node_in_the_hr_area_can_be_filed() {
    let store = common::test_store().await;
    let (ts, hr_acc, _member_acc, hr_user) = tenant_with_hr(&store, "area").await;
    let employee = ts
        .create_hr_employee(&person("Bram", "Peeters"), &hr_user)
        .await
        .unwrap();

    // The same HR user's own personal files: readable by them, and not an HR
    // document.
    let personal = hr_acc
        .drive_create_file(&DriveLocation::Personal, None, &contract_file("personal"))
        .await
        .unwrap();
    assert_not_found(
        ts.file_hr_document(
            &employee,
            &personal,
            HrDocumentKind::Contract,
            "wrong place",
            &hr_user,
        )
        .await,
    );

    // A Space they manage: same answer, for the same reason.
    let space = hr_acc.create_space("Team").await.unwrap();
    let in_space = hr_acc
        .drive_create_file(
            &DriveLocation::Space(space),
            None,
            &contract_file("in-space"),
        )
        .await
        .unwrap();
    assert_not_found(
        ts.file_hr_document(
            &employee,
            &in_space,
            HrDocumentKind::Contract,
            "wrong place",
            &hr_user,
        )
        .await,
    );

    // An id that never existed: the same denial again, so the refusal is not an
    // oracle for which nodes are where.
    assert_not_found(
        ts.file_hr_document(
            &employee,
            &DriveNodeId::new("no-such-node".to_owned()),
            HrDocumentKind::Contract,
            "nothing there",
            &hr_user,
        )
        .await,
    );

    // A node in the HR area that has been trashed is not a document to file.
    let trashed = hr_acc
        .drive_create_file(&DriveLocation::Hr, None, &contract_file("trashed"))
        .await
        .unwrap();
    hr_acc.drive_trash_node(&trashed).await.unwrap();
    assert_not_found(
        ts.file_hr_document(
            &employee,
            &trashed,
            HrDocumentKind::Contract,
            "on its way out",
            &hr_user,
        )
        .await,
    );

    assert!(ts.hr_documents(&employee).await.unwrap().is_empty());
}

/// One file is on one person's file, and detaching a mis-filing leaves the file
/// itself where it is.
#[tokio::test]
async fn a_file_is_filed_once_and_detaching_keeps_it() {
    let store = common::test_store().await;
    let (ts, hr_acc, _member_acc, hr_user, employee, node, filing) =
        tenant_with_a_filed_contract(&store, "once").await;
    let colleague = ts
        .create_hr_employee(&person("Bram", "Peeters"), &hr_user)
        .await
        .unwrap();

    // The same node on a second person: a conflict that names the rule and not
    // whose file it is already on.
    let refused = ts
        .file_hr_document(
            &colleague,
            &node,
            HrDocumentKind::Contract,
            "same paper",
            &hr_user,
        )
        .await;
    match refused {
        Err(StoreError::Conflict(message)) => {
            assert!(
                message.contains("already filed"),
                "names the rule: {message}"
            );
            assert!(!message.contains("Inès"), "never names the holder");
        }
        other => panic!("expected a Conflict, got: {other:?}"),
    }

    // The read carries the file's own name and size, so a list of somebody's
    // documents is one round trip.
    let filed = ts.hr_documents(&employee).await.unwrap();
    assert_eq!(filed.len(), 1);
    assert_eq!(filed[0].kind, HrDocumentKind::Contract);
    assert_eq!(filed[0].file_name.as_deref(), Some("once"));
    assert_eq!(filed[0].size, Some(42));
    assert_eq!(filed[0].filed_by.as_str(), hr_user.as_str());

    // Detaching removes the filing and nothing else.
    ts.detach_hr_document(&employee, &filing).await.unwrap();
    assert!(ts.hr_documents(&employee).await.unwrap().is_empty());
    assert!(hr_acc.drive_node(&node).await.unwrap().is_some());
    assert!(ts.hr_document(&employee, &filing).await.unwrap().is_none());
    // Detaching twice is the clean denial, not a silent success.
    assert_not_found(ts.detach_hr_document(&employee, &filing).await);

    // And now the file can be filed against the person it actually belongs to.
    ts.file_hr_document(
        &colleague,
        &node,
        HrDocumentKind::Contract,
        "refiled",
        &hr_user,
    )
    .await
    .unwrap();
    assert_eq!(ts.hr_documents(&colleague).await.unwrap().len(), 1);
}
