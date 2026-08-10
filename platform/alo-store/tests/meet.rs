//! Meetings: who may see one, who may join, and what the media engine is told.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use alo_store::{ChannelVisibility, MeetingId, NewMeeting};

#[tokio::test]
async fn a_meeting_in_a_room_is_visible_to_that_room_and_no_one_else() {
    let store = common::test_store().await;
    let t = store.create_tenant("meet-t").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@meet.test").await.unwrap();
    let ub = ts.create_user("ben@meet.test").await.unwrap();
    let a = store.for_account(t.clone(), ua);
    let b = store.for_account(t.clone(), ub);

    let private = a
        .create_channel("board", None, ChannelVisibility::Private)
        .await
        .unwrap();
    let meeting = a
        .create_meeting(&NewMeeting {
            title: "Q3 budget with Acme".to_owned(),
            channel_id: Some(private.clone()),
            event_id: None,
        })
        .await
        .unwrap();

    // Anna is in the room, so the meeting is hers to see and join.
    assert!(a.meeting(&meeting.id).await.is_ok());
    assert_eq!(a.live_meetings_in(&private).await.unwrap().len(), 1);

    // Ben is not. A meeting is not a way into a conversation.
    assert!(b.meeting(&meeting.id).await.is_err());
    assert!(b.join_meeting(&meeting.id).await.is_err());
    assert!(b.live_meetings_in(&private).await.is_err());
}

/// The engine is a third party. Its room name must not say what the meeting is
/// about, however tempting a readable name would be.
#[tokio::test]
async fn the_room_name_tells_the_engine_nothing() {
    let store = common::test_store().await;
    let t = store.create_tenant("meet-t2").await.unwrap();
    let ua = store
        .for_tenant(t.clone())
        .create_user("anna@meet.test")
        .await
        .unwrap();
    let a = store.for_account(t.clone(), ua);

    let meeting = a
        .create_meeting(&NewMeeting {
            title: "Acme renewal — pricing".to_owned(),
            ..NewMeeting::default()
        })
        .await
        .unwrap();
    for word in ["acme", "renewal", "pricing", "meet-t2"] {
        assert!(
            !meeting.room.to_lowercase().contains(word),
            "the engine is told {word:?} in the room name"
        );
    }
    // Two meetings with the same title must not collide into one room.
    let other = a
        .create_meeting(&NewMeeting {
            title: "Acme renewal — pricing".to_owned(),
            ..NewMeeting::default()
        })
        .await
        .unwrap();
    assert_ne!(meeting.room, other.room);
}

#[tokio::test]
async fn joining_is_idempotent_and_a_finished_meeting_cannot_be_rejoined() {
    let store = common::test_store().await;
    let t = store.create_tenant("meet-t3").await.unwrap();
    let ua = store
        .for_tenant(t.clone())
        .create_user("anna@meet.test")
        .await
        .unwrap();
    let a = store.for_account(t.clone(), ua);

    let meeting = a.create_meeting(&NewMeeting::default()).await.unwrap();
    assert!(meeting.started_at.is_none(), "nobody has joined yet");

    let first = a.join_meeting(&meeting.id).await.unwrap();
    let started = first.started_at.expect("joining starts it");
    let again = a.join_meeting(&meeting.id).await.unwrap();
    assert_eq!(
        again.started_at,
        Some(started),
        "pressing join twice does not move when it began"
    );
    assert_eq!(
        a.meeting_participants(&meeting.id).await.unwrap().len(),
        1,
        "one attendance, not two"
    );

    a.end_meeting(&meeting.id).await.unwrap();
    assert!(
        a.join_meeting(&meeting.id).await.is_err(),
        "a meeting that is over cannot be rejoined"
    );
    // Ending twice keeps the first ending.
    a.end_meeting(&meeting.id).await.unwrap();
}

/// A meeting attached to nothing belongs to whoever started it. Otherwise an
/// id guessed or leaked would open a call.
#[tokio::test]
async fn an_unattached_meeting_is_only_its_owners() {
    let store = common::test_store().await;
    let t = store.create_tenant("meet-t4").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@meet.test").await.unwrap();
    let ub = ts.create_user("ben@meet.test").await.unwrap();
    let a = store.for_account(t.clone(), ua);
    let b = store.for_account(t.clone(), ub);

    let meeting = a.create_meeting(&NewMeeting::default()).await.unwrap();
    assert!(a.meeting(&meeting.id).await.is_ok());
    assert!(b.meeting(&meeting.id).await.is_err());
}

/// Another tenant's meeting does not exist, even with its exact id.
#[tokio::test]
async fn a_meeting_never_crosses_a_tenant() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("meet-one").await.unwrap();
    let t2 = store.create_tenant("meet-two").await.unwrap();
    let u1 = store
        .for_tenant(t1.clone())
        .create_user("anna@one.test")
        .await
        .unwrap();
    let u2 = store
        .for_tenant(t2.clone())
        .create_user("anna@two.test")
        .await
        .unwrap();
    let a = store.for_account(t1, u1);
    let far = store.for_account(t2, u2);

    let meeting = a.create_meeting(&NewMeeting::default()).await.unwrap();
    assert!(far.meeting(&meeting.id).await.is_err());
    assert!(far.join_meeting(&meeting.id).await.is_err());
    assert!(
        far.meeting(&MeetingId::new(meeting.id.as_str().to_owned()))
            .await
            .is_err()
    );
}
