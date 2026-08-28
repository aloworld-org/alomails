//! The isolation suite (mandatory, CI-gated). For **every** public read
//! and write path on the account door, an id belonging to another
//! account — whether in another tenant or another user of the *same*
//! tenant — must get a clean `NotFound`/empty: never the other account's
//! data, never an unexpected error (a "500"). The boundary is the type
//! you hold ([`AccountStore`]), enforced by construction: there is no
//! ownership guard in any call path to omit. Runs against the real
//! Postgres from compose.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, BlobId, ChannelVisibility, MailboxId, MemberRole, Message, MessageId, Page, SEEN,
    StoreError, ThreadId,
};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`/`Blob`/`Migrate`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got internal error: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// Asserts a result is the clean forbidden denial — the caller can see the
/// resource but lacks the role (ADR 0026), never data, never a 500.
fn assert_forbidden<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::Forbidden) => {}
        Err(other) => panic!("expected Forbidden, got: {other:?}"),
        Ok(value) => panic!("expected Forbidden, but got data: {value:?}"),
    }
}

/// One account plus the ids of its single delivered message, that
/// message's thread, and the blob it references — everything a probe
/// needs to address it.
struct Probe {
    acc: AccountStore,
    inbox: MailboxId,
    message: MessageId,
    thread: ThreadId,
    blob: BlobId,
}

impl Probe {
    async fn build(acc: AccountStore, tag: &str) -> Self {
        let inbox = acc.inbox().await.unwrap();
        let raw = format!(
            "From: sender@example.test\r\nSubject: hello {tag}\r\n\
             Message-ID: <{tag}@example.test>\r\n\r\nbody of {tag}\r\n"
        );
        let message = acc.ingest(&inbox, raw.as_bytes()).await.unwrap();
        let full = acc.message(&message).await.unwrap();
        Self {
            thread: full.thread_id,
            blob: full.blob_id,
            acc,
            inbox,
            message,
        }
    }
}

/// Drives every account-door path on `attacker`'s handle against
/// `victim`'s ids and asserts each is denied — the shared core of the
/// cross-tenant and cross-account proofs.
async fn assert_fully_isolated(attacker: &Probe, victim: &Probe) {
    // --- single-row reads → NotFound ---
    assert_not_found(attacker.acc.mailbox(&victim.inbox).await);
    assert_not_found(attacker.acc.message(&victim.message).await);
    assert_not_found(attacker.acc.message_bytes(&victim.message).await);
    assert_not_found(attacker.acc.mailboxes_of_message(&victim.message).await);
    assert_not_found(attacker.acc.blob(&victim.blob).await);
    assert_not_found(attacker.acc.blob_bytes(&victim.blob).await);

    // --- collection reads → EMPTY, never the victim's rows ---
    assert!(
        attacker
            .acc
            .list_mailbox(&victim.inbox, Page::default())
            .await
            .unwrap()
            .is_empty(),
        "must not list another account's mailbox contents"
    );
    assert!(
        attacker
            .acc
            .keywords(&victim.message)
            .await
            .unwrap()
            .is_empty(),
        "must not read another account's keywords"
    );
    assert!(
        attacker
            .acc
            .thread_messages(&victim.thread, Page::default())
            .await
            .unwrap()
            .is_empty(),
        "must not enumerate another account's thread"
    );

    // --- writes with a foreign id → NotFound ---
    assert_not_found(attacker.acc.set_keyword(&victim.message, SEEN, true).await);
    assert_not_found(attacker.acc.destroy_message(&victim.message).await);
    // foreign message into own mailbox, and own message into foreign mailbox:
    assert_not_found(
        attacker
            .acc
            .add_to_mailbox(&victim.message, &attacker.inbox)
            .await,
    );
    assert_not_found(
        attacker
            .acc
            .add_to_mailbox(&attacker.message, &victim.inbox)
            .await,
    );
    assert_not_found(
        attacker
            .acc
            .remove_from_mailbox(&victim.message, &attacker.inbox)
            .await,
    );
    assert_not_found(attacker.acc.rename_mailbox(&victim.inbox, "Evil").await);
    assert_not_found(attacker.acc.move_mailbox(&victim.inbox, None).await);
    assert_not_found(attacker.acc.destroy_mailbox(&victim.inbox).await);
    // a mailbox created under a foreign parent must be refused:
    assert_not_found(
        attacker
            .acc
            .create_mailbox(Some(&victim.inbox), "Evil", None)
            .await,
    );
    // ingest into a foreign mailbox must be refused:
    assert_not_found(
        attacker
            .acc
            .ingest(&victim.inbox, b"From: x\r\n\r\nx")
            .await,
    );
}

/// Confirms a probe's account still holds exactly its one message after
/// being probed, and sees only its own.
async fn assert_intact(probe: &Probe) {
    let inbox = probe.acc.mailbox(&probe.inbox).await.unwrap();
    assert_eq!(inbox.total_messages, 1, "account still has its one message");
    assert_eq!(inbox.unread_messages, 1);
    let list = probe
        .acc
        .list_mailbox(&probe.inbox, Page::default())
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, probe.message);
    assert!(probe.acc.message(&probe.message).await.is_ok());
    // Its own blob is reachable (it references it).
    assert!(probe.acc.blob(&probe.blob).await.is_ok());
}

#[tokio::test]
async fn cross_tenant_is_denied_on_every_path() {
    let store = common::test_store().await;
    // Two accounts, each the sole user of its own tenant.
    let (a_acc, _ua, _ia) = common::fresh_account(&store, "xt-a").await;
    let (b_acc, _ub, _ib) = common::fresh_account(&store, "xt-b").await;
    let a = Probe::build(a_acc, "xt-a").await;
    let b = Probe::build(b_acc, "xt-b").await;

    assert_fully_isolated(&a, &b).await;
    assert_fully_isolated(&b, &a).await;
    assert_intact(&a).await;
    assert_intact(&b).await;
}

#[tokio::test]
async fn cross_account_within_one_tenant_is_denied_on_every_path() {
    // The account door's reason to exist: two users in the SAME tenant
    // must not reach each other's rows, with no ownership guard in the
    // path — the compiler-enforced boundary.
    let store = common::test_store().await;
    let tenant = store.create_tenant("one-tenant").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let ua = ts.create_user("alice@example.test").await.unwrap();
    let ub = ts.create_user("bob@example.test").await.unwrap();
    let a = Probe::build(store.for_account(tenant.clone(), ua), "ca-a").await;
    let b = Probe::build(store.for_account(tenant, ub), "ca-b").await;

    assert_fully_isolated(&a, &b).await;
    assert_fully_isolated(&b, &a).await;
    assert_intact(&a).await;
    assert_intact(&b).await;
}

#[tokio::test]
async fn blobs_do_not_leak_across_accounts_even_at_identical_content() {
    // Byte-identical messages dedup to one tenant blob, yet an account can
    // only read a blob one of ITS messages references — neither the other
    // tenant nor another user of the same tenant can read it.
    let store = common::test_store().await;

    // Same-tenant users with identical content.
    let tenant = store.create_tenant("blob-shared").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let ua = ts.create_user("a@example.test").await.unwrap();
    let ub = ts.create_user("b@example.test").await.unwrap();
    let a = store.for_account(tenant.clone(), ua);
    let b = store.for_account(tenant, ub);
    let ia = a.inbox().await.unwrap();
    let ib = b.inbox().await.unwrap();
    let raw = b"From: same@example.test\r\nSubject: identical\r\n\r\nidentical body\r\n";
    let ma = a.ingest(&ia, raw).await.unwrap();
    let mb = b.ingest(&ib, raw).await.unwrap();

    // Each reads its own message bytes.
    assert_eq!(a.message_bytes(&ma).await.unwrap().as_ref(), raw);
    assert_eq!(b.message_bytes(&mb).await.unwrap().as_ref(), raw);
    // Neither can read the other's message, though the bytes are identical.
    assert_not_found(a.message_bytes(&mb).await);
    assert_not_found(b.message_bytes(&ma).await);
    // Nor the other's blob directly (same tenant, one deduped blob row).
    let blob_of = |m: &Message| m.blob_id.clone();
    let a_blob = blob_of(&a.message(&ma).await.unwrap());
    let b_blob = blob_of(&b.message(&mb).await.unwrap());
    assert_eq!(a_blob, b_blob, "identical content dedups to one blob row");
    assert!(a.blob_bytes(&a_blob).await.is_ok());
    assert!(b.blob_bytes(&b_blob).await.is_ok());
}

/// The change cursor is per-account: user A's state token advances only
/// on A's own mutations, and A's `/changes` stream never surfaces B's
/// objects — even though A and B share one tenant. This closes the
/// coarse "activity-volume" side channel a tenant-wide modseq left open
/// (migration 0005), the invariant IMAP IDLE relies on.
#[tokio::test]
async fn account_state_is_silent_about_co_tenant_activity() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("modseq-scope").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let ua = ts.create_user("a@example.test").await.unwrap();
    let ub = ts.create_user("b@example.test").await.unwrap();
    let a = store.for_account(tenant.clone(), ua);
    let b = store.for_account(tenant, ub);

    // A mutates once, then observes its state.
    a.deliver(b"From: x@example.test\r\nSubject: a-one\r\n\r\nbody\r\n")
        .await
        .unwrap();
    let a_state_before = a.state().await.unwrap();

    // B mutates several times — noise A must not be able to observe.
    for i in 0..5 {
        b.deliver(format!("From: y@example.test\r\nSubject: b-{i}\r\n\r\nbody\r\n").as_bytes())
            .await
            .unwrap();
    }

    // A's state token has NOT advanced from B's activity.
    assert_eq!(
        a.state().await.unwrap(),
        a_state_before,
        "co-tenant B's mutations must not advance A's state cursor"
    );

    // And A's own change feed since that state is empty (no B objects, no
    // state jump): newState equals the state A already held.
    let since: i64 = a_state_before.parse().unwrap();
    for obj_type in [
        alo_store::changes::TYPE_EMAIL,
        alo_store::changes::TYPE_MAILBOX,
        alo_store::changes::TYPE_THREAD,
    ] {
        let delta = a.changes(obj_type, since, 100).await.unwrap();
        assert!(
            delta.created.is_empty() && delta.updated.is_empty() && delta.destroyed.is_empty(),
            "A must see no changes from B's activity for {obj_type}"
        );
        assert_eq!(
            delta.new_state, since,
            "A's newState must not jump on B's activity for {obj_type}"
        );
    }

    // B, by contrast, saw its own counter advance well past A's.
    let b_state: i64 = b.state().await.unwrap().parse().unwrap();
    assert!(
        b_state > since,
        "B's own cursor advanced on its own mutations"
    );

    // A's next mutation advances A's cursor by exactly its own steps,
    // independent of B's five deliveries in between.
    a.deliver(b"From: x@example.test\r\nSubject: a-two\r\n\r\nbody\r\n")
        .await
        .unwrap();
    let a_after: i64 = a.state().await.unwrap().parse().unwrap();
    assert_eq!(
        a_after,
        since + 1,
        "A's cursor counts only A's transactions, not the tenant's"
    );
}

/// A blob a user has never referenced is `NotFound` for that user even
/// within the same tenant, proving the ownership join is the gate.
#[tokio::test]
async fn unreferenced_blob_is_not_found_for_a_non_owner() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("blob-ref").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let ua = ts.create_user("owner@example.test").await.unwrap();
    let ub = ts.create_user("other@example.test").await.unwrap();
    let a = store.for_account(tenant.clone(), ua);
    let b = store.for_account(tenant, ub);
    let ia = a.inbox().await.unwrap();
    let m = a
        .ingest(&ia, b"From: x@example.test\r\n\r\nowned body\r\n")
        .await
        .unwrap();
    let blob = a.message(&m).await.unwrap().blob_id;
    // The owner reads it; the other same-tenant user cannot.
    assert!(a.blob(&blob).await.is_ok());
    assert_not_found(b.blob(&blob).await);
    assert_not_found(b.blob_bytes(&blob).await);
}

// --- Calendar sharing (Agenda slice 2) --------------------------------------
//
// Sharing deliberately opens a *within-tenant* path: a grant lets another user
// of the same tenant see (and, as editor, write) a calendar they don't own.
// The isolation contract is therefore sharper here than elsewhere — access
// must follow the grant exactly (right subject, right role) and STILL never
// cross a tenant boundary. Every calendar/event query keeps `tenant_id = $1`,
// so a grant can only ever name a subject inside the owner's tenant.

use alo_store::{CalendarEvent, CalendarId, EventId, OccurrenceOverride};
use time::{Duration, OffsetDateTime};

/// A per-occurrence override payload for the given new start (one hour long).
fn sample_override(start: OffsetDateTime) -> OccurrenceOverride {
    OccurrenceOverride {
        summary: "moved".to_owned(),
        description: None,
        location: None,
        starts_at: start,
        ends_at: start + Duration::hours(1),
        all_day: false,
    }
}

/// A one-hour, one-off event on `cal`. `create_event` assigns the real id, so
/// the placeholder here is never persisted.
fn sample_event(cal: &CalendarId, summary: &str) -> CalendarEvent {
    let start = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
    CalendarEvent {
        id: EventId::generate(),
        calendar_id: cal.clone(),
        summary: summary.to_owned(),
        description: None,
        location: None,
        starts_at: start,
        ends_at: start + Duration::hours(1),
        all_day: false,
        recurrence: None,
        attendees: Vec::new(),
        exdates: Vec::new(),
        timezone: None,
        rdates: Vec::new(),
        recurrence_id: None,
        reminder_minutes: None,
        attendee_status: Vec::new(),
    }
}

#[tokio::test]
async fn calendar_sharing_follows_the_grant_and_never_crosses_tenants() {
    let store = common::test_store().await;
    let window = (
        OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        OffsetDateTime::from_unix_timestamp(1_900_000_000).unwrap(),
    );
    let sees = |ks: &[alo_store::Calendar], id: &CalendarId| ks.iter().any(|k| &k.id == id);

    // Tenant 1: owner A, grantee B, stranger C.
    let t1 = store.create_tenant("cal-share").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("owner@example.test").await.unwrap();
    let ub = ts1.create_user("grantee@example.test").await.unwrap();
    let uc = ts1.create_user("stranger@example.test").await.unwrap();
    let a = store.for_account(t1.clone(), ua);
    let b = store.for_account(t1.clone(), ub.clone());
    let c = store.for_account(t1.clone(), uc.clone());

    // A owns a shared calendar carrying one event.
    let cal = a.create_calendar("Team", None).await.unwrap();
    let eid = a
        .create_event(&sample_event(&cal, "standup"))
        .await
        .unwrap();

    // Before any grant, only A can see the calendar or its event.
    assert!(sees(&a.calendars().await.unwrap(), &cal));
    assert!(!sees(&b.calendars().await.unwrap(), &cal));
    assert!(b.event(&eid).await.unwrap().is_none());
    assert!(
        !b.events_in_range(window.0, window.1)
            .await
            .unwrap()
            .iter()
            .any(|e| e.id == eid)
    );

    // Grant B *viewer*: B sees the calendar (role reported) and the event, but
    // may not write; its writes are a clean NotFound and leave A's event intact.
    a.grant_calendar(&cal, "user", ub.as_str(), "viewer")
        .await
        .unwrap();
    let b_cal = b
        .calendars()
        .await
        .unwrap()
        .into_iter()
        .find(|k| k.id == cal)
        .expect("viewer sees the shared calendar");
    assert_eq!(b_cal.role, "viewer");
    assert!(b.event(&eid).await.unwrap().is_some());
    assert!(
        b.events_in_range(window.0, window.1)
            .await
            .unwrap()
            .iter()
            .any(|e| e.id == eid)
    );
    assert!(!b.can_edit_calendar(&cal).await.unwrap());
    assert_not_found(b.delete_event(&eid).await);
    assert_not_found(b.create_event(&sample_event(&cal, "sneaky")).await);
    // A viewer cannot override one occurrence either (same edit gate).
    let slot = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
    assert_not_found(
        b.override_occurrence(&eid, slot, &sample_override(slot))
            .await,
    );
    assert!(a.event(&eid).await.unwrap().is_some());

    // Stranger C has no grant: sees nothing, writes nothing.
    assert!(!sees(&c.calendars().await.unwrap(), &cal));
    assert!(c.event(&eid).await.unwrap().is_none());
    assert_not_found(c.delete_event(&eid).await);

    // Upgrade B to *editor*: B can now update the event; it stays on A's calendar.
    a.grant_calendar(&cal, "user", ub.as_str(), "editor")
        .await
        .unwrap();
    assert!(b.can_edit_calendar(&cal).await.unwrap());
    let mut ev = a.event(&eid).await.unwrap().unwrap();
    ev.summary = "standup (edited by B)".to_owned();
    b.update_event(&eid, &ev).await.unwrap();
    assert_eq!(
        a.event(&eid).await.unwrap().unwrap().summary,
        "standup (edited by B)"
    );
    // As an editor, B may now override a single occurrence.
    b.override_occurrence(&eid, slot, &sample_override(slot))
        .await
        .unwrap();

    // Group sharing reaches every member: C, via the group, sees a second calendar.
    let group = ts1.create_group("eng").await.unwrap();
    ts1.add_group_member(&group, &uc).await.unwrap();
    let cal2 = a.create_calendar("Eng", None).await.unwrap();
    a.grant_calendar(&cal2, "group", group.as_str(), "viewer")
        .await
        .unwrap();
    assert!(sees(&c.calendars().await.unwrap(), &cal2));

    // Cross-tenant: an outsider in tenant 2 sees none of it and cannot forge a
    // grant onto tenant 1's calendar (it isn't theirs to share).
    let t2 = store.create_tenant("cal-other").await.unwrap();
    let ud = store
        .for_tenant(t2.clone())
        .create_user("outsider@example.test")
        .await
        .unwrap();
    let d = store.for_account(t2, ud);
    let d_cals = d.calendars().await.unwrap();
    assert!(!sees(&d_cals, &cal) && !sees(&d_cals, &cal2));
    assert!(d.event(&eid).await.unwrap().is_none());
    assert!(!d.can_edit_calendar(&cal).await.unwrap());
    assert_not_found(d.delete_event(&eid).await);
    assert_not_found(d.grant_calendar(&cal, "user", "anyone", "editor").await);
    assert_not_found(
        d.override_occurrence(&eid, slot, &sample_override(slot))
            .await,
    );
}

// --- Tasks (ADR 0021–0023) --------------------------------------------------
//
// A personal project resolves only for its owner; a team project is visible to
// its whole tenant (v1) but never across tenants. Proposals are scoped the same
// way and never surface as active work.

use alo_store::{NewTask, TaskEdit};

fn task(title: &str) -> NewTask {
    NewTask {
        title: title.to_owned(),
        ..Default::default()
    }
}

#[tokio::test]
async fn tasks_scope_by_project_and_never_cross_tenant() {
    let store = common::test_store().await;

    // Tenant 1: owner A + co-tenant B.
    let t1 = store.create_tenant("task-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@task.test").await.unwrap();
    let ub = ts1.create_user("b@task.test").await.unwrap();
    let ub_id = ub.as_str().to_owned();
    let a = store.for_account(t1.clone(), ua);
    let b = store.for_account(t1.clone(), ub);

    // A: a private task on the personal project, a task on a team project.
    let a_personal = a.ensure_personal_project().await.unwrap();
    let private = a.create_task(&a_personal, &task("private")).await.unwrap();
    let team = a.create_task_project("Team", None).await.unwrap();
    let shared = a.create_task(&team, &task("shared")).await.unwrap();

    // A sees both; B sees the team task but NOT A's personal task.
    assert!(a.task(&private).await.unwrap().is_some());
    assert!(a.task(&shared).await.unwrap().is_some());
    assert!(
        b.task(&private).await.unwrap().is_none(),
        "a personal task is hidden from a co-tenant"
    );
    assert!(
        b.task(&shared).await.unwrap().is_some(),
        "a team task is visible to a co-tenant"
    );
    let b_projects = b.task_projects().await.unwrap();
    assert!(b_projects.iter().any(|p| p.id == team));
    assert!(!b_projects.iter().any(|p| p.id == a_personal));

    // B cannot edit/move/delete A's personal task (not visible → NotFound).
    let edit = TaskEdit {
        title: "hijack".to_owned(),
        priority: "none".to_owned(),
        ..Default::default()
    };
    assert_not_found(b.update_task(&private, &edit).await);
    assert_not_found(b.move_task(&private, "done", 1.0).await);
    assert_not_found(b.delete_task(&private).await);
    assert_eq!(a.task(&private).await.unwrap().unwrap().title, "private"); // intact

    // Cross-tenant: an outsider sees nothing and can touch nothing.
    let t2 = store.create_tenant("task-t2").await.unwrap();
    let ud = store
        .for_tenant(t2.clone())
        .create_user("d@task.test")
        .await
        .unwrap();
    let d = store.for_account(t2, ud);
    assert!(d.task(&private).await.unwrap().is_none());
    assert!(d.task(&shared).await.unwrap().is_none());
    assert_not_found(d.delete_task(&shared).await);
    assert_not_found(d.move_task(&shared, "done", 1.0).await);

    // A task assigned to B is visible to B even though it lives in A's personal
    // project (so an assignee can open and work on their task) — but assignment
    // never crosses tenants.
    let for_b = a
        .create_task(
            &a_personal,
            &NewTask {
                title: "for b".to_owned(),
                assignee: Some(ub_id.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(
        b.task(&for_b).await.unwrap().is_some(),
        "an assignee sees a task assigned to them, even in another's personal project"
    );
    assert!(
        d.task(&for_b).await.unwrap().is_none(),
        "assignment never makes a task visible across tenants"
    );

    // Proposals (ADR 0023): scoped like tasks, and never shown as active work.
    a.create_task(
        &a_personal,
        &NewTask {
            title: "maybe".to_owned(),
            state: Some("proposed".to_owned()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(a.task_proposals().await.unwrap().len(), 1);
    assert_eq!(
        d.task_proposals().await.unwrap().len(),
        0,
        "proposals don't cross tenants"
    );
    assert!(
        a.tasks_in_project(&a_personal)
            .await
            .unwrap()
            .iter()
            .all(|t| t.title != "maybe"),
        "a proposal never appears as active work"
    );
}

/// Email → task (ADR 0024): a task made from an email carries the source link,
/// and that link is just an opaque id scoped like any other task — it never lets
/// an outsider reach across tenants to the source message.
#[tokio::test]
async fn email_sourced_task_keeps_source_and_never_crosses_tenant() {
    let store = common::test_store().await;

    // Tenant 1: A turns an email into a task (explicit path → active).
    let t1 = store.create_tenant("mailtask-t1").await.unwrap();
    let ua = store
        .for_tenant(t1.clone())
        .create_user("a@mailtask.test")
        .await
        .unwrap();
    let a = store.for_account(t1.clone(), ua);
    let project = a.ensure_personal_project().await.unwrap();
    let created = a
        .create_task(
            &project,
            &NewTask {
                title: "Reply to the tender".to_owned(),
                source_kind: Some("email".to_owned()),
                source_id: Some("M-abc123".to_owned()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // The source link round-trips for the owner (task → email marker uses these).
    let got = a.task(&created).await.unwrap().unwrap();
    assert_eq!(got.source_kind.as_deref(), Some("email"));
    assert_eq!(got.source_id.as_deref(), Some("M-abc123"));

    // A different tenant cannot see the task at all — so it can never learn the
    // source id, and the "?open=" round-trip resolves through its own mail door
    // (Email/get is tenant-scoped), never tenant 1's message.
    let t2 = store.create_tenant("mailtask-t2").await.unwrap();
    let ux = store
        .for_tenant(t2.clone())
        .create_user("x@mailtask.test")
        .await
        .unwrap();
    let x = store.for_account(t2, ux);
    assert!(
        x.task(&created).await.unwrap().is_none(),
        "an email-sourced task, and its source link, never cross tenants"
    );
}

/// Task files (ADR 0021 attachments): an attachment is reachable only through a
/// task the caller can see, and the project-wide roll-up honours project
/// visibility — so a personal project's files stay private to its owner and
/// nothing crosses tenants.
#[tokio::test]
async fn task_attachments_scope_by_visibility_and_never_cross_tenant() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("attach-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@attach.test").await.unwrap();
    let ub = ts1.create_user("b@attach.test").await.unwrap();
    let a = store.for_account(t1.clone(), ua);
    let b = store.for_account(t1.clone(), ub);

    // A: a private personal task with a file, and a team task with a file.
    let a_personal = a.ensure_personal_project().await.unwrap();
    let private = a.create_task(&a_personal, &task("private")).await.unwrap();
    a.add_task_attachment(&private, "blob-1", "secret.pdf", 100)
        .await
        .unwrap();
    let team = a.create_task_project("Team", None).await.unwrap();
    let shared = a.create_task(&team, &task("shared")).await.unwrap();
    a.add_task_attachment(&shared, "blob-2", "brief.pdf", 200)
        .await
        .unwrap();

    // A sees both, on the task and in the project roll-up.
    assert_eq!(a.task_attachments(&private).await.unwrap().len(), 1);
    assert_eq!(a.project_files(&a_personal).await.unwrap().len(), 1);
    assert_eq!(a.project_files(&team).await.unwrap().len(), 1);

    // B (co-tenant): the team task's file is visible; the personal one is not,
    // by the task id or the project roll-up, and B can't attach to it.
    assert_eq!(b.task_attachments(&shared).await.unwrap().len(), 1);
    assert_eq!(b.project_files(&team).await.unwrap().len(), 1);
    assert_not_found(b.task_attachments(&private).await);
    assert!(
        b.project_files(&a_personal).await.unwrap().is_empty(),
        "a personal project's files stay private to its owner"
    );
    assert_not_found(b.add_task_attachment(&private, "x", "x.pdf", 1).await);

    // Cross-tenant: an outsider sees nothing and can attach nothing.
    let t2 = store.create_tenant("attach-t2").await.unwrap();
    let ud = store
        .for_tenant(t2.clone())
        .create_user("d@attach.test")
        .await
        .unwrap();
    let d = store.for_account(t2, ud);
    assert_not_found(d.task_attachments(&shared).await);
    assert!(d.project_files(&team).await.unwrap().is_empty());
    assert_not_found(d.add_task_attachment(&shared, "x", "x.pdf", 1).await);
}

/// Task followers (ADR 0021): a follower row is reachable only through a task the
/// caller can see; the creator auto-follows; co-tenants can follow a team task;
/// and nothing crosses tenants.
#[tokio::test]
async fn task_followers_scope_by_visibility_and_never_cross_tenant() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("follow-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@follow.test").await.unwrap();
    let ub = ts1.create_user("b@follow.test").await.unwrap();
    let a = store.for_account(t1.clone(), ua);
    let b = store.for_account(t1.clone(), ub);

    // A creates a team task → A auto-follows it.
    let team = a.create_task_project("Team", None).await.unwrap();
    let shared = a.create_task(&team, &task("shared")).await.unwrap();
    assert_eq!(
        a.task_followers(&shared).await.unwrap().len(),
        1,
        "creator auto-follows"
    );

    // B (co-tenant) can follow the team task, then leave.
    b.follow_task(&shared).await.unwrap();
    assert_eq!(a.task_followers(&shared).await.unwrap().len(), 2);
    b.unfollow_task(&shared).await.unwrap();
    assert_eq!(a.task_followers(&shared).await.unwrap().len(), 1);

    // A's private personal task: B can neither see nor follow it.
    let a_personal = a.ensure_personal_project().await.unwrap();
    let private = a.create_task(&a_personal, &task("private")).await.unwrap();
    assert_not_found(b.task_followers(&private).await);
    assert_not_found(b.follow_task(&private).await);

    // Cross-tenant: an outsider sees no followers and can follow nothing.
    let t2 = store.create_tenant("follow-t2").await.unwrap();
    let ud = store
        .for_tenant(t2.clone())
        .create_user("d@follow.test")
        .await
        .unwrap();
    let d = store.for_account(t2, ud);
    assert_not_found(d.task_followers(&shared).await);
    assert_not_found(d.follow_task(&shared).await);
}

/// Task dependencies: a "blocked by" edge is reachable only through a task the
/// caller can see, both endpoints must be visible to add one, a task can't depend
/// on itself, and no edge — on the task or in the project roll-up — crosses tenants.
#[tokio::test]
async fn task_dependencies_scope_by_visibility_and_never_cross_tenant() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("dep-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@dep.test").await.unwrap();
    let ub = ts1.create_user("b@dep.test").await.unwrap();
    let a = store.for_account(t1.clone(), ua);
    let b = store.for_account(t1.clone(), ub);

    // A team project with two tasks; the second is blocked by the first.
    let team = a.create_task_project("Team", None).await.unwrap();
    let first = a.create_task(&team, &task("first")).await.unwrap();
    let second = a.create_task(&team, &task("second")).await.unwrap();
    a.add_dependency(&second, &first).await.unwrap();
    assert_eq!(
        a.dependencies(&second).await.unwrap().len(),
        1,
        "second is blocked by first"
    );
    assert_eq!(a.project_dependencies(&team).await.unwrap().len(), 1);

    // A task cannot depend on itself.
    assert!(a.add_dependency(&second, &second).await.is_err());

    // A private personal task can't be pointed at from a visible task by an
    // outsider, nor can B read/remove edges on a task it can't see.
    let a_personal = a.ensure_personal_project().await.unwrap();
    let private = a.create_task(&a_personal, &task("private")).await.unwrap();
    assert_not_found(b.dependencies(&private).await);
    assert_not_found(b.add_dependency(&private, &first).await);
    // B (co-tenant) sees the team edge but can't add one pointing at A's private task.
    assert_eq!(b.dependencies(&second).await.unwrap().len(), 1);
    assert_not_found(b.add_dependency(&second, &private).await);

    // Cross-tenant: an outsider sees no edges and can add none, in either direction.
    let t2 = store.create_tenant("dep-t2").await.unwrap();
    let ux = store
        .for_tenant(t2.clone())
        .create_user("x@dep.test")
        .await
        .unwrap();
    let x = store.for_account(t2, ux);
    let x_task = x
        .create_task(&x.ensure_personal_project().await.unwrap(), &task("x"))
        .await
        .unwrap();
    assert_not_found(x.dependencies(&second).await);
    assert!(x.project_dependencies(&team).await.unwrap().is_empty());
    assert_not_found(x.add_dependency(&second, &first).await);
    // Can't make one's own task depend on another tenant's task either.
    assert_not_found(x.add_dependency(&x_task, &first).await);
}

/// Task labels (ADR 0021): labels are tenant-scoped (a shared vocabulary), and a
/// task's labels are reachable only through a task the caller can see. Nothing —
/// the label list, a task's labels, or the batch stamp — crosses tenants.
#[tokio::test]
async fn task_labels_scope_by_tenant_and_task_visibility() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("label-t1").await.unwrap();
    let ua = store
        .for_tenant(t1.clone())
        .create_user("a@label.test")
        .await
        .unwrap();
    let a = store.for_account(t1.clone(), ua);
    let a_personal = a.ensure_personal_project().await.unwrap();
    let the_task = a.create_task(&a_personal, &task("labelled")).await.unwrap();
    let label = a
        .create_task_label("Design", Some("#4b83c4"))
        .await
        .unwrap();
    a.add_task_label(&the_task, &label).await.unwrap();

    assert_eq!(a.labels_for_task(&the_task).await.unwrap().len(), 1);
    assert_eq!(a.task_labels().await.unwrap().len(), 1);
    assert_eq!(
        a.labels_for_task_ids(&[the_task.as_str().to_owned()])
            .await
            .unwrap()
            .get(the_task.as_str())
            .map(Vec::len),
        Some(1),
    );

    // Cross-tenant: an outsider sees no labels and can attach none.
    let t2 = store.create_tenant("label-t2").await.unwrap();
    let ud = store
        .for_tenant(t2.clone())
        .create_user("d@label.test")
        .await
        .unwrap();
    let d = store.for_account(t2, ud);
    assert!(
        d.task_labels().await.unwrap().is_empty(),
        "labels are tenant-scoped"
    );
    assert_not_found(d.labels_for_task(&the_task).await);
    assert_not_found(d.add_task_label(&the_task, &label).await);
    assert!(
        d.labels_for_task_ids(&[the_task.as_str().to_owned()])
            .await
            .unwrap()
            .is_empty(),
        "the batch label stamp never crosses tenants"
    );
}

/// Law #1: deleting a tenant leaves nothing behind. A task query is scoped by
/// `tenant_id`, so an orphaned row (same tenant id, no tenant) would still be
/// returned — this asserts the tenant-delete cascade (migration 0047) removes
/// the task and its children, not just the tenant + users.
#[tokio::test]
async fn deleting_a_tenant_purges_its_tasks() {
    let store = common::test_store().await;
    let t = store.create_tenant("purge-tasks").await.unwrap();
    let u = store
        .for_tenant(t.clone())
        .create_user("a@purge.test")
        .await
        .unwrap();
    let a = store.for_account(t.clone(), u);

    let project = a.ensure_personal_project().await.unwrap();
    let task = a
        .create_task(&project, &task("keep me until purge"))
        .await
        .unwrap();
    a.add_subtask(&task, "a step").await.unwrap();
    a.add_task_comment(&task, "a note").await.unwrap();
    // Present before the delete.
    assert!(a.task(&task).await.unwrap().is_some());
    assert_eq!(a.subtasks(&task).await.unwrap().len(), 1);
    assert_eq!(a.task_comments(&task).await.unwrap().len(), 1);

    store.delete_tenant(&t).await.unwrap();

    // Gone after it — a tenant-scoped query returns nothing because the rows
    // were cascaded away, not merely detached.
    assert!(
        a.task(&task).await.unwrap().is_none(),
        "the task is purged with its tenant, not orphaned"
    );
    assert!(
        a.subtasks(&task).await.unwrap().is_empty(),
        "subtasks purged too"
    );
    assert!(
        a.task_comments(&task).await.unwrap().is_empty(),
        "comments purged too"
    );
    // Read the rows directly rather than through `task_projects()`: that list
    // call first *ensures* the caller's personal project exists, and a write
    // for a tenant that no longer exists is a foreign-key error, not an empty
    // list. The claim under test is about the stored rows.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM task_projects WHERE tenant_id = $1")
            .bind(t.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0, "the tenant's task projects are purged too");
}

/// Spaces (ADR 0026): a Space and its membership are reachable only by members;
/// a non-member — same tenant or another — gets `NotFound` (existence hidden);
/// a member below the required role gets `Forbidden`; and no cross-tenant user
/// can be added to a space.
#[tokio::test]
async fn spaces_scope_by_membership_and_role_never_cross_tenant() {
    use alo_store::{SpaceRole, UserId};
    let store = common::test_store().await;
    let t1 = store.create_tenant("space-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@space.test").await.unwrap();
    let ub = ts1.create_user("b@space.test").await.unwrap();
    let uc = ts1.create_user("c@space.test").await.unwrap();
    let ua_id = ua.as_str().to_owned();
    let ub_id = ub.as_str().to_owned();
    let uc_id = uc.as_str().to_owned();
    let a = store.for_account(t1.clone(), ua);
    let b = store.for_account(t1.clone(), ub);
    let c = store.for_account(t1.clone(), uc);

    // A creates a Space → A is its manager and sees it; the files module is on.
    let space = a.create_space("Acme project").await.unwrap();
    assert_eq!(a.spaces().await.unwrap().len(), 1);
    assert_eq!(
        a.space(&space).await.unwrap().unwrap().my_role,
        SpaceRole::Manager
    );
    assert_eq!(
        a.space_modules(&space).await.unwrap(),
        vec!["files".to_owned()]
    );

    // B is not a member: the Space and its membership are invisible, and B can
    // neither read members nor manage.
    assert!(b.space(&space).await.unwrap().is_none());
    assert!(b.spaces().await.unwrap().is_empty());
    assert_not_found(b.space_members(&space).await);
    assert_not_found(b.rename_space(&space, "hijacked").await);
    assert_not_found(
        b.add_space_member(&space, &UserId::new(ua_id.clone()), SpaceRole::Viewer)
            .await,
    );

    // A adds B as a viewer. Now B sees the space + membership, but as a viewer
    // cannot manage — that is Forbidden (B knows it exists), not NotFound.
    a.add_space_member(&space, &UserId::new(ub_id.clone()), SpaceRole::Viewer)
        .await
        .unwrap();
    assert_eq!(
        b.space(&space).await.unwrap().unwrap().my_role,
        SpaceRole::Viewer
    );
    assert_eq!(b.space_members(&space).await.unwrap().len(), 2);
    assert_forbidden(b.rename_space(&space, "nope").await);
    assert_forbidden(
        b.add_space_member(&space, &UserId::new(uc_id.clone()), SpaceRole::Viewer)
            .await,
    );

    // A promotes B to manager; B can now manage.
    a.add_space_member(&space, &UserId::new(ub_id.clone()), SpaceRole::Manager)
        .await
        .unwrap();
    b.rename_space(&space, "Acme").await.unwrap();
    assert_eq!(a.space(&space).await.unwrap().unwrap().name, "Acme");

    // The last-manager guard: with two managers, A can be removed; the guard
    // only bites when it would leave zero managers.
    a.remove_space_member(&space, &UserId::new(ua_id.clone()))
        .await
        .unwrap();
    // Now B is the only manager and cannot be removed or demoted.
    match b
        .remove_space_member(&space, &UserId::new(ub_id.clone()))
        .await
    {
        Err(StoreError::Conflict(_)) => {}
        other => panic!("expected last-manager Conflict, got {other:?}"),
    }

    // Cross-tenant: an outsider sees nothing, and B cannot add a user from
    // another tenant into the space.
    let t2 = store.create_tenant("space-t2").await.unwrap();
    let ud = store
        .for_tenant(t2.clone())
        .create_user("d@space.test")
        .await
        .unwrap();
    let ud_id = ud.as_str().to_owned();
    let d = store.for_account(t2, ud);
    assert!(d.space(&space).await.unwrap().is_none());
    assert_not_found(d.space_members(&space).await);
    assert_not_found(
        d.add_space_member(&space, &UserId::new(ub_id.clone()), SpaceRole::Viewer)
            .await,
    );
    match b
        .add_space_member(&space, &UserId::new(ud_id), SpaceRole::Viewer)
        .await
    {
        Err(StoreError::Conflict(_)) => {}
        other => panic!("expected cross-tenant-user Conflict, got {other:?}"),
    }

    // C (still a non-member) remains fully locked out throughout.
    assert!(c.space(&space).await.unwrap().is_none());
    assert_not_found(c.space_members(&space).await);
}

/// Sites (ADR 0036): a site is tenant-scoped — an outsider addressing it gets
/// the clean not-found denial on every path — while the subdomain namespace is
/// deliberately global: a claim collides across tenants but reveals only
/// taken/free, never the owner.
#[tokio::test]
async fn sites_scope_by_tenant_and_subdomains_are_globally_unique() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("sites-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@sites.test").await.unwrap();
    let uc = ts1.create_user("c@sites.test").await.unwrap();
    let a = store.for_account(t1.clone(), ua);
    let c = store.for_account(t1, uc);
    let t2 = store.create_tenant("sites-t2").await.unwrap();
    let ub = store
        .for_tenant(t2.clone())
        .create_user("b@sites.test")
        .await
        .unwrap();
    let b = store.for_account(t2, ub);

    // Unique per test run: the compose Postgres is shared across runs and the
    // subdomain namespace is global by design.
    let sub = format!(
        "iso-{}x",
        alo_store::SiteId::generate()
            .as_str()
            .to_lowercase()
            .replace('_', "-")
    );

    // A creates a site; it starts as a draft with an empty theme.
    let site = a.create_site("Acme Widgets", &sub).await.unwrap();
    let got = a.site(&site).await.unwrap().unwrap();
    assert_eq!(got.name, "Acme Widgets");
    assert_eq!(got.subdomain, sub);
    assert_eq!(got.status, alo_store::SiteStatus::Draft);
    assert_eq!(got.theme, serde_json::json!({}));
    assert_eq!(got.default_locale, "en");
    assert_eq!(got.enabled_locales, ["en"]);

    // Sites are tenant-wide: a co-tenant user sees and can manage them.
    assert_eq!(c.sites().await.unwrap().len(), 1);
    c.rename_site(&site, "Acme").await.unwrap();
    assert_eq!(a.site(&site).await.unwrap().unwrap().name, "Acme");

    // An outsider tenant gets the clean denial on every path: never data,
    // never an internal error.
    assert!(b.site(&site).await.unwrap().is_none());
    assert!(b.sites().await.unwrap().is_empty());
    assert_not_found(b.rename_site(&site, "hijacked").await);
    assert_not_found(b.set_site_subdomain(&site, "stolen-subdomain").await);
    assert_not_found(b.set_site_locales(&site, "fr", &["fr".to_owned()]).await);
    assert_not_found(
        b.set_site_theme(
            &site,
            serde_json::json!({"schema_version": 1, "preset": "ink"}),
        )
        .await,
    );
    assert_not_found(b.delete_site(&site).await);
    // ... and nothing they tried changed A's row.
    let untouched = a.site(&site).await.unwrap().unwrap();
    assert_eq!(untouched.subdomain, sub);
    assert_eq!(untouched.theme, serde_json::json!({}));
    assert_eq!(untouched.default_locale, "en");
    assert_eq!(untouched.enabled_locales, ["en"]);

    // A co-tenant can enable the site's visitor languages. Tags are stored in
    // canonical lowercase order, while malformed/default-missing requests are
    // refused before the row changes.
    c.set_site_locales(
        &site,
        "FR",
        &["fr".to_owned(), "NL".to_owned(), "en-GB".to_owned()],
    )
    .await
    .unwrap();
    let multilingual = a.site(&site).await.unwrap().unwrap();
    assert_eq!(multilingual.default_locale, "fr");
    assert_eq!(multilingual.enabled_locales, ["fr", "nl", "en-gb"]);
    match a
        .set_site_locales(&site, "de", &["fr".to_owned(), "nl".to_owned()])
        .await
    {
        Err(StoreError::Conflict(message)) => assert!(message.contains("must also be enabled")),
        other => panic!("expected default-language Conflict, got {other:?}"),
    }
    assert_eq!(
        a.site(&site).await.unwrap().unwrap().enabled_locales,
        multilingual.enabled_locales,
        "a rejected locale write must not change the site"
    );

    // The theme write gate: a co-tenant user can set a valid theme (stored
    // canonically), and off-schema values never reach the column.
    c.set_site_theme(
        &site,
        serde_json::json!({"schema_version": 1, "preset": "terra"}),
    )
    .await
    .unwrap();
    let themed = a.site(&site).await.unwrap().unwrap();
    assert_eq!(
        alo_store::SiteTheme::from_stored(themed.theme.clone()).preset,
        "terra"
    );
    for bad in [
        serde_json::json!({"schema_version": 1, "preset": "vaporwave"}),
        serde_json::json!({"schema_version": 9, "preset": "north"}),
        serde_json::json!({"preset": "north"}),
        serde_json::json!({"schema_version": 1, "preset": "north", "logo": "not/a/token"}),
    ] {
        match a.set_site_theme(&site, bad.clone()).await {
            Err(StoreError::Conflict(_)) => {}
            other => panic!("expected theme-gate Conflict for {bad}, got {other:?}"),
        }
    }
    assert_eq!(
        a.site(&site).await.unwrap().unwrap().theme,
        themed.theme,
        "a rejected theme write must not change the stored value"
    );

    // The global namespace: B cannot claim A's subdomain, and the check
    // answers taken/free only.
    assert!(!b.subdomain_available(&sub).await.unwrap());
    match b.create_site("Other", &sub).await {
        Err(StoreError::Conflict(msg)) => {
            assert!(msg.contains("taken"), "taken/free only, got: {msg}")
        }
        other => panic!("expected subdomain-taken Conflict, got {other:?}"),
    }

    // Validation guards the write paths: reserved and malformed claims never
    // reach the table.
    for bad in ["www", "smtp", "ab", "-bad", "Bad!"] {
        match a.create_site("X", bad).await {
            Err(StoreError::Conflict(_)) => {}
            other => panic!("expected validation Conflict for {bad:?}, got {other:?}"),
        }
    }

    // Deleting the site releases its subdomain for anyone — including another
    // tenant.
    a.delete_site(&site).await.unwrap();
    assert!(a.site(&site).await.unwrap().is_none());
    assert!(b.subdomain_available(&sub).await.unwrap());
    let reclaimed = b.create_site("Reclaimed", &sub).await.unwrap();
    // ... and B's new site is invisible to tenant 1 in turn.
    assert!(a.site(&reclaimed).await.unwrap().is_none());
    assert_not_found(a.delete_site(&reclaimed).await);
}

/// Site pages (ADR 0036): pages scope by (tenant, site) — an outsider tenant
/// gets the clean denial on every path, a page cannot be addressed through a
/// different site of the same tenant, slugs are unique per site (not
/// globally), the home-page rules hold (one home; empty slug only on home),
/// the sections write gate rejects off-schema JSON, and deleting a site
/// cascades to its pages.
#[tokio::test]
async fn site_pages_scope_by_tenant_and_site_with_slug_and_home_rules() {
    use serde_json::json;

    let store = common::test_store().await;
    let t1 = store.create_tenant("pages-t1").await.unwrap();
    let ua = store
        .for_tenant(t1.clone())
        .create_user("a@pages.test")
        .await
        .unwrap();
    let a = store.for_account(t1, ua);
    let t2 = store.create_tenant("pages-t2").await.unwrap();
    let ub = store
        .for_tenant(t2.clone())
        .create_user("b@pages.test")
        .await
        .unwrap();
    let b = store.for_account(t2, ub);

    // Unique per test run: the compose Postgres is shared across runs and the
    // subdomain namespace is global by design.
    let unique = |tag: &str| {
        format!(
            "{tag}-{}x",
            alo_store::SiteId::generate()
                .as_str()
                .to_lowercase()
                .replace('_', "-")
        )
    };
    let site = a.create_site("Acme", &unique("pg")).await.unwrap();

    // The home page lives at the empty slug; ordinary pages append in order.
    let home = a.create_site_page(&site, "Home", "", true).await.unwrap();
    let about = a
        .create_site_page(&site, "About", "about", false)
        .await
        .unwrap();
    let contact = a
        .create_site_page(&site, "Contact", "contact", false)
        .await
        .unwrap();
    let pages = a.site_pages(&site).await.unwrap();
    assert_eq!(
        pages.iter().map(|p| p.nav_order).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    let got = a.site_page(&site, &home).await.unwrap().unwrap();
    assert!(got.is_home && got.slug.is_empty());
    // A new page starts with the empty current-version envelope.
    assert_eq!(got.sections, json!({"schema_version": 1, "sections": []}));

    // Slug rules on write: duplicates (per site), a second home, empty slug
    // on a non-home page, reserved public paths, and malformed slugs all get
    // a clean Conflict — never a row, never a 500.
    let conflict = |result: Result<alo_store::SitePageId, StoreError>| match result {
        Err(StoreError::Conflict(_)) => {}
        other => panic!("expected Conflict, got {other:?}"),
    };
    conflict(a.create_site_page(&site, "Dup", "about", false).await);
    conflict(a.create_site_page(&site, "Second home", "", true).await);
    conflict(a.create_site_page(&site, "No slug", "", false).await);
    for bad in ["blog", "f", "-bad", "Bad", "a b"] {
        conflict(a.create_site_page(&site, "X", bad, false).await);
    }
    // Emptying a non-home slug trips the CHECK, mapped to a Conflict.
    match a.set_page_slug(&site, &about, "").await {
        Err(StoreError::Conflict(_)) => {}
        other => panic!("expected Conflict, got {other:?}"),
    }

    // The sections write gate: schema-valid JSON persists (canonically),
    // off-schema and future-version JSON never reach the row.
    let hero = json!({
        "schema_version": 1,
        "sections": [{"type": "hero", "heading": "Welcome"}]
    });
    a.set_page_sections(&site, &home, hero.clone())
        .await
        .unwrap();
    assert_eq!(
        a.site_page(&site, &home).await.unwrap().unwrap().sections,
        hero
    );
    for bad in [
        json!({"schema_version": 1, "sections": [{"type": "carousel"}]}),
        json!({"schema_version": 2, "sections": []}),
        json!({"sections": []}),
    ] {
        match a.set_page_sections(&site, &home, bad).await {
            Err(StoreError::Conflict(_)) => {}
            other => panic!("expected schema Conflict, got {other:?}"),
        }
    }

    // SEO overrides: set, then blank clears.
    a.set_page_seo(&site, &about, Some("About Acme"), Some("What we do."))
        .await
        .unwrap();
    let got = a.site_page(&site, &about).await.unwrap().unwrap();
    assert_eq!(got.seo_title.as_deref(), Some("About Acme"));
    a.set_page_seo(&site, &about, Some("  "), None)
        .await
        .unwrap();
    let got = a.site_page(&site, &about).await.unwrap().unwrap();
    assert!(got.seo_title.is_none() && got.seo_description.is_none());

    // Localized drafts keep the same page identity. Before French exists the
    // read explicitly falls back to English; after the write it resolves the
    // French slug, SEO, and section envelope independently.
    a.set_site_locales(
        &site,
        "en",
        &["en".to_owned(), "fr".to_owned(), "nl".to_owned()],
    )
    .await
    .unwrap();
    let fallback = a
        .localized_site_page(&site, &about, "FR")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fallback.page.id, about);
    assert_eq!(fallback.requested_locale, "fr");
    assert_eq!(fallback.resolved_locale, "en");
    assert!(fallback.fallback);

    let french = json!({
        "schema_version": 1,
        "sections": [{"type": "hero", "heading": "Notre histoire"}]
    });
    a.set_site_page_locale(
        &site,
        &about,
        "fr",
        "Notre histoire",
        "notre-histoire",
        french.clone(),
        Some("À propos d'Acme"),
        Some("Ce que nous faisons."),
    )
    .await
    .unwrap();
    let localized = a
        .localized_site_page(&site, &about, "fr")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(localized.page.id, about);
    assert_eq!(localized.page.title, "Notre histoire");
    assert_eq!(localized.page.slug, "notre-histoire");
    assert_eq!(localized.page.sections, french);
    assert_eq!(localized.resolved_locale, "fr");
    assert!(!localized.fallback);
    let readiness = a.site_translation_readiness(&site).await.unwrap().unwrap();
    assert_eq!(readiness.default_locale, "en");
    assert_eq!(readiness.total_pages, 3);
    assert_eq!(
        readiness
            .locales
            .iter()
            .map(|language| (language.locale.as_str(), language.translated_pages))
            .collect::<Vec<_>>(),
        [("en", 3), ("fr", 1), ("nl", 0)]
    );
    match a
        .set_site_page_locale(
            &site,
            &contact,
            "fr",
            "Même chemin",
            "notre-histoire",
            json!({"schema_version": 1, "sections": []}),
            None,
            None,
        )
        .await
    {
        Err(StoreError::Conflict(message)) => assert!(message.contains("slug")),
        other => panic!("expected localized slug Conflict, got {other:?}"),
    }
    match a.localized_site_page(&site, &about, "de").await {
        Err(StoreError::Conflict(message)) => assert!(message.contains("not enabled")),
        other => panic!("expected disabled-language Conflict, got {other:?}"),
    }

    // Changing the site's default and then editing that language promotes its
    // draft without destroying the previous default-language content.
    a.set_site_locales(
        &site,
        "fr",
        &["fr".to_owned(), "en".to_owned(), "nl".to_owned()],
    )
    .await
    .unwrap();
    a.set_site_page_locale(
        &site,
        &about,
        "fr",
        "Notre histoire",
        "notre-histoire",
        localized.page.sections.clone(),
        Some("À propos d'Acme"),
        Some("Ce que nous faisons."),
    )
    .await
    .unwrap();
    let english = a
        .localized_site_page(&site, &about, "en")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(english.page.title, "About");
    assert_eq!(english.resolved_locale, "en");
    assert!(!english.fallback);

    // The localized surface has the same clean wrong-tenant denial as the
    // base page surface, on both read and write.
    assert!(
        b.localized_site_page(&site, &about, "fr")
            .await
            .unwrap()
            .is_none()
    );
    assert!(b.site_translation_readiness(&site).await.unwrap().is_none());
    assert_not_found(
        b.set_site_page_locale(
            &site,
            &about,
            "fr",
            "Piraté",
            "pirate",
            json!({"schema_version": 1, "sections": []}),
            None,
            None,
        )
        .await,
    );
    assert_eq!(
        a.localized_site_page(&site, &about, "fr")
            .await
            .unwrap()
            .unwrap()
            .page
            .title,
        "Notre histoire"
    );

    // Moving the home flag: the current home sits at the empty slug, so it
    // must get a real slug first — then the flip demotes it atomically.
    match a.set_home_page(&site, &about).await {
        Err(StoreError::Conflict(_)) => {}
        other => panic!("expected Conflict (home has empty slug), got {other:?}"),
    }
    a.set_page_slug(&site, &home, "welcome").await.unwrap();
    a.set_home_page(&site, &about).await.unwrap();
    let pages = a.site_pages(&site).await.unwrap();
    assert_eq!(
        pages
            .iter()
            .filter(|p| p.is_home)
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        vec![about.as_str()]
    );

    // Nav reorder is a full permutation — anything else is a Conflict.
    a.reorder_site_pages(&site, &[contact.clone(), home.clone(), about.clone()])
        .await
        .unwrap();
    let ordered: Vec<String> = a
        .site_pages(&site)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.id.as_str().to_owned())
        .collect();
    assert_eq!(
        ordered,
        vec![
            contact.as_str().to_owned(),
            home.as_str().to_owned(),
            about.as_str().to_owned()
        ]
    );
    for bad_order in [
        vec![home.clone(), about.clone()],               // missing a page
        vec![home.clone(), home.clone(), about.clone()], // duplicate
        vec![
            home.clone(),
            about.clone(),
            alo_store::SitePageId::generate(),
        ], // stranger
    ] {
        match a.reorder_site_pages(&site, &bad_order).await {
            Err(StoreError::Conflict(_)) => {}
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    // An outsider tenant gets the clean denial on every path — never data,
    // never an internal error — and nothing they tried changed A's rows.
    assert!(b.site_page(&site, &home).await.unwrap().is_none());
    assert!(b.site_pages(&site).await.unwrap().is_empty());
    assert_not_found(
        b.create_site_page(&site, "Intruder", "intruder", false)
            .await,
    );
    assert_not_found(b.set_page_title(&site, &home, "hijacked").await);
    assert_not_found(b.set_page_slug(&site, &home, "hijacked").await);
    assert_not_found(b.set_page_seo(&site, &home, Some("x"), None).await);
    assert_not_found(
        b.set_page_sections(&site, &home, json!({"schema_version": 1, "sections": []}))
            .await,
    );
    assert_not_found(b.set_home_page(&site, &home).await);
    assert_not_found(
        b.reorder_site_pages(&site, std::slice::from_ref(&home))
            .await,
    );
    assert_not_found(b.delete_site_page(&site, &home).await);
    let got = a.site_page(&site, &home).await.unwrap().unwrap();
    assert_eq!(got.title, "Home");
    assert_eq!(got.slug, "welcome");
    assert_eq!(got.sections, hero);

    // Pages also scope by site within the same tenant: another site of A's
    // cannot address them, and slugs are only unique per site.
    let site2 = a.create_site("Beta", &unique("pg")).await.unwrap();
    assert!(a.site_page(&site2, &about).await.unwrap().is_none());
    assert_not_found(a.set_page_title(&site2, &about, "cross-site").await);
    assert_not_found(a.delete_site_page(&site2, &about).await);
    a.create_site_page(&site2, "About", "about", false)
        .await
        .unwrap();

    // Deleting a page frees its slug; deleting the site cascades the rest.
    a.delete_site_page(&site, &contact).await.unwrap();
    assert!(a.site_page(&site, &contact).await.unwrap().is_none());
    a.create_site_page(&site, "Contact again", "contact", false)
        .await
        .unwrap();
    a.delete_site(&site).await.unwrap();
    assert!(a.site_pages(&site).await.unwrap().is_empty());
    assert!(a.site_page(&site, &about).await.unwrap().is_none());
}

/// Site forms and submissions (ADR 0036): both scope by (tenant, site) — an
/// outsider tenant gets the clean denial on every path, a form cannot be
/// addressed through another site of the same tenant, the submission write
/// gate rejects malformed fields, and deleting a form or its site cascades
/// the submissions away.
#[tokio::test]
async fn site_forms_and_submissions_scope_by_tenant_and_site() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("forms-t1").await.unwrap();
    let ua = store
        .for_tenant(t1.clone())
        .create_user("a@forms.test")
        .await
        .unwrap();
    let a = store.for_account(t1, ua);
    let t2 = store.create_tenant("forms-t2").await.unwrap();
    let ub = store
        .for_tenant(t2.clone())
        .create_user("b@forms.test")
        .await
        .unwrap();
    let b = store.for_account(t2, ub);

    // Unique per test run: the compose Postgres is shared across runs and the
    // subdomain namespace is global by design.
    let unique = |tag: &str| {
        format!(
            "{tag}-{}x",
            alo_store::SiteId::generate()
                .as_str()
                .to_lowercase()
                .replace('_', "-")
        )
    };
    let site = a.create_site("Acme", &unique("fm")).await.unwrap();
    let site2 = a.create_site("Beta", &unique("fm")).await.unwrap();

    // ---- form CRUD on the owner's own site ----------------------------------
    let form = a.create_site_form(&site, "  Contact  ").await.unwrap();
    let got = a.site_form(&site, &form).await.unwrap().unwrap();
    assert_eq!(got.name, "Contact");
    assert_eq!(a.site_forms(&site).await.unwrap().len(), 1);
    a.rename_site_form(&site, &form, "Sales").await.unwrap();
    assert_eq!(
        a.site_form(&site, &form).await.unwrap().unwrap().name,
        "Sales"
    );
    match a.create_site_form(&site, "   ").await {
        Err(StoreError::Validation(_)) => {}
        other => panic!("expected Validation on a blank name, got {other:?}"),
    }

    // Creating on a foreign site is the clean denial, not a stray row.
    assert_not_found(b.create_site_form(&site, "Intruder").await);

    // ---- submissions: write gate, then newest-first reads -------------------
    let first = a
        .add_site_form_submission(&site, &form, "Ada", "ada@example.test", "First message")
        .await
        .unwrap();
    let second = a
        .add_site_form_submission(
            &site,
            &form,
            "Grace",
            "grace@example.test",
            "Second message",
        )
        .await
        .unwrap();
    let listed = a.site_form_submissions(&site, &form).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].sender_name, "Grace"); // newest first
    assert!(!listed[0].handled);
    for (name, email, message) in [
        ("", "ada@example.test", "hi"),
        ("Ada", "not-an-email", "hi"),
        ("Ada", "ada@example.test", "   "),
    ] {
        match a
            .add_site_form_submission(&site, &form, name, email, message)
            .await
        {
            Err(StoreError::Validation(_)) => {}
            other => {
                panic!("expected Validation for {name:?}/{email:?}/{message:?}, got {other:?}")
            }
        }
    }
    a.set_form_submission_handled(&site, &form, &first, true)
        .await
        .unwrap();
    let listed = a.site_form_submissions(&site, &form).await.unwrap();
    assert!(listed.iter().any(|s| s.id == first && s.handled));
    assert!(listed.iter().any(|s| s.id == second && !s.handled));

    // ---- the outsider tenant: clean denial on every path --------------------
    assert!(b.site_form(&site, &form).await.unwrap().is_none());
    assert!(b.site_forms(&site).await.unwrap().is_empty());
    assert_not_found(b.rename_site_form(&site, &form, "hijacked").await);
    assert_not_found(b.delete_site_form(&site, &form).await);
    assert_not_found(
        b.add_site_form_submission(&site, &form, "Eve", "eve@example.test", "intrusion")
            .await,
    );
    assert!(
        b.site_form_submissions(&site, &form)
            .await
            .unwrap()
            .is_empty()
    );
    assert_not_found(
        b.set_form_submission_handled(&site, &form, &second, true)
            .await,
    );
    assert_not_found(b.delete_form_submission(&site, &form, &second).await);
    // ... and nothing they tried changed A's rows.
    assert_eq!(
        a.site_form(&site, &form).await.unwrap().unwrap().name,
        "Sales"
    );
    assert_eq!(
        a.site_form_submissions(&site, &form).await.unwrap().len(),
        2
    );

    // ---- forms also scope by site within the same tenant --------------------
    assert!(a.site_form(&site2, &form).await.unwrap().is_none());
    assert!(a.site_forms(&site2).await.unwrap().is_empty());
    assert_not_found(a.rename_site_form(&site2, &form, "cross-site").await);
    assert_not_found(
        a.add_site_form_submission(&site2, &form, "Ada", "ada@example.test", "cross-site")
            .await,
    );
    assert!(
        a.site_form_submissions(&site2, &form)
            .await
            .unwrap()
            .is_empty()
    );
    assert_not_found(
        a.set_form_submission_handled(&site2, &form, &second, true)
            .await,
    );
    assert_not_found(a.delete_form_submission(&site2, &form, &second).await);

    // ---- deletes cascade ----------------------------------------------------
    a.delete_form_submission(&site, &form, &second)
        .await
        .unwrap();
    assert_eq!(
        a.site_form_submissions(&site, &form).await.unwrap().len(),
        1
    );
    a.delete_site_form(&site, &form).await.unwrap();
    assert!(a.site_form(&site, &form).await.unwrap().is_none());
    assert!(
        a.site_form_submissions(&site, &form)
            .await
            .unwrap()
            .is_empty()
    );
    let survivor = a.create_site_form(&site, "Contact").await.unwrap();
    a.delete_site(&site).await.unwrap();
    assert!(a.site_form(&site, &survivor).await.unwrap().is_none());
    assert!(a.site_forms(&site).await.unwrap().is_empty());
}

/// Site publishing (ADR 0036): a publish freezes pages, exact language drafts,
/// the locale contract, and the theme into immutable snapshots. Later edits
/// never change the published set — only a republish creates the next set.
/// An outsider tenant gets the clean denial on every path, and a publish
/// cannot be addressed through another site of the same tenant.
#[tokio::test]
async fn site_publishes_freeze_immutable_snapshots_and_scope_by_tenant() {
    use serde_json::json;

    let store = common::test_store().await;
    let t1 = store.create_tenant("publish-t1").await.unwrap();
    let ua = store
        .for_tenant(t1.clone())
        .create_user("a@publish.test")
        .await
        .unwrap();
    let ua_id = ua.as_str().to_owned();
    let a = store.for_account(t1, ua);
    let t2 = store.create_tenant("publish-t2").await.unwrap();
    let ub = store
        .for_tenant(t2.clone())
        .create_user("b@publish.test")
        .await
        .unwrap();
    let b = store.for_account(t2, ub);

    // Unique per test run: the compose Postgres is shared across runs and the
    // subdomain namespace is global by design.
    let unique = |tag: &str| {
        format!(
            "{tag}-{}x",
            alo_store::SiteId::generate()
                .as_str()
                .to_lowercase()
                .replace('_', "-")
        )
    };
    let site = a.create_site("Acme", &unique("pub")).await.unwrap();
    let published_languages = vec!["en".to_owned(), "fr".to_owned(), "nl".to_owned()];
    a.set_site_locales(&site, "en", &published_languages)
        .await
        .unwrap();

    // An empty site must not go live; neither may one without a home page.
    let conflict = |result: Result<alo_store::SitePublishId, StoreError>| match result {
        Err(StoreError::Conflict(_)) => {}
        other => panic!("expected publish Conflict, got {other:?}"),
    };
    conflict(a.publish_site(&site).await);
    let about = a
        .create_site_page(&site, "About", "about", false)
        .await
        .unwrap();
    conflict(a.publish_site(&site).await);

    // With a home page (and a theme) the site publishes and goes live.
    let home = a.create_site_page(&site, "Home", "", true).await.unwrap();
    let hero = json!({
        "schema_version": 1,
        "sections": [{"type": "hero", "heading": "Welcome"}]
    });
    a.set_page_sections(&site, &home, hero.clone())
        .await
        .unwrap();
    let french_hero = json!({
        "schema_version": 1,
        "sections": [{"type": "hero", "heading": "Bienvenue"}]
    });
    a.set_site_page_locale(
        &site,
        &home,
        "fr",
        "Accueil",
        "",
        french_hero.clone(),
        Some("Accueil Acme"),
        Some("Bienvenue chez Acme"),
    )
    .await
    .unwrap();
    a.set_site_page_locale(
        &site,
        &about,
        "fr",
        "Notre histoire",
        "notre-histoire",
        json!({"schema_version": 1, "sections": []}),
        None,
        None,
    )
    .await
    .unwrap();
    let terra = json!({"schema_version": 1, "preset": "terra"});
    a.set_site_theme(&site, terra.clone()).await.unwrap();
    let p1 = a.publish_site(&site).await.unwrap();
    assert_eq!(
        a.site(&site).await.unwrap().unwrap().status,
        alo_store::SiteStatus::Live
    );
    let current = a.current_site_publish(&site).await.unwrap().unwrap();
    assert_eq!(current.id, p1);
    assert_eq!(current.published_by, ua_id);
    assert_eq!(current.theme, terra);
    assert_eq!(current.default_locale, "en");
    assert_eq!(current.enabled_locales, published_languages);
    let frozen = a.site_publish_snapshots(&site, &p1).await.unwrap();
    assert_eq!(frozen.len(), 4, "only exact en/fr drafts are frozen");
    let frozen_home_en = frozen
        .iter()
        .find(|page| page.page_id == home && page.locale == "en")
        .unwrap();
    assert_eq!(frozen_home_en.slug, "");
    assert!(frozen_home_en.is_home);
    assert_eq!(frozen_home_en.sections, hero);
    let frozen_home_fr = frozen
        .iter()
        .find(|page| page.page_id == home && page.locale == "fr")
        .unwrap();
    assert_eq!(frozen_home_fr.title, "Accueil");
    assert_eq!(frozen_home_fr.sections, french_hero);
    assert!(frozen.iter().all(|page| page.locale != "nl"));

    // Drafts never leak: edit, add, delete, and retheme AFTER publishing —
    // the published set must not move by a single byte.
    a.set_page_sections(
        &site,
        &home,
        json!({
            "schema_version": 1,
            "sections": [{
                "type": "cta",
                "heading": "Buy now",
                "button": {"label": "Buy", "href": "/pricing"}
            }]
        }),
    )
    .await
    .unwrap();
    a.set_page_title(&site, &about, "Team").await.unwrap();
    a.set_site_page_locale(
        &site,
        &home,
        "fr",
        "Nouvel accueil",
        "",
        json!({"schema_version": 1, "sections": []}),
        None,
        None,
    )
    .await
    .unwrap();
    a.create_site_page(&site, "Pricing", "pricing", false)
        .await
        .unwrap();
    a.delete_site_page(&site, &about).await.unwrap();
    a.set_site_theme(&site, json!({"schema_version": 1, "preset": "ink"}))
        .await
        .unwrap();
    assert_eq!(a.current_site_publish(&site).await.unwrap().unwrap().id, p1);
    let still = a.site_publish_snapshots(&site, &p1).await.unwrap();
    assert_eq!(still.len(), 4);
    assert!(still.iter().any(|page| page.title == "About"));
    assert_eq!(
        still.iter().filter(|page| page.page_id == about).count(),
        2,
        "both language snapshots must survive the page deletion"
    );
    assert_eq!(
        still
            .iter()
            .find(|page| page.page_id == home && page.locale == "fr")
            .unwrap()
            .sections,
        french_hero,
        "localized snapshots must not follow draft edits"
    );
    assert_eq!(
        a.current_site_publish(&site).await.unwrap().unwrap().theme,
        terra,
        "the published theme is the one frozen at publish time"
    );

    // Republish: a NEW set reflecting today's draft; the old set survives
    // untouched (immutable history).
    let next_languages = vec!["en".to_owned(), "nl".to_owned()];
    a.set_site_locales(&site, "en", &next_languages)
        .await
        .unwrap();
    a.set_site_page_locale(
        &site,
        &home,
        "nl",
        "Start",
        "",
        json!({"schema_version": 1, "sections": []}),
        None,
        None,
    )
    .await
    .unwrap();
    let p2 = a.publish_site(&site).await.unwrap();
    assert_ne!(p2, p1);
    assert_eq!(a.current_site_publish(&site).await.unwrap().unwrap().id, p2);
    let republished = a.site_publish_snapshots(&site, &p2).await.unwrap();
    assert_eq!(republished.len(), 3);
    assert!(
        republished.iter().all(|s| s.page_id != about),
        "the deleted page must not be in the new set"
    );
    assert!(republished.iter().any(|s| s.slug == "pricing"));
    assert!(republished.iter().any(|s| s.locale == "nl"));
    assert!(republished.iter().all(|s| s.locale != "fr"));
    let current = a.current_site_publish(&site).await.unwrap().unwrap();
    assert_eq!(current.enabled_locales, next_languages);
    let old = a.site_publish_snapshots(&site, &p1).await.unwrap();
    assert_eq!(old.len(), 4);
    assert!(old.iter().any(|page| page.locale == "fr"));

    // An outsider tenant gets the clean denial on every path — never data,
    // never an internal error — and A's published state is untouched.
    assert_not_found(b.publish_site(&site).await);
    assert_not_found(b.unpublish_site(&site).await);
    assert!(b.current_site_publish(&site).await.unwrap().is_none());
    assert!(
        b.site_publish_snapshots(&site, &p2)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(a.current_site_publish(&site).await.unwrap().unwrap().id, p2);

    // A publish also scopes by site within the tenant: another site of A's
    // cannot address it.
    let site2 = a.create_site("Beta", &unique("pub")).await.unwrap();
    assert!(
        a.site_publish_snapshots(&site2, &p2)
            .await
            .unwrap()
            .is_empty()
    );

    // Unpublish hides the site but erases nothing; republishing works again.
    a.unpublish_site(&site).await.unwrap();
    assert_eq!(
        a.site(&site).await.unwrap().unwrap().status,
        alo_store::SiteStatus::Draft
    );
    assert!(a.current_site_publish(&site).await.unwrap().is_none());
    assert_eq!(a.site_publish_snapshots(&site, &p1).await.unwrap().len(), 4);
    assert_eq!(a.site_publish_snapshots(&site, &p2).await.unwrap().len(), 3);
    let p3 = a.publish_site(&site).await.unwrap();
    assert_eq!(a.current_site_publish(&site).await.unwrap().unwrap().id, p3);

    // Deleting the site cascades its publishes and snapshots (and the
    // published-set pointer goes with the row).
    a.delete_site(&site).await.unwrap();
    assert!(a.current_site_publish(&site).await.unwrap().is_none());
    assert!(
        a.site_publish_snapshots(&site, &p3)
            .await
            .unwrap()
            .is_empty()
    );
}

/// Drive (ADR 0027): a node's access follows its location. Personal files are
/// private to their owner; Space files are readable by members and writable by
/// editors+; moving a file re-scopes its access; and nothing — a node, its
/// bytes, its versions — ever crosses to a non-member or another tenant.
#[tokio::test]
async fn drive_nodes_scope_by_location_and_never_cross_tenant() {
    use alo_store::{DriveLocation, NewDriveFile, SpaceRole, UserId};
    let store = common::test_store().await;
    let t1 = store.create_tenant("drive-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@drive.test").await.unwrap();
    let ub = ts1.create_user("b@drive.test").await.unwrap();
    let ub_id = ub.as_str().to_owned();
    let a = store.for_account(t1.clone(), ua);
    let b = store.for_account(t1.clone(), ub);

    let file = |name: &str| NewDriveFile {
        name: name.to_owned(),
        blob_id: format!("blob-{name}"),
        size: 10,
        ..Default::default()
    };

    // A's personal file: private to A. B (same tenant) cannot see it at all.
    let mine = a
        .drive_create_file(&DriveLocation::Personal, None, &file("secret.txt"))
        .await
        .unwrap();
    assert!(a.drive_node(&mine).await.unwrap().is_some());
    assert_eq!(
        a.drive_list(&DriveLocation::Personal, None)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        b.drive_node(&mine).await.unwrap().is_none(),
        "another user's personal file is invisible"
    );
    assert_not_found(b.drive_rename(&mine, "hax").await);
    // B's own personal location never shows A's file.
    assert!(
        b.drive_list(&DriveLocation::Personal, None)
            .await
            .unwrap()
            .is_empty()
    );

    // A Space with a file. B is added as a viewer.
    let space = a.create_space("Team").await.unwrap();
    let sloc = DriveLocation::Space(space.clone());
    let folder = a.drive_create_folder(&sloc, None, "Docs").await.unwrap();
    let shared = a
        .drive_create_file(&sloc, Some(&folder), &file("brief.pdf"))
        .await
        .unwrap();
    a.add_space_member(&space, &UserId::new(ub_id.clone()), SpaceRole::Viewer)
        .await
        .unwrap();

    // Viewer B can read the space's files but not write them.
    assert!(
        b.drive_node(&shared).await.unwrap().is_some(),
        "a member reads space files"
    );
    assert_eq!(b.drive_list(&sloc, Some(&folder)).await.unwrap().len(), 1);
    assert_forbidden(b.drive_rename(&shared, "nope").await);
    assert_forbidden(b.drive_create_file(&sloc, None, &file("x")).await);

    // Promote B to editor → now B can write.
    a.add_space_member(&space, &UserId::new(ub_id.clone()), SpaceRole::Editor)
        .await
        .unwrap();
    b.drive_rename(&shared, "brief-v2.pdf").await.unwrap();
    assert_eq!(
        a.drive_node(&shared).await.unwrap().unwrap().name,
        "brief-v2.pdf"
    );

    // Versioning: a new upload appends a version; history is kept.
    let v = a.drive_add_version(&shared, "blob-new", 20).await.unwrap();
    assert_eq!(v, 2);
    assert_eq!(a.drive_versions(&shared).await.unwrap().len(), 2);

    // Move re-scopes access: move A's PERSONAL file into the Space → B (a member)
    // can now see it; move it back → B loses access.
    a.drive_move(&mine, &sloc, None).await.unwrap();
    assert!(
        b.drive_node(&mine).await.unwrap().is_some(),
        "moving into a space grants members access"
    );
    a.drive_move(&mine, &DriveLocation::Personal, None)
        .await
        .unwrap();
    assert!(
        b.drive_node(&mine).await.unwrap().is_none(),
        "moving back out revokes it"
    );

    // Trash / restore keep scoping.
    a.drive_trash_node(&shared).await.unwrap();
    assert!(
        a.drive_list(&sloc, Some(&folder)).await.unwrap().is_empty(),
        "trashed is hidden from listing"
    );
    assert_eq!(a.drive_trash(&sloc).await.unwrap().len(), 1);
    a.drive_restore_node(&shared).await.unwrap();
    assert_eq!(a.drive_list(&sloc, Some(&folder)).await.unwrap().len(), 1);

    // Cross-tenant: an outsider sees nothing and can touch nothing.
    let t2 = store.create_tenant("drive-t2").await.unwrap();
    let ud = store
        .for_tenant(t2.clone())
        .create_user("d@drive.test")
        .await
        .unwrap();
    let d = store.for_account(t2, ud);
    assert!(d.drive_node(&shared).await.unwrap().is_none());
    assert!(d.drive_node(&mine).await.unwrap().is_none());
    assert_not_found(d.drive_rename(&shared, "evil").await);
    assert_not_found(d.drive_versions(&shared).await);
    assert_not_found(d.drive_move(&shared, &DriveLocation::Personal, None).await);
    // The outsider cannot even address the Space location as their own.
    assert_not_found(
        d.drive_list(&DriveLocation::Space(space.clone()), None)
            .await,
    );
}

/// alo Base (ADR 0032): a Base's data is reachable only through its Drive node's
/// access — a Space viewer reads but cannot write (Forbidden), an editor writes,
/// a non-member or another tenant gets NotFound/None on every path.
#[tokio::test]
async fn base_data_scopes_through_its_drive_node() {
    use alo_store::{DriveLocation, SpaceRole, UserId};
    use serde_json::json;
    let store = common::test_store().await;
    let t1 = store.create_tenant("base-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@base.test").await.unwrap();
    let ub = ts1.create_user("b@base.test").await.unwrap();
    let ub_id = ub.as_str().to_owned();
    let a = store.for_account(t1.clone(), ua);
    let b = store.for_account(t1.clone(), ub);

    // A creates a Space + a Base in it; B is added as a viewer.
    let space = a.create_space("Team").await.unwrap();
    let sloc = DriveLocation::Space(space.clone());
    let node = a.create_base(&sloc, None, "CRM").await.unwrap();
    a.add_space_member(&space, &UserId::new(ub_id.clone()), SpaceRole::Viewer)
        .await
        .unwrap();

    // The Base loads with its default table; a record can be added by A.
    let base = a.base(&node).await.unwrap().unwrap();
    assert_eq!(base.tables.len(), 1, "default table");
    let table = base.tables[0].id.clone();
    assert_eq!(base.tables[0].fields.len(), 2, "Name + Notes");
    assert_eq!(base.tables[0].records.len(), 3, "three seed rows");
    let rec = a
        .base_add_record(&table, &json!({ "x": "hi" }))
        .await
        .unwrap();

    // Viewer B can READ the base but not write it.
    assert!(
        b.base(&node).await.unwrap().is_some(),
        "a member reads the base"
    );
    assert_forbidden(b.base_add_record(&table, &json!({})).await);
    assert_forbidden(b.base_update_record(&rec, &json!({})).await);
    assert_forbidden(b.base_add_field(&table, "Extra", "text", &json!({})).await);
    assert_forbidden(b.base_add_table(&node, "T2").await);

    // Promote B to editor → now B can write.
    a.add_space_member(&space, &UserId::new(ub_id.clone()), SpaceRole::Editor)
        .await
        .unwrap();
    b.base_update_record(&rec, &json!({ "x": "edited" }))
        .await
        .unwrap();

    // A private personal Base stays private to its owner.
    let personal = a
        .create_base(&DriveLocation::Personal, None, "Private")
        .await
        .unwrap();
    assert!(a.base(&personal).await.unwrap().is_some());
    assert!(
        b.base(&personal).await.unwrap().is_none(),
        "another user's personal base is invisible"
    );

    // Cross-tenant: an outsider sees nothing and can write nothing.
    let t2 = store.create_tenant("base-t2").await.unwrap();
    let ud = store
        .for_tenant(t2.clone())
        .create_user("d@base.test")
        .await
        .unwrap();
    let d = store.for_account(t2, ud);
    assert!(d.base(&node).await.unwrap().is_none());
    assert_not_found(d.base_add_record(&table, &json!({})).await);
    assert_not_found(d.base_update_record(&rec, &json!({})).await);
    assert_not_found(d.base_add_table(&node, "evil").await);
    // A bad field type / view kind is refused (Conflict), not a silent accept.
    assert!(
        a.base_add_field(&table, "F", "not-a-type", &json!({}))
            .await
            .is_err()
    );
    assert!(
        a.base_add_view(&table, "not-a-kind", "V", &json!({}))
            .await
            .is_err()
    );
}

/// Workspace search (ADR 0029): only surfaces what the caller can already see —
/// their personal files, member-Space files, and visible tasks — never another
/// user's private items and never another tenant's.
#[tokio::test]
async fn workspace_search_only_returns_visible_items() {
    use alo_store::{DriveLocation, NewDriveFile};
    let store = common::test_store().await;
    let t1 = store.create_tenant("srch-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@srch.test").await.unwrap();
    let ub = ts1.create_user("b@srch.test").await.unwrap();
    let a = store.for_account(t1.clone(), ua);
    let b = store.for_account(t1.clone(), ub);

    // A's private file + task both mention "acme".
    a.drive_create_file(
        &DriveLocation::Personal,
        None,
        &NewDriveFile {
            name: "Acme brief".to_owned(),
            blob_id: "x".to_owned(),
            size: 1,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let proj = a.ensure_personal_project().await.unwrap();
    a.create_task(&proj, &task("Acme kickoff")).await.unwrap();

    // A finds both; B (same tenant, non-owner) finds neither (private).
    assert_eq!(a.workspace_search("acme", 20).await.unwrap().len(), 2);
    assert!(
        b.workspace_search("acme", 20).await.unwrap().is_empty(),
        "another user's private items"
    );

    // Cross-tenant: an outsider finds nothing.
    let t2 = store.create_tenant("srch-t2").await.unwrap();
    let ud = store
        .for_tenant(t2.clone())
        .create_user("d@srch.test")
        .await
        .unwrap();
    let d = store.for_account(t2, ud);
    assert!(
        d.workspace_search("acme", 20).await.unwrap().is_empty(),
        "never across tenants"
    );

    // An empty query is empty, not everything.
    assert!(a.workspace_search("   ", 20).await.unwrap().is_empty());
}

/// Workspace search — mail is matched by full CONTENT (the body, via the mail
/// full-text index), not just the subject, and only ever the caller's own mail.
#[tokio::test]
async fn workspace_search_finds_own_mail_by_body_only() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("srchm-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@srchm.test").await.unwrap();
    let ub = ts1.create_user("b@srchm.test").await.unwrap();
    let a = store.for_account(t1.clone(), ua);
    let b = store.for_account(t1.clone(), ub);
    let inbox = a.inbox().await.unwrap();

    // The distinctive term lives ONLY in the body, never the subject — so a hit
    // proves content search, not subject search.
    let raw = "From: sender@example.test\r\nSubject: Status update\r\n\r\n\
               Please review the flamingo migration plan before Friday.\r\n";
    a.ingest(&inbox, raw.as_bytes()).await.unwrap();

    let hits = a.workspace_search("flamingo", 20).await.unwrap();
    assert_eq!(hits.len(), 1, "own message matched by body");
    assert_eq!(hits[0].kind, "message");

    // A co-tenant who doesn't own the mailbox sees nothing; nor does another
    // tenant. Mail is per-user (Law 1).
    assert!(
        b.workspace_search("flamingo", 20).await.unwrap().is_empty(),
        "another user's mail"
    );
    let t2 = store.create_tenant("srchm-t2").await.unwrap();
    let ud = store
        .for_tenant(t2.clone())
        .create_user("d@srchm.test")
        .await
        .unwrap();
    let d = store.for_account(t2, ud);
    assert!(
        d.workspace_search("flamingo", 20).await.unwrap().is_empty(),
        "never across tenants"
    );
}

/// Workspace search — Drive files match by CONTENT, not just name: a plain-text
/// file and an alo Doc are indexed from their bytes at write time. Content is
/// still location-scoped (personal → owner only), never cross-tenant.
#[tokio::test]
async fn workspace_search_finds_drive_files_by_content() {
    use alo_store::{DriveLocation, NewDriveFile};
    use bytes::Bytes;
    let store = common::test_store().await;
    let t1 = store.create_tenant("srchc-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@srchc.test").await.unwrap();
    let ub = ts1.create_user("b@srchc.test").await.unwrap();
    let a = store.for_account(t1.clone(), ua);
    let b = store.for_account(t1.clone(), ub);

    // A text file whose NAME ("notes.txt") does not contain the term, but whose
    // BODY does.
    let txt = a
        .put_blob(
            Bytes::from_static(b"the migration plan is due friday"),
            Some("text/plain"),
        )
        .await
        .unwrap();
    a.drive_create_file(
        &DriveLocation::Personal,
        None,
        &NewDriveFile {
            name: "notes.txt".to_owned(),
            blob_id: txt.as_str().to_owned(),
            size: 32,
            content_type: Some("text/plain".to_owned()),
            kind: Some("file".to_owned()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // An alo Doc (BlockNote JSON) with a distinctive word only in its text run.
    let doc_json = br#"[{"type":"paragraph","content":[{"type":"text","text":"secret pangolin roadmap","styles":{}}]}]"#;
    let dblob = a
        .put_blob(Bytes::from_static(doc_json), Some("application/json"))
        .await
        .unwrap();
    a.drive_create_file(
        &DriveLocation::Personal,
        None,
        &NewDriveFile {
            name: "Untitled".to_owned(),
            blob_id: dblob.as_str().to_owned(),
            size: doc_json.len() as i64,
            content_type: Some("application/json".to_owned()),
            kind: Some("doc".to_owned()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // A finds each by a word that lives only inside the file.
    let mig = a.workspace_search("migration", 20).await.unwrap();
    assert_eq!(mig.len(), 1, "text file matched by body");
    let pan = a.workspace_search("pangolin", 20).await.unwrap();
    assert_eq!(pan.len(), 1, "alo Doc matched by content");
    assert_eq!(pan[0].kind, "doc");

    // A co-tenant who doesn't own these personal files sees nothing; neither
    // does another tenant.
    assert!(
        b.workspace_search("migration", 20)
            .await
            .unwrap()
            .is_empty(),
        "another user's private file"
    );
    let t2 = store.create_tenant("srchc-t2").await.unwrap();
    let ud = store
        .for_tenant(t2.clone())
        .create_user("d@srchc.test")
        .await
        .unwrap();
    let d = store.for_account(t2, ud);
    assert!(
        d.workspace_search("pangolin", 20).await.unwrap().is_empty(),
        "never across tenants"
    );
}

/// AI retrieval reduces a natural-language question to keywords, so it matches
/// items a literal substring of the whole question never would — still scoped to
/// what the caller can see.
#[tokio::test]
async fn workspace_search_terms_matches_question_keywords() {
    use alo_store::{DriveLocation, NewDriveFile};
    let store = common::test_store().await;
    let t1 = store.create_tenant("kw-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@kw.test").await.unwrap();
    let ub = ts1.create_user("b@kw.test").await.unwrap();
    let a = store.for_account(t1.clone(), ua);
    let b = store.for_account(t1.clone(), ub);

    a.drive_create_file(
        &DriveLocation::Personal,
        None,
        &NewDriveFile {
            name: "Acme proposal.docx".to_owned(),
            blob_id: "x".to_owned(),
            size: 1,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let proj = a.ensure_personal_project().await.unwrap();
    a.create_task(&proj, &task("Acme kickoff meeting"))
        .await
        .unwrap();

    // The literal question is not a substring of either title, but its keyword
    // "acme" is — keyword retrieval finds both.
    let q = "what do I have about the Acme proposal?";
    assert!(
        a.workspace_search(q, 20).await.unwrap().is_empty(),
        "literal search misses"
    );
    assert_eq!(
        a.workspace_search_terms(q, 20).await.unwrap().len(),
        2,
        "keyword search finds both"
    );

    // Still access-scoped: another user and another tenant get nothing.
    assert!(b.workspace_search_terms(q, 20).await.unwrap().is_empty());
    let t2 = store.create_tenant("kw-t2").await.unwrap();
    let ud = store
        .for_tenant(t2.clone())
        .create_user("d@kw.test")
        .await
        .unwrap();
    let d = store.for_account(t2, ud);
    assert!(d.workspace_search_terms(q, 20).await.unwrap().is_empty());

    // An all-stopword question falls back cleanly (no keywords → no crash).
    assert!(
        a.workspace_search_terms("what is this?", 20)
            .await
            .unwrap()
            .is_empty()
    );
}

/// The public serving door (ADR 0036): `SitePublicStore` resolves a subdomain
/// to exactly its own tenant's current publish — unknown, unpublished, and
/// unpublished-again subdomains are indistinguishable `None`; drafts edited
/// after a publish never surface until a republish; and the pages read is
/// scoped by the resolved value, so one host's lookup can never return
/// another site's content (the store half of the Host-isolation guarantee).
#[tokio::test]
async fn public_resolver_scopes_by_subdomain_and_never_leaks_drafts() {
    use alo_store::SitePublicStore;
    use serde_json::json;

    let store = common::test_store().await;
    let public = SitePublicStore::new(
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&common::database_url())
            .await
            .expect("connect public pool"),
        alo_store::BlobStore::in_memory(1024 * 1024),
    );

    let t1 = store.create_tenant("sp-t1").await.unwrap();
    let ua = store
        .for_tenant(t1.clone())
        .create_user("a@sitepublic.test")
        .await
        .unwrap();
    let a = store.for_account(t1, ua);
    let t2 = store.create_tenant("sp-t2").await.unwrap();
    let ub = store
        .for_tenant(t2.clone())
        .create_user("b@sitepublic.test")
        .await
        .unwrap();
    let b = store.for_account(t2, ub);

    // Unique per run: the compose Postgres is shared and subdomains are global.
    let unique = |tag: &str| {
        format!(
            "{tag}-{}x",
            alo_store::SiteId::generate()
                .as_str()
                .to_lowercase()
                .replace('_', "-")
        )
    };
    let sub_a = unique("pub-a");
    let sub_b = unique("pub-b");

    // Unknown subdomain and a created-but-never-published site read the same.
    assert!(
        public
            .resolve_published("no-such-site")
            .await
            .unwrap()
            .is_none()
    );
    let site_a = a.create_site("Alpha Works", &sub_a).await.unwrap();
    let home_a = a.create_site_page(&site_a, "Home", "", true).await.unwrap();
    let hero_a = json!({
        "schema_version": 1,
        "sections": [{"type": "hero", "heading": "ALPHA-MARKER"}]
    });
    a.set_page_sections(&site_a, &home_a, hero_a.clone())
        .await
        .unwrap();
    assert!(public.resolve_published(&sub_a).await.unwrap().is_none());

    // Publishing makes the resolver see exactly the frozen set.
    let terra = json!({"schema_version": 1, "preset": "terra"});
    a.set_site_theme(&site_a, terra.clone()).await.unwrap();
    let p1 = a.publish_site(&site_a).await.unwrap();
    let resolved = public.resolve_published(&sub_a).await.unwrap().unwrap();
    assert_eq!(resolved.site, site_a);
    assert_eq!(resolved.name, "Alpha Works");
    assert_eq!(resolved.publish, p1);
    assert_eq!(resolved.theme, terra);
    let pages = public.published_pages(&resolved).await.unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].slug, "");
    assert!(pages[0].is_home);
    assert_eq!(pages[0].sections, hero_a);

    // A second tenant's live site: each subdomain resolves to its own tenant's
    // content only, and neither pages read contains the other's marker.
    let site_b = b.create_site("Beta Books", &sub_b).await.unwrap();
    let home_b = b.create_site_page(&site_b, "Home", "", true).await.unwrap();
    b.set_page_sections(
        &site_b,
        &home_b,
        json!({
            "schema_version": 1,
            "sections": [{"type": "hero", "heading": "BETA-MARKER"}]
        }),
    )
    .await
    .unwrap();
    b.publish_site(&site_b).await.unwrap();
    let resolved_a = public.resolve_published(&sub_a).await.unwrap().unwrap();
    let resolved_b = public.resolve_published(&sub_b).await.unwrap().unwrap();
    assert_eq!(resolved_a.name, "Alpha Works");
    assert_eq!(resolved_b.name, "Beta Books");
    let body_a = format!("{:?}", public.published_pages(&resolved_a).await.unwrap());
    let body_b = format!("{:?}", public.published_pages(&resolved_b).await.unwrap());
    assert!(body_a.contains("ALPHA-MARKER") && !body_a.contains("BETA-MARKER"));
    assert!(body_b.contains("BETA-MARKER") && !body_b.contains("ALPHA-MARKER"));

    // Draft edits after the publish never surface through the public door
    // until a republish flips the pointer to a new frozen set.
    a.set_page_sections(
        &site_a,
        &home_a,
        json!({
            "schema_version": 1,
            "sections": [{"type": "hero", "heading": "DRAFT-ONLY"}]
        }),
    )
    .await
    .unwrap();
    let still = public.resolve_published(&sub_a).await.unwrap().unwrap();
    assert_eq!(still.publish, p1);
    let frozen = format!("{:?}", public.published_pages(&still).await.unwrap());
    assert!(frozen.contains("ALPHA-MARKER") && !frozen.contains("DRAFT-ONLY"));
    let p2 = a.publish_site(&site_a).await.unwrap();
    let republished = public.resolve_published(&sub_a).await.unwrap().unwrap();
    assert_eq!(republished.publish, p2);
    let fresh = format!("{:?}", public.published_pages(&republished).await.unwrap());
    assert!(fresh.contains("DRAFT-ONLY") && !fresh.contains("ALPHA-MARKER"));

    // Unpublishing takes the subdomain back to the indistinguishable None.
    a.unpublish_site(&site_a).await.unwrap();
    assert!(public.resolve_published(&sub_a).await.unwrap().is_none());
    assert!(public.resolve_published(&sub_b).await.unwrap().is_some());
}

/// Site image reads (S1.14): `AccountStore::site_image` serves a tenant's
/// own image blobs only — a foreign tenant's blob id reads as absent, and a
/// non-image content type is never served on an image path. The public
/// door's `published_image` is scoped the same way through the resolved
/// site's private tenant, so a Host lookup can never lead to another
/// tenant's bytes even with a known blob id.
#[tokio::test]
async fn site_images_scope_by_tenant_and_refuse_non_images() {
    use alo_store::SitePublicStore;
    use bytes::Bytes;
    use serde_json::json;

    let (store, blobs) = common::test_store_with_blobs().await;
    let public = SitePublicStore::new(
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&common::database_url())
            .await
            .expect("connect public pool"),
        blobs,
    );

    let t1 = store.create_tenant("simg-t1").await.unwrap();
    let ua = store
        .for_tenant(t1.clone())
        .create_user("a@siteimage.test")
        .await
        .unwrap();
    let a = store.for_account(t1, ua);
    let t2 = store.create_tenant("simg-t2").await.unwrap();
    let ub = store
        .for_tenant(t2.clone())
        .create_user("b@siteimage.test")
        .await
        .unwrap();
    let b = store.for_account(t2, ub);

    // Tenant A uploads a logo (image) and an HTML blob (never an image).
    let png = a
        .put_blob(Bytes::from_static(b"png-bytes-alpha"), Some("image/png"))
        .await
        .unwrap();
    let html = a
        .put_blob(
            Bytes::from_static(b"<script>alert(1)</script>"),
            Some("text/html"),
        )
        .await
        .unwrap();

    // Own tenant: the image serves with its allowlisted type; the HTML blob
    // is indistinguishable from absent on the image path.
    let served = a.site_image(&png).await.unwrap().expect("own image serves");
    assert_eq!(served.content_type, "image/png");
    assert_eq!(&served.bytes[..], b"png-bytes-alpha");
    assert!(a.site_image(&html).await.unwrap().is_none());

    // Wrong tenant: A's blob id resolves to nothing for B — clean absence,
    // not an error and not bytes.
    assert!(b.site_image(&png).await.unwrap().is_none());

    // The public door: a live site of A's serves A's image through the
    // resolved-site scope; the same id through B's resolved site is absent.
    let unique = |tag: &str| {
        format!(
            "{tag}-{}x",
            alo_store::SiteId::generate()
                .as_str()
                .to_lowercase()
                .replace('_', "-")
        )
    };
    let sub_a = unique("img-a");
    let sub_b = unique("img-b");
    let site_a = a.create_site("Alpha Studio", &sub_a).await.unwrap();
    a.create_site_page(&site_a, "Home", "", true).await.unwrap();
    a.set_site_theme(
        &site_a,
        json!({"schema_version": 1, "preset": "north", "logo": png.as_str()}),
    )
    .await
    .unwrap();
    a.publish_site(&site_a).await.unwrap();
    let site_b = b.create_site("Beta Studio", &sub_b).await.unwrap();
    b.create_site_page(&site_b, "Home", "", true).await.unwrap();
    b.publish_site(&site_b).await.unwrap();

    let resolved_a = public.resolve_published(&sub_a).await.unwrap().unwrap();
    let resolved_b = public.resolve_published(&sub_b).await.unwrap().unwrap();
    let public_img = public
        .published_image(&resolved_a, png.as_str())
        .await
        .unwrap()
        .expect("published image serves");
    assert_eq!(public_img.content_type, "image/png");
    assert_eq!(&public_img.bytes[..], b"png-bytes-alpha");
    assert!(
        public
            .published_image(&resolved_b, png.as_str())
            .await
            .unwrap()
            .is_none(),
        "another tenant's resolved site must never reach the blob"
    );
    assert!(
        public
            .published_image(&resolved_a, html.as_str())
            .await
            .unwrap()
            .is_none(),
        "non-image content types never serve on the image path"
    );
}

/// alo Chat (ADR 0038): **membership is the permission**, and no room is ever
/// reachable from another tenant. Walks the whole visibility ladder — public,
/// private, DM — plus the owner-only room controls and DM idempotency.
#[tokio::test]
async fn chat_rooms_are_membership_scoped_and_never_leave_their_tenant() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("chat-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@chat.test").await.unwrap();
    let uc = ts1.create_user("c@chat.test").await.unwrap();
    let a = store.for_account(t1.clone(), ua.clone());
    let c = store.for_account(t1, uc.clone());

    let t2 = store.create_tenant("chat-t2").await.unwrap();
    let ub = store
        .for_tenant(t2.clone())
        .create_user("b@chat.test")
        .await
        .unwrap();
    let b = store.for_account(t2, ub.clone());

    let private = a
        .create_channel("plans", Some("what we ship"), ChannelVisibility::Private)
        .await
        .unwrap();
    let public = a
        .create_channel("general", None, ChannelVisibility::Public)
        .await
        .unwrap();

    // The other tenant sees nothing on any path, and can change nothing.
    assert_not_found(b.channel(&private).await);
    assert_not_found(b.channel(&public).await);
    assert_not_found(b.channel_members(&public).await);
    assert_not_found(b.join_channel(&public).await);
    assert_not_found(b.add_member(&public, &ub).await);
    assert_not_found(b.archive_channel(&public).await);
    assert!(b.channels().await.unwrap().is_empty());
    assert!(b.joinable_channels().await.unwrap().is_empty());
    // Names are per tenant: the same `#plans` is free next door.
    b.create_channel("plans", None, ChannelVisibility::Public)
        .await
        .unwrap();

    // A co-tenant who is not a member: the private room does not exist for
    // them; the public one does, and is joinable.
    assert_not_found(c.channel(&private).await);
    assert_not_found(c.channel_members(&private).await);
    assert_not_found(c.join_channel(&private).await);
    assert_eq!(
        c.channel(&public).await.unwrap().id.as_str(),
        public.as_str()
    );
    assert_eq!(c.joinable_channels().await.unwrap().len(), 1);
    assert!(c.channels().await.unwrap().is_empty());

    // Joining is idempotent and lands a plain member.
    c.join_channel(&public).await.unwrap();
    c.join_channel(&public).await.unwrap();
    assert_eq!(c.channels().await.unwrap().len(), 1);
    assert!(c.joinable_channels().await.unwrap().is_empty());
    assert_eq!(
        c.channel_role(&public).await.unwrap(),
        Some(MemberRole::Member)
    );
    assert_eq!(a.channel_members(&public).await.unwrap().len(), 2);

    // A plain member may not rename or archive the room; its owner may.
    assert_forbidden(c.rename_channel(&public, Some("lounge"), None).await);
    assert_forbidden(c.archive_channel(&public).await);
    a.rename_channel(&public, Some("lounge"), Some("chat"))
        .await
        .unwrap();
    assert_eq!(
        a.channel(&public).await.unwrap().name.as_deref(),
        Some("lounge")
    );

    // A live name is unique inside the tenant.
    assert!(
        a.create_channel("plans", None, ChannelVisibility::Public)
            .await
            .is_err(),
        "a live channel name is claimed once per tenant"
    );

    // Anyone may leave; only an owner may remove someone else.
    c.remove_member(&public, &uc).await.unwrap();
    assert!(c.channels().await.unwrap().is_empty());
    a.add_member(&public, &uc).await.unwrap();
    assert_eq!(a.channel_members(&public).await.unwrap().len(), 2);
    a.remove_member(&public, &uc).await.unwrap();
    assert_eq!(a.channel_members(&public).await.unwrap().len(), 1);
    // Someone from another tenant can never be added.
    assert_not_found(a.add_member(&public, &ub).await);

    // A DM is one room from either side, however often it is opened.
    let dm = a.open_dm(&uc).await.unwrap();
    assert_eq!(a.open_dm(&uc).await.unwrap().as_str(), dm.as_str());
    assert_eq!(c.open_dm(&ua).await.unwrap().as_str(), dm.as_str());
    assert_not_found(b.channel(&dm).await);
    let a_dm = a
        .channel_summaries()
        .await
        .unwrap()
        .into_iter()
        .find(|summary| summary.channel.id == dm)
        .unwrap();
    let c_dm = c
        .channel_summaries()
        .await
        .unwrap()
        .into_iter()
        .find(|summary| summary.channel.id == dm)
        .unwrap();
    assert_eq!(a_dm.counterpart.as_deref(), Some("c@chat.test"));
    assert_eq!(c_dm.counterpart.as_deref(), Some("a@chat.test"));
    assert_ne!(a_dm.counterpart.as_deref(), Some("a@chat.test"));
    assert_ne!(c_dm.counterpart.as_deref(), Some("c@chat.test"));
    // ...and it keeps exactly its two people, with no name to change.
    assert!(a.add_member(&dm, &uc).await.is_err());
    assert!(a.remove_member(&dm, &uc).await.is_err());
    assert!(a.rename_channel(&dm, Some("us"), None).await.is_err());
    assert!(a.open_dm(&ua).await.is_err(), "no DM with oneself");
    assert_not_found(a.open_dm(&ub).await);

    // Archiving keeps history for members, hides the room from everyone else,
    // and frees the name.
    a.archive_channel(&public).await.unwrap();
    assert!(a.channel(&public).await.unwrap().archived_at.is_some());
    assert_not_found(c.channel(&public).await);
    a.create_channel("lounge", None, ChannelVisibility::Public)
        .await
        .unwrap();
}

