//! IMAP import: the dedup/ingest half against a real store (import a
//! batch, re-import it → all skipped, verify the new mail lands in the
//! Inbox and stays tenant-scoped), plus the endpoint's guard rails —
//! validation and the SSRF refusal of a private/loopback host.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use crate::common::{harness, send};
use alo_jmap::imap_import::{
    FetchedFlags, FetchedMessage, FolderTarget, ImportOutcome, RawFolder, import_folders,
    import_messages,
};
use alo_store::Page;
use axum::body::Body;
use axum::http::{Request, StatusCode};

fn msg(id: &str, subject: &str) -> Vec<u8> {
    format!(
        "From: old@example.eu\r\nTo: me@example.test\r\nSubject: {subject}\r\n\
         Message-ID: <{id}>\r\nDate: Mon, 27 Jul 2026 00:00:00 +0000\r\n\r\nbody {id}\r\n"
    )
    .into_bytes()
}

#[tokio::test]
async fn import_ingests_then_dedupes_on_reimport() {
    let h = harness("imap-dedup").await;
    let inbox = h.acc.inbox().await.unwrap();
    let before = h
        .acc
        .list_mailbox(&inbox, Page::default())
        .await
        .unwrap()
        .len();

    let batch = vec![msg("a@imp", "first"), msg("b@imp", "second")];

    // First import: both are new.
    let out = import_messages(&h.acc, batch.clone()).await.unwrap();
    assert_eq!(
        out,
        ImportOutcome {
            imported: 2,
            skipped: 0,
            failed: 0
        }
    );
    let after = h.acc.list_mailbox(&inbox, Page::default()).await.unwrap();
    assert_eq!(after.len(), before + 2, "both landed in the Inbox");
    assert!(after.iter().any(|m| m.subject == "first"));

    // Re-import the same batch: both already present → skipped, none added.
    let again = import_messages(&h.acc, batch).await.unwrap();
    assert_eq!(
        again,
        ImportOutcome {
            imported: 0,
            skipped: 2,
            failed: 0
        }
    );
    assert_eq!(
        h.acc
            .list_mailbox(&inbox, Page::default())
            .await
            .unwrap()
            .len(),
        before + 2,
        "no duplicates on re-import"
    );
}

fn fmsg(id: &str, subject: &str, flags: FetchedFlags) -> FetchedMessage {
    FetchedMessage {
        raw: msg(id, subject),
        flags,
    }
}

