//! Product-scoped retrieval (A1.3): an agent grounds in its **own** product's
//! records, and "Ask alo" is the only one that looks everywhere.
//!
//! The whole point of the item is a negative, so the negatives are what these
//! tests assert. One workspace holds the same word in a file, a task, an email,
//! a contact, a diary entry and a room — and each product's agent is then shown
//! exactly one of them.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, AgentProduct, CalendarEvent, CalendarId, ChannelVisibility, Contact, ContactId,
    DriveLocation, EventId, NewDriveFile, NewTask, SearchHit,
};
use time::{Duration, OffsetDateTime};

/// The word every record below carries, so a product's grounding can only ever
/// be narrower than the workspace's — never a different question.
const WORD: &str = "pangolin";

/// The question every agent is asked, phrased as a person would.
const QUESTION: &str = "what is going on with the pangolin?";

fn kinds(hits: &[SearchHit]) -> Vec<&str> {
    hits.iter().map(|hit| hit.kind.as_str()).collect()
}

/// One workspace with the same word filed in six different products.
async fn seed(acc: &AccountStore) {
    acc.drive_create_file(
        &DriveLocation::Personal,
        None,
        &NewDriveFile {
            name: format!("{WORD} report.docx"),
            blob_id: "x".to_owned(),
            size: 1,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let project = acc.ensure_personal_project().await.unwrap();
    acc.create_task(
        &project,
        &NewTask {
            title: format!("chase the {WORD}"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let inbox = acc.inbox().await.unwrap();
    common::deliver(
        acc,
        &inbox,
        "<ground-1@alo.test>",
        &[],
        &format!("the {WORD} account"),
    )
    .await;

    acc.create_contact(&Contact {
        id: ContactId::generate(),
        display_name: format!("{WORD} Ltd"),
        first_name: None,
        last_name: None,
        emails: Vec::new(),
        phones: Vec::new(),
        organization: None,
        job_title: None,
        notes: None,
    })
    .await
    .unwrap();

    let calendar = acc.ensure_personal_calendar().await.unwrap();
    acc.create_event(&event(&calendar, &format!("{WORD} review")))
        .await
        .unwrap();

    let room = acc
        .create_channel("ground", None, ChannelVisibility::Public)
        .await
        .unwrap();
    acc.post_message(&room, &format!("the {WORD} shipped today"), None)
        .await
        .unwrap();
}

fn event(calendar: &CalendarId, summary: &str) -> CalendarEvent {
    let start = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
    CalendarEvent {
        id: EventId::generate(),
        calendar_id: calendar.clone(),
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

/// The sentence the queue item asks to be proved, and the five beside it: each
/// product's agent sees its own records and nobody else's, out of a workspace
/// where every product holds a match.
#[tokio::test]
async fn each_product_grounds_in_its_own_records_and_no_others() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("ground-t1").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("anna@ground.test")
        .await
        .unwrap();
    let acc = store.for_account(tenant, user);
    seed(&acc).await;

    // Mail: its own two sources, and — the item's own words — no Drive rows.
    let mail = acc
        .agent_ground(AgentProduct::Mail, QUESTION, 20)
        .await
        .unwrap();
    assert_eq!(kinds(&mail), ["message", "contact"]);
    assert!(
        !kinds(&mail).contains(&"file"),
        "a Mail agent's grounding contains no Drive rows"
    );

    assert_eq!(
        kinds(
            &acc.agent_ground(AgentProduct::Drive, QUESTION, 20)
                .await
                .unwrap()
        ),
        ["file"]
    );
    assert_eq!(
        kinds(
            &acc.agent_ground(AgentProduct::Tasks, QUESTION, 20)
                .await
                .unwrap()
        ),
        ["task"]
    );
    assert_eq!(
        kinds(
            &acc.agent_ground(AgentProduct::Agenda, QUESTION, 20)
                .await
                .unwrap()
        ),
        ["event"]
    );
    let chat = acc
        .agent_ground(AgentProduct::Chat, QUESTION, 20)
        .await
        .unwrap();
    assert_eq!(kinds(&chat), ["chat"]);
    assert_eq!(chat[0].title, format!("the {WORD} shipped today"));

    // Ask alo is the one agent that looks everywhere — and it looks at exactly
    // what the shared workspace search returns, unchanged.
    let workspace = acc
        .agent_ground(AgentProduct::Workspace, QUESTION, 20)
        .await
        .unwrap();
    assert_eq!(kinds(&workspace), ["file", "task", "message"]);
    assert_eq!(
        kinds(&workspace),
        kinds(&acc.workspace_search_terms(QUESTION, 20).await.unwrap())
    );

    // A product that reaches its records through a reading tool is grounded in
    // nothing at all — narrower than what it had, never wider. Asked in a
    // workspace where six records match, the Inventory agent is shown none of
    // them, which is what stops it answering a stock question from somebody's
    // email.
    for by_tool in [
        AgentProduct::Billing,
        AgentProduct::Crm,
        AgentProduct::Projects,
        AgentProduct::Finance,
        AgentProduct::Inventory,
        AgentProduct::Hr,
        AgentProduct::Insights,
        AgentProduct::Meet,
        AgentProduct::Sites,
    ] {
        assert!(
            acc.agent_ground(by_tool, QUESTION, 20)
                .await
                .unwrap()
                .is_empty(),
            "{by_tool} grounds in nothing and uses its tools"
        );
    }
}

/// Law 1, over every product at once: grounding is one person's reach in one
/// tenant, and neither a colleague nor another tenant can be grounded in it.
#[tokio::test]
async fn grounding_is_never_another_tenants_and_never_a_colleagues() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("ground-t2").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let anna = ts.create_user("anna@ground2.test").await.unwrap();
    let bob = ts.create_user("bob@ground2.test").await.unwrap();
    let a = store.for_account(tenant.clone(), anna);
    let b = store.for_account(tenant, bob);
    seed(&a).await;

    let other = store.create_tenant("ground-t3").await.unwrap();
    let dana = store
        .for_tenant(other.clone())
        .create_user("dana@ground3.test")
        .await
        .unwrap();
    let d = store.for_account(other, dana);

    for product in [
        AgentProduct::Mail,
        AgentProduct::Agenda,
        AgentProduct::Tasks,
        AgentProduct::Drive,
        AgentProduct::Workspace,
    ] {
        assert!(
            !a.agent_ground(product, QUESTION, 20)
                .await
                .unwrap()
                .is_empty(),
            "{product} finds Anna's own records"
        );
        // A colleague in the same tenant reaches none of it: the mailbox, the
        // address book, the diary and the personal file are hers.
        assert!(
            b.agent_ground(product, QUESTION, 20)
                .await
                .unwrap()
                .is_empty(),
            "{product} is not a colleague's to see"
        );
        // And another tenant reaches none of it, ever.
        assert!(
            d.agent_ground(product, QUESTION, 20)
                .await
                .unwrap()
                .is_empty(),
            "{product} is never another tenant's"
        );
    }
}

/// A private room the asker is not in is not grounding, however the question is
/// phrased — the Chat agent carries the asker's membership, not the agent's.
#[tokio::test]
async fn a_chat_agent_never_grounds_in_a_room_the_asker_is_not_in() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("ground-t4").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let anna = ts.create_user("anna@ground4.test").await.unwrap();
    let bob = ts.create_user("bob@ground4.test").await.unwrap();
    let a = store.for_account(tenant.clone(), anna);
    let b = store.for_account(tenant, bob);

    let private = a
        .create_channel("board", None, ChannelVisibility::Private)
        .await
        .unwrap();
    a.post_message(&private, &format!("the {WORD} deal closes friday"), None)
        .await
        .unwrap();

    assert_eq!(
        kinds(
            &a.agent_ground(AgentProduct::Chat, QUESTION, 20)
                .await
                .unwrap()
        ),
        ["chat"],
        "Anna is in the room"
    );
    assert!(
        b.agent_ground(AgentProduct::Chat, QUESTION, 20)
            .await
            .unwrap()
            .is_empty(),
        "Bob is not, so nothing from it grounds his turn"
    );
}

/// A question of nothing grounds in nothing, rather than in everything.
#[tokio::test]
async fn an_empty_question_grounds_in_nothing() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("ground-t5").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("anna@ground5.test")
        .await
        .unwrap();
    let acc = store.for_account(tenant, user);
    seed(&acc).await;

    for blank in ["", "   ", "\n"] {
        assert!(
            acc.agent_ground(AgentProduct::Mail, blank, 20)
                .await
                .unwrap()
                .is_empty(),
            "{blank:?} grounds in nothing"
        );
    }
    // A question of pure stop-words still grounds — on the phrase itself, which
    // matches nothing here rather than everything.
    assert!(
        acc.agent_ground(AgentProduct::Mail, "who are they?", 20)
            .await
            .unwrap()
            .is_empty()
    );
}