/// alo Chat phase 3: what is said in a room stays in it. Proves the sequence
/// is the room's own clock, that reading a public room never makes you a
/// participant in it, and that read state can move neither backwards nor past
/// the end.
#[tokio::test]
async fn chat_messages_are_room_scoped_and_ordered_by_their_own_sequence() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("chatmsg-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@chatmsg.test").await.unwrap();
    let uc = ts1.create_user("c@chatmsg.test").await.unwrap();
    let a = store.for_account(t1.clone(), ua.clone());
    let c = store.for_account(t1, uc.clone());
    let t2 = store.create_tenant("chatmsg-t2").await.unwrap();
    let ub = store
        .for_tenant(t2.clone())
        .create_user("b@chatmsg.test")
        .await
        .unwrap();
    let b = store.for_account(t2, ub);

    let room = a
        .create_channel("standup", None, ChannelVisibility::Public)
        .await
        .unwrap();
    let other = a
        .create_channel("design", None, ChannelVisibility::Private)
        .await
        .unwrap();

    // The sequence is per room and starts at 1.
    let first = a.post_message(&room, "morning", None).await.unwrap();
    let second = a
        .post_message(&room, "two things today", None)
        .await
        .unwrap();
    assert_eq!((first.seq, second.seq), (1, 2));
    assert_eq!(
        a.post_message(&other, "elsewhere", None).await.unwrap().seq,
        1
    );

    // Another tenant cannot see, post, or address any of it.
    assert_not_found(b.messages(&room, None, 50).await);
    assert_not_found(b.post_message(&room, "hello?", None).await);
    assert_not_found(b.chat_message(&first.id).await);
    assert_not_found(b.mark_read(&room, 1).await);

    // A co-tenant may READ a live public room without being in it — but
    // reading is not joining: posting still needs membership.
    assert_eq!(c.messages(&room, None, 50).await.unwrap().len(), 2);
    assert_not_found(c.post_message(&room, "may I?", None).await);
    assert_not_found(c.messages(&other, None, 50).await);
    // ...and a message of a room they cannot see is not theirs to address.
    let hidden = a.post_message(&other, "private note", None).await.unwrap();
    assert_not_found(c.chat_message(&hidden.id).await);
    assert_eq!(c.chat_message(&first.id).await.unwrap().seq, 1);

    // History is newest-first and walks back by cursor.
    for n in 3..=6 {
        a.post_message(&room, &format!("line {n}"), None)
            .await
            .unwrap();
    }
    let page = a.messages(&room, None, 3).await.unwrap();
    assert_eq!(
        page.iter().map(|m| m.message.seq).collect::<Vec<_>>(),
        vec![6, 5, 4]
    );
    let older = a.messages(&room, Some(4), 3).await.unwrap();
    assert_eq!(
        older.iter().map(|m| m.message.seq).collect::<Vec<_>>(),
        vec![3, 2, 1]
    );

    // A reply hangs under a root; a thread never grows a thread.
    let reply = a
        .post_message(&room, "on that", Some(first.seq))
        .await
        .unwrap();
    assert_eq!(reply.thread_root_seq, Some(1));
    assert_eq!(a.thread_replies(&room, first.seq).await.unwrap().len(), 1);

    // ...and it leaves the main feed for it. A reply read twice — once in its
    // thread and once in the room's spine — is the failure threaded chat is
    // judged on. The feed keeps only the count, so the thread still announces
    // itself.
    let feed = a.messages(&room, None, 50).await.unwrap();
    assert!(
        !feed.iter().any(|m| m.message.seq == reply.seq),
        "a reply belongs to its thread, not to the feed"
    );
    let root = feed
        .iter()
        .find(|m| m.message.seq == first.seq)
        .expect("the root stays in the feed");
    assert_eq!(root.reply_count, 1);
    assert!(root.last_reply_at.is_some());
    assert_eq!(
        feed.iter().filter(|m| m.reply_count > 0).count(),
        1,
        "only the one message that was replied to"
    );
    assert!(
        a.post_message(&room, "nested", Some(reply.seq))
            .await
            .is_err()
    );
    assert!(a.post_message(&room, "ghost", Some(9_999)).await.is_err());

    // Edits and withdrawals are the author's alone, and the sequence survives
    // a withdrawal as a tombstone with no words left in it.
    c.join_channel(&room).await.unwrap();
    assert_forbidden(c.edit_message(&second.id, "not mine").await);
    assert_forbidden(c.delete_message(&second.id).await);
    let edited = a
        .edit_message(&second.id, "three things today")
        .await
        .unwrap();
    assert_eq!(edited.body, "three things today");
    assert!(edited.edited_at.is_some());
    assert_eq!(edited.seq, second.seq);
    a.delete_message(&second.id).await.unwrap();
    let gone = a.chat_message(&second.id).await.unwrap();
    assert!(gone.deleted_at.is_some());
    assert!(gone.body.is_empty(), "a withdrawn message keeps no words");
    assert!(a.edit_message(&second.id, "back again").await.is_err());
    a.delete_message(&second.id).await.unwrap(); // twice is not an error

    // Read state: mine never counts, it never moves backwards, and it cannot
    // run past what the room has actually said.
    let mine = a
        .channel_summaries()
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.channel.id.as_str() == room.as_str())
        .unwrap();
    assert_eq!(mine.unread, 0, "my own messages are never unread to me");
    assert_eq!(mine.last_seq, Some(7));

    let theirs = |summaries: Vec<alo_store::ChatChannelSummary>| {
        summaries
            .into_iter()
            .find(|s| s.channel.id.as_str() == room.as_str())
            .unwrap()
    };
    let before = theirs(c.channel_summaries().await.unwrap());
    assert_eq!(before.last_read_seq, 0);
    assert_eq!(before.unread, 6, "seven said, one withdrawn");

    c.mark_read(&room, 4).await.unwrap();
    assert_eq!(theirs(c.channel_summaries().await.unwrap()).unread, 3);
    let receipts = a.messages(&room, None, 50).await.unwrap();
    assert!(
        receipts
            .iter()
            .filter(|line| line.message.author == ua)
            .filter(|line| line.message.seq <= 4)
            .all(|line| line.read_by == 1),
        "another member reading through a message produces a real receipt"
    );
    assert!(
        receipts
            .iter()
            .filter(|line| line.message.author == ua)
            .filter(|line| line.message.seq > 4)
            .all(|line| line.read_by == 0),
        "messages beyond the other member's cursor remain merely sent"
    );
    c.mark_read(&room, 2).await.unwrap();
    assert_eq!(
        theirs(c.channel_summaries().await.unwrap()).last_read_seq,
        4,
        "a read cursor never moves backwards"
    );
    c.mark_read(&room, 9_999).await.unwrap();
    assert_eq!(
        theirs(c.channel_summaries().await.unwrap()).last_read_seq,
        7,
        "a read cursor never runs past the end"
    );
    assert_eq!(theirs(c.channel_summaries().await.unwrap()).unread, 0);

    // An archived room keeps its history and takes no new words.
    a.archive_channel(&room).await.unwrap();
    assert!(a.post_message(&room, "after the end", None).await.is_err());
    assert_eq!(
        a.messages(&room, None, 50).await.unwrap().len(),
        6,
        "seven said, one of them a reply that lives in its thread"
    );
}