#[tokio::test]
async fn import_folders_maps_structure_and_flags() {
    let h = harness("imap-folders").await;
    let seen = FetchedFlags {
        seen: true,
        ..Default::default()
    };
    let flagged = FetchedFlags {
        flagged: true,
        ..Default::default()
    };
    let folders = vec![
        RawFolder {
            target: FolderTarget::Role {
                role: "inbox",
                name: "Inbox",
            },
            messages: vec![fmsg("i1@imp", "in-seen", seen)],
        },
        RawFolder {
            target: FolderTarget::Role {
                role: "sent",
                name: "Sent",
            },
            messages: vec![fmsg("s1@imp", "sent-msg", FetchedFlags::default())],
        },
        RawFolder {
            target: FolderTarget::Named("Work".into()),
            messages: vec![fmsg("w1@imp", "work-msg", flagged)],
        },
    ];
    let out = import_folders(&h.acc, folders).await.unwrap();
    assert_eq!(
        out,
        ImportOutcome {
            imported: 3,
            skipped: 0,
            failed: 0
        }
    );

    // The Sent *role* mailbox was get-or-created and holds the sent message.
    let sent = h
        .acc
        .mailbox_by_role("sent")
        .await
        .unwrap()
        .expect("sent mailbox created");
    let sent_list = h.acc.list_mailbox(&sent, Page::default()).await.unwrap();
    assert!(sent_list.iter().any(|m| m.subject == "sent-msg"));

    // A user folder "Work" was created by name and holds its message, with the
    // remote \Flagged carried over as the $flagged keyword.
    let boxes = h.acc.mailboxes(Page::default()).await.unwrap();
    let work = boxes
        .iter()
        .find(|m| m.name == "Work")
        .expect("Work folder created");
    let work_list = h.acc.list_mailbox(&work.id, Page::default()).await.unwrap();
    let work_msg = work_list.iter().find(|m| m.subject == "work-msg").unwrap();
    let kws = h.acc.keywords(&work_msg.id).await.unwrap();
    assert!(
        kws.iter().any(|k| k == "$flagged"),
        "flag preserved: {kws:?}"
    );

    // The INBOX message's \Seen carried over as $seen.
    let inbox = h.acc.inbox().await.unwrap();
    let in_list = h.acc.list_mailbox(&inbox, Page::default()).await.unwrap();
    let in_msg = in_list.iter().find(|m| m.subject == "in-seen").unwrap();
    let in_kws = h.acc.keywords(&in_msg.id).await.unwrap();
    assert!(
        in_kws.iter().any(|k| k == "$seen"),
        "seen preserved: {in_kws:?}"
    );

    // Re-import → already present, skipped, no duplicate.
    let again = import_folders(
        &h.acc,
        vec![RawFolder {
            target: FolderTarget::Role {
                role: "inbox",
                name: "Inbox",
            },
            messages: vec![fmsg("i1@imp", "in-seen", seen)],
        }],
    )
    .await
    .unwrap();
    assert_eq!(again.imported, 0);
    assert_eq!(again.skipped, 1);
}

#[tokio::test]
async fn import_folders_dedupes_across_folders_in_one_run() {
    let h = harness("imap-xfolder").await;
    // The same Message-ID present in two remote folders is stored once (the
    // first folder in priority order wins), not duplicated.
    let folders = vec![
        RawFolder {
            target: FolderTarget::Role {
                role: "inbox",
                name: "Inbox",
            },
            messages: vec![fmsg("dup@imp", "dup", FetchedFlags::default())],
        },
        RawFolder {
            target: FolderTarget::Named("Work".into()),
            messages: vec![fmsg("dup@imp", "dup", FetchedFlags::default())],
        },
    ];
    let out = import_folders(&h.acc, folders).await.unwrap();
    assert_eq!(
        out,
        ImportOutcome {
            imported: 1,
            skipped: 1,
            failed: 0
        }
    );
}

#[tokio::test]
async fn imported_mail_is_tenant_scoped() {
    let a = harness("imap-iso-a").await;
    let b = harness("imap-iso-b").await;
    import_messages(&a.acc, vec![msg("secret@imp", "A only")])
        .await
        .unwrap();

    let b_inbox = b.acc.inbox().await.unwrap();
    let b_list = b.acc.list_mailbox(&b_inbox, Page::default()).await.unwrap();
    assert!(
        b_list.iter().all(|m| m.subject != "A only"),
        "B never sees A's imported mail"
    );
}

async fn post_import(h: &common::Harness, body: &str) -> (StatusCode, String) {
    let req = Request::builder()
        .method("POST")
        .uri("/import/imap")
        .header("authorization", format!("Bearer {}", h.token))
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap();
    let (status, json) = send(&h.app, req).await;
    (status, json.to_string())
}

#[tokio::test]
async fn endpoint_validates_and_refuses_ssrf() {
    let h = harness("imap-guard").await;

    // Missing fields → 400.
    let (status, _) = post_import(&h, r#"{"host":"","username":"","password":""}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // A loopback/private host is refused by the SSRF guard (Host → 400),
    // never dialed — the import wizard must not become an internal-network
    // probe. 127.0.0.1 resolves to a blocked address.
    let (status, _) = post_import(
        &h,
        r#"{"host":"127.0.0.1","port":993,"username":"u","password":"p"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "loopback host refused");
}
