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