/// The batch directory lookup must label only the caller's own tenant. A chat
/// feed asks it to name every author on a page, so if it ever answered for a
/// foreign id it would turn a rendering helper into a cross-tenant probe: send
/// a guessed id, learn from the answer whether that person exists elsewhere.
#[tokio::test]
async fn batch_email_lookup_never_names_a_user_from_another_tenant() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("dir-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@dir.test").await.unwrap();
    let ub = ts1.create_user("b@dir.test").await.unwrap();

    let t2 = store.create_tenant("dir-t2").await.unwrap();
    let foreign = store
        .for_tenant(t2)
        .create_user("stranger@dir.test")
        .await
        .unwrap();

    // Its own people, in one call, in any order and with duplicates.
    let found = ts1
        .emails_of(&[ub.clone(), ua.clone(), ua.clone()])
        .await
        .unwrap();
    assert_eq!(found.len(), 2, "two distinct users, deduped");
    assert_eq!(
        found.get(ua.as_str()).map(String::as_str),
        Some("a@dir.test")
    );
    assert_eq!(
        found.get(ub.as_str()).map(String::as_str),
        Some("b@dir.test")
    );

    // A foreign id is absent — not an error, and not an address. Absence is
    // the same answer an id that never existed gets, so nothing is disclosed.
    let probed = ts1
        .emails_of(&[
            foreign.clone(),
            alo_store::UserId::new("no-such-user".to_owned()),
        ])
        .await
        .unwrap();
    assert!(probed.is_empty(), "no foreign user, no invented user");

    // And the mirror: tenant 2 cannot name tenant 1's people either.
    let back = store.for_tenant(t1).emails_of(&[foreign]).await.unwrap();
    assert!(back.is_empty());

    // Asking about nobody is a valid, cheap question.
    assert!(ts1.emails_of(&[]).await.unwrap().is_empty());
}

