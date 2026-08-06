//! The isolation suite (mandatory, CI-gated). For **every** public read
//! and write path on the account door, an id belonging to another
//! account — whether in another tenant or another user of the *same*
//! tenant — must get a clean `NotFound`/empty: never the other account's
//! data, never an unexpected error (a "500"). The boundary is the type
//! you hold ([`AccountStore`]), enforced by construction: there is no
//! ownership guard in any call path to omit. Runs against the real
//! Postgres from compose.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    AccountStore, BlobId, MailboxId, Message, MessageId, Page, SEEN, StoreError, ThreadId,
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
    NewTask { title: title.to_owned(), ..Default::default() }
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
    let edit = TaskEdit { title: "hijack".to_owned(), priority: "none".to_owned(), ..Default::default() };
    assert_not_found(b.update_task(&private, &edit).await);
    assert_not_found(b.move_task(&private, "done", 1.0).await);
    assert_not_found(b.delete_task(&private).await);
    assert_eq!(a.task(&private).await.unwrap().unwrap().title, "private"); // intact

    // Cross-tenant: an outsider sees nothing and can touch nothing.
    let t2 = store.create_tenant("task-t2").await.unwrap();
    let ud = store.for_tenant(t2.clone()).create_user("d@task.test").await.unwrap();
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
        &NewTask { title: "maybe".to_owned(), state: Some("proposed".to_owned()), ..Default::default() },
    )
    .await
    .unwrap();
    assert_eq!(a.task_proposals().await.unwrap().len(), 1);
    assert_eq!(d.task_proposals().await.unwrap().len(), 0, "proposals don't cross tenants");
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
    let ua = store.for_tenant(t1.clone()).create_user("a@mailtask.test").await.unwrap();
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
    let ux = store.for_tenant(t2.clone()).create_user("x@mailtask.test").await.unwrap();
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
    a.add_task_attachment(&private, "blob-1", "secret.pdf", 100).await.unwrap();
    let team = a.create_task_project("Team", None).await.unwrap();
    let shared = a.create_task(&team, &task("shared")).await.unwrap();
    a.add_task_attachment(&shared, "blob-2", "brief.pdf", 200).await.unwrap();

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
    let ud = store.for_tenant(t2.clone()).create_user("d@attach.test").await.unwrap();
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
    assert_eq!(a.task_followers(&shared).await.unwrap().len(), 1, "creator auto-follows");

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
    let ud = store.for_tenant(t2.clone()).create_user("d@follow.test").await.unwrap();
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
    assert_eq!(a.dependencies(&second).await.unwrap().len(), 1, "second is blocked by first");
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
    let ux = store.for_tenant(t2.clone()).create_user("x@dep.test").await.unwrap();
    let x = store.for_account(t2, ux);
    let x_task = x.create_task(&x.ensure_personal_project().await.unwrap(), &task("x")).await.unwrap();
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
    let ua = store.for_tenant(t1.clone()).create_user("a@label.test").await.unwrap();
    let a = store.for_account(t1.clone(), ua);
    let a_personal = a.ensure_personal_project().await.unwrap();
    let the_task = a.create_task(&a_personal, &task("labelled")).await.unwrap();
    let label = a.create_task_label("Design", Some("#4b83c4")).await.unwrap();
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
    let ud = store.for_tenant(t2.clone()).create_user("d@label.test").await.unwrap();
    let d = store.for_account(t2, ud);
    assert!(d.task_labels().await.unwrap().is_empty(), "labels are tenant-scoped");
    assert_not_found(d.labels_for_task(&the_task).await);
    assert_not_found(d.add_task_label(&the_task, &label).await);
    assert!(
        d.labels_for_task_ids(&[the_task.as_str().to_owned()]).await.unwrap().is_empty(),
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
    let u = store.for_tenant(t.clone()).create_user("a@purge.test").await.unwrap();
    let a = store.for_account(t.clone(), u);

    let project = a.ensure_personal_project().await.unwrap();
    let task = a.create_task(&project, &task("keep me until purge")).await.unwrap();
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
    assert!(a.subtasks(&task).await.unwrap().is_empty(), "subtasks purged too");
    assert!(a.task_comments(&task).await.unwrap().is_empty(), "comments purged too");
    assert!(
        a.task_projects().await.unwrap().is_empty(),
        "the tenant's task projects are purged too"
    );
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
    assert_eq!(a.space(&space).await.unwrap().unwrap().my_role, SpaceRole::Manager);
    assert_eq!(a.space_modules(&space).await.unwrap(), vec!["files".to_owned()]);

    // B is not a member: the Space and its membership are invisible, and B can
    // neither read members nor manage.
    assert!(b.space(&space).await.unwrap().is_none());
    assert!(b.spaces().await.unwrap().is_empty());
    assert_not_found(b.space_members(&space).await);
    assert_not_found(b.rename_space(&space, "hijacked").await);
    assert_not_found(b.add_space_member(&space, &UserId::new(ua_id.clone()), SpaceRole::Viewer).await);

    // A adds B as a viewer. Now B sees the space + membership, but as a viewer
    // cannot manage — that is Forbidden (B knows it exists), not NotFound.
    a.add_space_member(&space, &UserId::new(ub_id.clone()), SpaceRole::Viewer).await.unwrap();
    assert_eq!(b.space(&space).await.unwrap().unwrap().my_role, SpaceRole::Viewer);
    assert_eq!(b.space_members(&space).await.unwrap().len(), 2);
    assert_forbidden(b.rename_space(&space, "nope").await);
    assert_forbidden(
        b.add_space_member(&space, &UserId::new(uc_id.clone()), SpaceRole::Viewer).await,
    );

    // A promotes B to manager; B can now manage.
    a.add_space_member(&space, &UserId::new(ub_id.clone()), SpaceRole::Manager).await.unwrap();
    b.rename_space(&space, "Acme").await.unwrap();
    assert_eq!(a.space(&space).await.unwrap().unwrap().name, "Acme");

    // The last-manager guard: with two managers, A can be removed; the guard
    // only bites when it would leave zero managers.
    a.remove_space_member(&space, &UserId::new(ua_id.clone())).await.unwrap();
    // Now B is the only manager and cannot be removed or demoted.
    match b.remove_space_member(&space, &UserId::new(ub_id.clone())).await {
        Err(StoreError::Conflict(_)) => {}
        other => panic!("expected last-manager Conflict, got {other:?}"),
    }

    // Cross-tenant: an outsider sees nothing, and B cannot add a user from
    // another tenant into the space.
    let t2 = store.create_tenant("space-t2").await.unwrap();
    let ud = store.for_tenant(t2.clone()).create_user("d@space.test").await.unwrap();
    let ud_id = ud.as_str().to_owned();
    let d = store.for_account(t2, ud);
    assert!(d.space(&space).await.unwrap().is_none());
    assert_not_found(d.space_members(&space).await);
    assert_not_found(d.add_space_member(&space, &UserId::new(ub_id.clone()), SpaceRole::Viewer).await);
    match b.add_space_member(&space, &UserId::new(ud_id), SpaceRole::Viewer).await {
        Err(StoreError::Conflict(_)) => {}
        other => panic!("expected cross-tenant-user Conflict, got {other:?}"),
    }

    // C (still a non-member) remains fully locked out throughout.
    assert!(c.space(&space).await.unwrap().is_none());
    assert_not_found(c.space_members(&space).await);
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
    assert_eq!(a.drive_list(&DriveLocation::Personal, None).await.unwrap().len(), 1);
    assert!(b.drive_node(&mine).await.unwrap().is_none(), "another user's personal file is invisible");
    assert_not_found(b.drive_rename(&mine, "hax").await);
    // B's own personal location never shows A's file.
    assert!(b.drive_list(&DriveLocation::Personal, None).await.unwrap().is_empty());

    // A Space with a file. B is added as a viewer.
    let space = a.create_space("Team").await.unwrap();
    let sloc = DriveLocation::Space(space.clone());
    let folder = a.drive_create_folder(&sloc, None, "Docs").await.unwrap();
    let shared = a.drive_create_file(&sloc, Some(&folder), &file("brief.pdf")).await.unwrap();
    a.add_space_member(&space, &UserId::new(ub_id.clone()), SpaceRole::Viewer).await.unwrap();

    // Viewer B can read the space's files but not write them.
    assert!(b.drive_node(&shared).await.unwrap().is_some(), "a member reads space files");
    assert_eq!(b.drive_list(&sloc, Some(&folder)).await.unwrap().len(), 1);
    assert_forbidden(b.drive_rename(&shared, "nope").await);
    assert_forbidden(b.drive_create_file(&sloc, None, &file("x")).await);

    // Promote B to editor → now B can write.
    a.add_space_member(&space, &UserId::new(ub_id.clone()), SpaceRole::Editor).await.unwrap();
    b.drive_rename(&shared, "brief-v2.pdf").await.unwrap();
    assert_eq!(a.drive_node(&shared).await.unwrap().unwrap().name, "brief-v2.pdf");

    // Versioning: a new upload appends a version; history is kept.
    let v = a.drive_add_version(&shared, "blob-new", 20).await.unwrap();
    assert_eq!(v, 2);
    assert_eq!(a.drive_versions(&shared).await.unwrap().len(), 2);

    // Move re-scopes access: move A's PERSONAL file into the Space → B (a member)
    // can now see it; move it back → B loses access.
    a.drive_move(&mine, &sloc, None).await.unwrap();
    assert!(b.drive_node(&mine).await.unwrap().is_some(), "moving into a space grants members access");
    a.drive_move(&mine, &DriveLocation::Personal, None).await.unwrap();
    assert!(b.drive_node(&mine).await.unwrap().is_none(), "moving back out revokes it");

    // Trash / restore keep scoping.
    a.drive_trash_node(&shared).await.unwrap();
    assert!(a.drive_list(&sloc, Some(&folder)).await.unwrap().is_empty(), "trashed is hidden from listing");
    assert_eq!(a.drive_trash(&sloc).await.unwrap().len(), 1);
    a.drive_restore_node(&shared).await.unwrap();
    assert_eq!(a.drive_list(&sloc, Some(&folder)).await.unwrap().len(), 1);

    // Cross-tenant: an outsider sees nothing and can touch nothing.
    let t2 = store.create_tenant("drive-t2").await.unwrap();
    let ud = store.for_tenant(t2.clone()).create_user("d@drive.test").await.unwrap();
    let d = store.for_account(t2, ud);
    assert!(d.drive_node(&shared).await.unwrap().is_none());
    assert!(d.drive_node(&mine).await.unwrap().is_none());
    assert_not_found(d.drive_rename(&shared, "evil").await);
    assert_not_found(d.drive_versions(&shared).await);
    assert_not_found(d.drive_move(&shared, &DriveLocation::Personal, None).await);
    // The outsider cannot even address the Space location as their own.
    assert_not_found(d.drive_list(&DriveLocation::Space(space.clone()), None).await);
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
    a.add_space_member(&space, &UserId::new(ub_id.clone()), SpaceRole::Viewer).await.unwrap();

    // The Base loads with its default table; a record can be added by A.
    let base = a.base(&node).await.unwrap().unwrap();
    assert_eq!(base.tables.len(), 1, "default table");
    let table = base.tables[0].id.clone();
    assert_eq!(base.tables[0].fields.len(), 2, "Name + Notes");
    assert_eq!(base.tables[0].records.len(), 3, "three seed rows");
    let rec = a.base_add_record(&table, &json!({ "x": "hi" })).await.unwrap();

    // Viewer B can READ the base but not write it.
    assert!(b.base(&node).await.unwrap().is_some(), "a member reads the base");
    assert_forbidden(b.base_add_record(&table, &json!({})).await);
    assert_forbidden(b.base_update_record(&rec, &json!({})).await);
    assert_forbidden(b.base_add_field(&table, "Extra", "text", &json!({})).await);
    assert_forbidden(b.base_add_table(&node, "T2").await);

    // Promote B to editor → now B can write.
    a.add_space_member(&space, &UserId::new(ub_id.clone()), SpaceRole::Editor).await.unwrap();
    b.base_update_record(&rec, &json!({ "x": "edited" })).await.unwrap();

    // A private personal Base stays private to its owner.
    let personal = a.create_base(&DriveLocation::Personal, None, "Private").await.unwrap();
    assert!(a.base(&personal).await.unwrap().is_some());
    assert!(b.base(&personal).await.unwrap().is_none(), "another user's personal base is invisible");

    // Cross-tenant: an outsider sees nothing and can write nothing.
    let t2 = store.create_tenant("base-t2").await.unwrap();
    let ud = store.for_tenant(t2.clone()).create_user("d@base.test").await.unwrap();
    let d = store.for_account(t2, ud);
    assert!(d.base(&node).await.unwrap().is_none());
    assert_not_found(d.base_add_record(&table, &json!({})).await);
    assert_not_found(d.base_update_record(&rec, &json!({})).await);
    assert_not_found(d.base_add_table(&node, "evil").await);
    // A bad field type / view kind is refused (Conflict), not a silent accept.
    assert!(a.base_add_field(&table, "F", "not-a-type", &json!({})).await.is_err());
    assert!(a.base_add_view(&table, "not-a-kind", "V", &json!({})).await.is_err());
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
        &NewDriveFile { name: "Acme brief".to_owned(), blob_id: "x".to_owned(), size: 1, ..Default::default() },
    )
    .await
    .unwrap();
    let proj = a.ensure_personal_project().await.unwrap();
    a.create_task(&proj, &task("Acme kickoff")).await.unwrap();

    // A finds both; B (same tenant, non-owner) finds neither (private).
    assert_eq!(a.workspace_search("acme", 20).await.unwrap().len(), 2);
    assert!(b.workspace_search("acme", 20).await.unwrap().is_empty(), "another user's private items");

    // Cross-tenant: an outsider finds nothing.
    let t2 = store.create_tenant("srch-t2").await.unwrap();
    let ud = store.for_tenant(t2.clone()).create_user("d@srch.test").await.unwrap();
    let d = store.for_account(t2, ud);
    assert!(d.workspace_search("acme", 20).await.unwrap().is_empty(), "never across tenants");

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
    assert!(b.workspace_search("flamingo", 20).await.unwrap().is_empty(), "another user's mail");
    let t2 = store.create_tenant("srchm-t2").await.unwrap();
    let ud = store.for_tenant(t2.clone()).create_user("d@srchm.test").await.unwrap();
    let d = store.for_account(t2, ud);
    assert!(d.workspace_search("flamingo", 20).await.unwrap().is_empty(), "never across tenants");
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
        .put_blob(Bytes::from_static(b"the migration plan is due friday"), Some("text/plain"))
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
    let dblob = a.put_blob(Bytes::from_static(doc_json), Some("application/json")).await.unwrap();
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
    assert!(b.workspace_search("migration", 20).await.unwrap().is_empty(), "another user's private file");
    let t2 = store.create_tenant("srchc-t2").await.unwrap();
    let ud = store.for_tenant(t2.clone()).create_user("d@srchc.test").await.unwrap();
    let d = store.for_account(t2, ud);
    assert!(d.workspace_search("pangolin", 20).await.unwrap().is_empty(), "never across tenants");
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
        &NewDriveFile { name: "Acme proposal.docx".to_owned(), blob_id: "x".to_owned(), size: 1, ..Default::default() },
    )
    .await
    .unwrap();
    let proj = a.ensure_personal_project().await.unwrap();
    a.create_task(&proj, &task("Acme kickoff meeting")).await.unwrap();

    // The literal question is not a substring of either title, but its keyword
    // "acme" is — keyword retrieval finds both.
    let q = "what do I have about the Acme proposal?";
    assert!(a.workspace_search(q, 20).await.unwrap().is_empty(), "literal search misses");
    assert_eq!(a.workspace_search_terms(q, 20).await.unwrap().len(), 2, "keyword search finds both");

    // Still access-scoped: another user and another tenant get nothing.
    assert!(b.workspace_search_terms(q, 20).await.unwrap().is_empty());
    let t2 = store.create_tenant("kw-t2").await.unwrap();
    let ud = store.for_tenant(t2.clone()).create_user("d@kw.test").await.unwrap();
    let d = store.for_account(t2, ud);
    assert!(d.workspace_search_terms(q, 20).await.unwrap().is_empty());

    // An all-stopword question falls back cleanly (no keywords → no crash).
    assert!(a.workspace_search_terms("what is this?", 20).await.unwrap().is_empty());
}
