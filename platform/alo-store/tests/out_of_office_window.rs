//! Out-of-office scheduling against real Postgres: a holiday set in advance
//! stays quiet until it starts, replies while it runs, stops by itself when it
//! ends, and never touches a `vacation` rule the user wrote themselves.
//!
//! These go through `deliver_sieve` rather than calling `active_at` directly,
//! because the thing worth proving is not the arithmetic — it is that the
//! window is consulted at the one moment that matters, when a message arrives
//! and the reply is decided.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::OutboundAction;
use time::{Duration, OffsetDateTime};

/// Whether a delivery produced an auto-reply.
fn replied(outcome: &alo_store::SieveDelivery) -> bool {
    outcome
        .outbound
        .iter()
        .any(|a| matches!(a, OutboundAction::Vacation { .. }))
}

/// Sends one message to `owner` from a correspondent nobody else has used.
///
/// The sender differs per call because a vacation reply is sent at most once
/// per correspondent per `:days`; reusing one address would make the second
/// delivery in a test silent for that reason instead of the one under test.
async fn deliver_from(
    acc: &alo_store::AccountStore,
    owner: &str,
    sender: &str,
) -> alo_store::SieveDelivery {
    let raw = format!("From: {sender}\r\nTo: {owner}\r\nSubject: hi\r\n\r\nq\r\n");
    acc.deliver_sieve(raw.as_bytes(), Some(sender), owner)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_holiday_set_in_advance_is_silent_until_it_starts() {
    // The case the feature exists for: you set it the evening before you leave.
    // Until now the only way to do that was to remember on the morning itself.
    let store = common::test_store().await;
    let (acc, _u, _inbox) = common::fresh_account(&store, "ooo-future").await;
    let owner = "u-ooo-future@example.test";
    let now = OffsetDateTime::now_utc();

    acc.set_out_of_office(
        true,
        "Away",
        "I am away",
        Some(now + Duration::days(3)),
        Some(now + Duration::days(10)),
    )
    .await
    .unwrap();

    let outcome = deliver_from(&acc, owner, "early@ext.test").await;
    assert!(
        !replied(&outcome),
        "a holiday three days out must not answer today: {:?}",
        outcome.outbound
    );
    assert!(
        outcome.warnings.iter().any(|w| w.contains("window")),
        "and it says why, so an operator reading a trace is not guessing: {:?}",
        outcome.warnings
    );
}

#[tokio::test]
async fn it_replies_while_the_holiday_is_running() {
    let store = common::test_store().await;
    let (acc, _u, _inbox) = common::fresh_account(&store, "ooo-now").await;
    let owner = "u-ooo-now@example.test";
    let now = OffsetDateTime::now_utc();

    acc.set_out_of_office(
        true,
        "Away",
        "I am away",
        Some(now - Duration::days(1)),
        Some(now + Duration::days(6)),
    )
    .await
    .unwrap();

    let outcome = deliver_from(&acc, owner, "during@ext.test").await;
    assert!(
        replied(&outcome),
        "inside the window it answers: {:?}",
        outcome.outbound
    );
}

#[tokio::test]
async fn it_stops_by_itself_when_the_holiday_ends() {
    // Nothing runs on a timer to switch this off. If this test passes, the
    // reply stopped because a message arrived after the end date and the
    // window was read then — which is the only way it can stop without a
    // scheduler that could be down on the day.
    let store = common::test_store().await;
    let (acc, _u, _inbox) = common::fresh_account(&store, "ooo-past").await;
    let owner = "u-ooo-past@example.test";
    let now = OffsetDateTime::now_utc();

    acc.set_out_of_office(
        true,
        "Away",
        "I am away",
        Some(now - Duration::days(10)),
        Some(now - Duration::days(1)),
    )
    .await
    .unwrap();

    let outcome = deliver_from(&acc, owner, "late@ext.test").await;
    assert!(
        !replied(&outcome),
        "a holiday that ended yesterday is over: {:?}",
        outcome.outbound
    );
}

#[tokio::test]
async fn a_reply_with_no_window_behaves_as_it_always_did() {
    // Every account that had out-of-office on before this change stored no
    // dates. Their behaviour must not move.
    let store = common::test_store().await;
    let (acc, _u, _inbox) = common::fresh_account(&store, "ooo-open").await;
    let owner = "u-ooo-open@example.test";

    acc.set_out_of_office(true, "Away", "I am away", None, None)
        .await
        .unwrap();

    let outcome = deliver_from(&acc, owner, "whenever@ext.test").await;
    assert!(
        replied(&outcome),
        "on with no dates means on: {:?}",
        outcome.outbound
    );
}

#[tokio::test]
async fn a_hand_written_vacation_rule_is_not_gated_by_the_settings_window() {
    // Someone who writes their own Sieve `vacation` means it to fire when their
    // rule says. Gating it on a settings screen they never opened would break a
    // working rule invisibly — the window applies only to the reply we manage.
    let store = common::test_store().await;
    let (acc, _u, _inbox) = common::fresh_account(&store, "ooo-hand").await;
    let owner = "u-ooo-hand@example.test";
    let now = OffsetDateTime::now_utc();

    // A settings window that is firmly over.
    acc.set_out_of_office(
        true,
        "Away",
        "I am away",
        Some(now - Duration::days(10)),
        Some(now - Duration::days(1)),
    )
    .await
    .unwrap();

    // …and the user's own script, which replaces the managed one as the active
    // script and carries no handle of ours.
    acc.put_sieve_script(
        "mine",
        "require [\"vacation\"]; vacation :days 7 :subject \"Mine\" \"my own rule\";",
    )
    .await
    .unwrap();
    acc.activate_sieve_script(Some("mine")).await.unwrap();

    let outcome = deliver_from(&acc, owner, "hand@ext.test").await;
    assert!(
        replied(&outcome),
        "the user's own rule fires on its own terms: {:?}",
        outcome.outbound
    );
}

#[tokio::test]
async fn a_window_that_ends_before_it_starts_is_refused() {
    // Stored, it would simply never fire, which reads to the person who set it
    // exactly like the feature being broken.
    let store = common::test_store().await;
    let (acc, _u, _inbox) = common::fresh_account(&store, "ooo-backwards").await;
    let now = OffsetDateTime::now_utc();

    let err = acc
        .set_out_of_office_state(
            true,
            "Away",
            "I am away",
            Some(now + Duration::days(5)),
            Some(now + Duration::days(1)),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, alo_store::StoreError::Validation(_)),
        "refused as invalid input, not as a server fault: {err:?}"
    );
}

#[tokio::test]
async fn the_window_is_account_scoped() {
    // Two users in one tenant: B's holiday is B's, and A's silence is not
    // inherited from it.
    let store = common::test_store().await;
    let tenant = store.create_tenant("ooo-scope").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let ua = ts.create_user("a@ooo.test").await.unwrap();
    let ub = ts.create_user("b@ooo.test").await.unwrap();
    let a = store.for_account(tenant.clone(), ua);
    let b = store.for_account(tenant, ub);
    let now = OffsetDateTime::now_utc();

    b.set_out_of_office(
        true,
        "Away",
        "B is away",
        Some(now - Duration::days(1)),
        Some(now + Duration::days(6)),
    )
    .await
    .unwrap();

    let a_ooo = a.out_of_office().await.unwrap();
    assert!(!a_ooo.enabled, "A has set nothing");
    assert!(a_ooo.from.is_none() && a_ooo.to.is_none());

    let b_ooo = b.out_of_office().await.unwrap();
    assert!(b_ooo.enabled && b_ooo.from.is_some() && b_ooo.to.is_some());
}