/// A thread's count follows what is still standing in it. Its own room, so the
/// arithmetic of the larger message test stays undisturbed.
#[tokio::test]
async fn a_withdrawn_reply_stops_being_counted_but_keeps_its_place() {
    let store = common::test_store().await;
    let t = store.create_tenant("thread-count").await.unwrap();
    let ua = store
        .for_tenant(t.clone())
        .create_user("a@thread.test")
        .await
        .unwrap();
    let a = store.for_account(t, ua);

    let room = a
        .create_channel("threads", None, ChannelVisibility::Public)
        .await
        .unwrap();
    let root = a.post_message(&room, "the question", None).await.unwrap();
    let one = a
        .post_message(&room, "an answer", Some(root.seq))
        .await
        .unwrap();
    a.post_message(&room, "another", Some(root.seq))
        .await
        .unwrap();

    let counted = |feed: Vec<alo_store::ChatFeedMessage>| {
        feed.into_iter()
            .find(|m| m.message.seq == root.seq)
            .expect("the root stays in the feed")
    };

    let before = counted(a.messages(&room, None, 50).await.unwrap());
    assert_eq!(before.reply_count, 2);
    assert!(before.last_reply_at.is_some());

    a.delete_message(&one.id).await.unwrap();
    assert_eq!(
        counted(a.messages(&room, None, 50).await.unwrap()).reply_count,
        1
    );

    // The tombstone stays in the thread — the sequence never gains a hole —
    // it simply stops being advertised on the feed.
    assert_eq!(a.thread_replies(&room, root.seq).await.unwrap().len(), 2);
}

/// Reactions carry the same rules as the words they hang on: a room you cannot
/// see has no reactions to leave, reading a public room does not let you react
/// in it, and the toggle is the primary key's job rather than the caller's.
#[tokio::test]
async fn reactions_follow_the_room_and_toggle_exactly_once() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("react-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("a@react.test").await.unwrap();
    let uc = ts1.create_user("c@react.test").await.unwrap();
    let a = store.for_account(t1.clone(), ua.clone());
    let c = store.for_account(t1.clone(), uc.clone());

    let t2 = store.create_tenant("react-t2").await.unwrap();
    let ub = store
        .for_tenant(t2.clone())
        .create_user("b@react.test")
        .await
        .unwrap();
    let b = store.for_account(t2, ub);

    let room = a
        .create_channel("standup", None, ChannelVisibility::Public)
        .await
        .unwrap();
    let said = a.post_message(&room, "shipping today", None).await.unwrap();

    // Another tenant cannot react to it, tally it, or learn it exists.
    assert_not_found(b.toggle_reaction(&said.id, "👍").await);
    assert_not_found(b.message_reactions(&said.id).await);

    // A co-tenant may READ this public room, but reacting is contributing —
    // it needs membership, exactly as posting does.
    assert_eq!(c.message_reactions(&said.id).await.unwrap().len(), 0);
    assert_not_found(c.toggle_reaction(&said.id, "👍").await);

    // Only what the room offers.
    assert!(a.toggle_reaction(&said.id, "🚀").await.is_err());
    assert!(a.toggle_reaction(&said.id, "not an emoji").await.is_err());

    // On, off, on — and never two of the same from one person.
    assert!(a.toggle_reaction(&said.id, "👍").await.unwrap());
    assert!(!a.toggle_reaction(&said.id, "👍").await.unwrap());
    assert!(a.toggle_reaction(&said.id, "👍").await.unwrap());
    let tally = a.message_reactions(&said.id).await.unwrap();
    assert_eq!(tally.len(), 1);
    assert_eq!(
        (tally[0].emoji.as_str(), tally[0].count, tally[0].mine),
        ("👍", 1, true)
    );

    // A second person on the same emoji is a count, not a duplicate — and it
    // is "mine" only to the person who left it.
    c.join_channel(&room).await.unwrap();
    assert!(c.toggle_reaction(&said.id, "👍").await.unwrap());
    assert!(c.toggle_reaction(&said.id, "🎉").await.unwrap());
    let seen = c.message_reactions(&said.id).await.unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!((seen[0].emoji.as_str(), seen[0].count), ("👍", 2));
    assert_eq!((seen[1].emoji.as_str(), seen[1].count), ("🎉", 1));

    let mine_only = a.message_reactions(&said.id).await.unwrap();
    let party = mine_only.iter().find(|r| r.emoji == "🎉").unwrap();
    assert!(!party.mine, "someone else's reaction is not mine");

    // Who left it, in the order they did.
    let who = a.reaction_users(&said.id, "👍").await.unwrap();
    assert_eq!(
        who.iter()
            .map(alo_store::UserId::as_str)
            .collect::<Vec<_>>(),
        vec![ua.as_str(), uc.as_str()]
    );

    // A page is tallied in one pass, and a message with none is simply absent.
    let quiet = a.post_message(&room, "nothing to see", None).await.unwrap();
    let page = a
        .reactions_for_channel(&room, &[said.id.clone(), quiet.id.clone()])
        .await
        .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(said.id.as_str()).unwrap().len(), 2);
    assert!(!page.contains_key(quiet.id.as_str()));

    // Withdrawn words take no new reactions, and an archived room takes none
    // at all — the same two refusals posting gives.
    a.delete_message(&quiet.id).await.unwrap();
    assert!(a.toggle_reaction(&quiet.id, "👍").await.is_err());
    a.archive_channel(&room).await.unwrap();
    assert!(a.toggle_reaction(&said.id, "❤️").await.is_err());
    // ...but what was already left stays readable.
    assert_eq!(a.message_reactions(&said.id).await.unwrap().len(), 2);
}

/// Mentions are resolved against the room's members at post time. A handle
/// naming someone who is not in the room resolves to nobody — a badge that
/// pointed at a door they have no key to would be worse than no badge.
#[tokio::test]
async fn a_mention_reaches_only_someone_already_in_the_room() {
    let store = common::test_store().await;
    let t = store.create_tenant("mention-t").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@mention.test").await.unwrap();
    let ub = ts.create_user("ben@mention.test").await.unwrap();
    let uc = ts.create_user("outsider@mention.test").await.unwrap();
    let a = store.for_account(t.clone(), ua.clone());
    let b = store.for_account(t.clone(), ub.clone());
    let c = store.for_account(t, uc.clone());

    let room = a
        .create_channel("plans", None, ChannelVisibility::Public)
        .await
        .unwrap();
    a.add_member(&room, &ub).await.unwrap();

    // A member is reached; a co-tenant who is not in the room is not, even
    // though the address is perfectly real.
    let m = a
        .post_message(&room, "@ben and @outsider, thoughts?", None)
        .await
        .unwrap();
    let named = a
        .mentions_for_channel(&room, std::slice::from_ref(&m.id))
        .await
        .unwrap();
    assert_eq!(
        named
            .get(m.id.as_str())
            .map(|u| u.iter().map(alo_store::UserId::as_str).collect::<Vec<_>>()),
        Some(vec![ub.as_str()]),
        "only the member is named"
    );

    // It badges the person named, and nobody else.
    assert_eq!(
        b.unread_mentions().await.unwrap().get(room.as_str()),
        Some(&1)
    );
    assert!(c.unread_mentions().await.unwrap().is_empty());
    // Not the author, even when they write their own handle.
    a.post_message(&room, "note to @anna: ship it", None)
        .await
        .unwrap();
    assert!(a.unread_mentions().await.unwrap().is_empty());

    // Reading the room clears the badge; it is the same cursor everything
    // else uses.
    b.mark_read(&room, 2).await.unwrap();
    assert!(b.unread_mentions().await.unwrap().is_empty());

    // Editing re-derives: a name added afterwards still reaches its person,
    // and a name edited out stops badging them.
    let later = a.post_message(&room, "nothing here", None).await.unwrap();
    assert!(b.unread_mentions().await.unwrap().is_empty());
    a.edit_message(&later.id, "actually @ben, look at this")
        .await
        .unwrap();
    assert_eq!(
        b.unread_mentions().await.unwrap().get(room.as_str()),
        Some(&1)
    );
    a.edit_message(&later.id, "never mind").await.unwrap();
    assert!(b.unread_mentions().await.unwrap().is_empty());

    // Withdrawing takes the mention with the words.
    a.edit_message(&later.id, "@ben one more time")
        .await
        .unwrap();
    assert_eq!(
        b.unread_mentions().await.unwrap().get(room.as_str()),
        Some(&1)
    );
    a.delete_message(&later.id).await.unwrap();
    assert!(
        b.unread_mentions().await.unwrap().is_empty(),
        "a badge must not point at an empty tombstone"
    );
}

/// A shared file is a pointer into Drive, and Drive keeps deciding who may see
/// it. You may only share what you can already open, and a pointer that stops
/// resolving stops being shown — including its name, which is the part a
/// write-time-only check would leave on display.
#[tokio::test]
async fn a_shared_file_is_a_pointer_that_drive_keeps_deciding_about() {
    use alo_store::{DriveLocation, NewDriveFile};
    use bytes::Bytes;

    let store = common::test_store().await;
    let t1 = store.create_tenant("attach-t1").await.unwrap();
    let ts1 = store.for_tenant(t1.clone());
    let ua = ts1.create_user("anna@attach.test").await.unwrap();
    let ub = ts1.create_user("ben@attach.test").await.unwrap();
    let a = store.for_account(t1.clone(), ua);
    let b = store.for_account(t1, ub.clone());

    let t2 = store.create_tenant("attach-t2").await.unwrap();
    let uc = store
        .for_tenant(t2.clone())
        .create_user("stranger@attach.test")
        .await
        .unwrap();
    let c = store.for_account(t2, uc);

    // A file in Anna's own Drive, and one in the other tenant's.
    let blob = a
        .put_blob(
            Bytes::from_static(b"the quarterly plan"),
            Some("text/plain"),
        )
        .await
        .unwrap();
    let mine = a
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: "plan.txt".to_owned(),
                blob_id: blob.as_str().to_owned(),
                size: 18,
                content_type: Some("text/plain".to_owned()),
                ..NewDriveFile::default()
            },
        )
        .await
        .unwrap();

    let far_blob = c
        .put_blob(Bytes::from_static(b"not yours"), Some("text/plain"))
        .await
        .unwrap();
    let theirs = c
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: "secret.txt".to_owned(),
                blob_id: far_blob.as_str().to_owned(),
                size: 9,
                content_type: Some("text/plain".to_owned()),
                ..NewDriveFile::default()
            },
        )
        .await
        .unwrap();

    let room = a
        .create_channel("planning", None, ChannelVisibility::Public)
        .await
        .unwrap();
    a.add_member(&room, &ub).await.unwrap();
    let said = a.post_message(&room, "here it is", None).await.unwrap();

    // You cannot share a file you cannot open — and the refusal is the same
    // "not found" a missing file gets, so it tells nothing either way.
    assert_not_found(
        a.attach_files(&said.id, std::slice::from_ref(&theirs))
            .await,
    );
    assert_not_found(
        a.attach_files(
            &said.id,
            &[alo_store::DriveNodeId::new("no-such".to_owned())],
        )
        .await,
    );

    // Your own file attaches, once, in the order given.
    let kept = a
        .attach_files(&said.id, &[mine.clone(), mine.clone()])
        .await
        .unwrap();
    assert_eq!(kept.len(), 1, "the same file twice is one attachment");

    // Anna sees it with Drive's current name and size.
    let seen = a.message_attachments(&said.id).await.unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].name, "plan.txt");
    assert_eq!(seen[0].size, 18);
    assert!(!seen[0].trashed);

    // Ben is in the room and can read the message — but the file lives in
    // ANNA'S personal Drive, which he was never given. The pointer resolves to
    // nothing for him, so he sees the words without the filename.
    let bens = b.message_attachments(&said.id).await.unwrap();
    assert!(
        bens.is_empty(),
        "a room must not disclose the name of a file the reader cannot open"
    );

    // A renamed file shows its new name: the name is read live, never stored.
    a.drive_rename(&mine, "plan-v2.txt").await.unwrap();
    let after = a.message_attachments(&said.id).await.unwrap();
    assert_eq!(after[0].name, "plan-v2.txt");

    // Another tenant reaches none of it.
    assert_not_found(c.message_attachments(&said.id).await);

    // Past the ceiling is refused rather than silently truncated.
    let many: Vec<_> = (0..alo_store::ATTACHMENTS_MAX + 1)
        .map(|_| mine.clone())
        .collect();
    assert!(a.attach_files(&said.id, &many).await.is_err());
}
