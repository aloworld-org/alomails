//! WIRE PROOF (interop): EmailSubmission/set strips Bcc from the bytes put on
//! the SMTP wire, still delivers to the blind-carbon recipient via the
//! envelope, and keeps Bcc in the sender own Sent copy. Drives the REAL
//! submission method against a REAL Postgres store and a REAL alo-smtp-client
//! connection to a tiny in-process SMTP sink that records the exact RCPT TO
//! lines and the exact DATA payload the recipient receives. Disposable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use alo_identity::{Identity, IdentityConfig};
use alo_jmap::PushHub;
use alo_jmap::mime::{Addr, Outgoing, build};
use alo_jmap::state::{Account, AppState, Limits};
use alo_store::{BlobStore, MAX_PAGE, Page, Store};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

const NL: u8 = 10;
const DOT: u8 = 46;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://alo:alo@127.0.0.1:5455/alo".to_owned())
}

fn undot(raw: &[u8]) -> Vec<u8> {
    let body = match raw.windows(5).position(|w| w == b"\r\n.\r\n") {
        Some(pos) => &raw[..pos + 2],
        None => raw,
    };
    let mut out = Vec::with_capacity(body.len());
    for seg in body.split_inclusive(|&b| b == NL) {
        if seg.first() == Some(&DOT) {
            out.extend_from_slice(&seg[1..]);
        } else {
            out.extend_from_slice(seg);
        }
    }
    out
}

async fn spawn_sink() -> (String, tokio::task::JoinHandle<(Vec<String>, Vec<u8>)>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let handle = tokio::spawn(async move {
        let (stream, _peer) = listener.accept().await.unwrap();
        let (rd, mut wr) = stream.into_split();
        let mut reader = BufReader::new(rd);
        let mut rcpts = Vec::new();
        let mut data = Vec::new();
        wr.write_all(b"220 sink.test ESMTP\r\n").await.unwrap();
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await.unwrap();
            if n == 0 {
                break;
            }
            let cmd = line.trim_end();
            let upper = cmd.to_ascii_uppercase();
            if upper.starts_with("EHLO") || upper.starts_with("HELO") {
                wr.write_all(b"250 sink.test\r\n").await.unwrap();
            } else if upper.starts_with("MAIL FROM") {
                wr.write_all(b"250 2.1.0 ok\r\n").await.unwrap();
            } else if upper.starts_with("RCPT TO") {
                rcpts.push(cmd.to_owned());
                wr.write_all(b"250 2.1.5 ok\r\n").await.unwrap();
            } else if upper.starts_with("DATA") {
                wr.write_all(b"354 go\r\n").await.unwrap();
                loop {
                    let mut buf = [0u8; 4096];
                    let m = reader.read(&mut buf).await.unwrap();
                    if m == 0 {
                        break;
                    }
                    data.extend_from_slice(&buf[..m]);
                    if data.ends_with(b"\r\n.\r\n") {
                        break;
                    }
                }
                wr.write_all(b"250 2.0.0 queued\r\n").await.unwrap();
            } else if upper.starts_with("RSET") {
                wr.write_all(b"250 2.0.0 ok\r\n").await.unwrap();
            } else if upper.starts_with("QUIT") {
                wr.write_all(b"221 2.0.0 bye\r\n").await.unwrap();
                break;
            } else {
                wr.write_all(b"250 ok\r\n").await.unwrap();
            }
        }
        (rcpts, undot(&data))
    });
    (addr, handle)
}

#[tokio::test]
async fn bcc_stripped_on_wire_delivered_by_envelope_kept_in_sent() {
    let Ok(pool) = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
    else {
        eprintln!("SKIP: no database at {}", database_url());
        return;
    };
    let store = Arc::new(Store::new(pool, BlobStore::in_memory(50 * 1024 * 1024)));
    store.migrate().await.unwrap();

    let tenant = store.create_tenant("bcc-wire").await.unwrap();
    let sender = format!("sender-{tenant}@sink.test").to_lowercase();
    let ts = store.for_tenant(tenant.clone());
    let user = ts.create_user(&sender).await.unwrap();
    let acc = store.for_account(tenant.clone(), user.clone());

    let to = "alice-to@recipient.test";
    let cc = "carol-cc@recipient.test";
    let bcc = "dave-bcc@recipient.test";

    let draft = Outgoing {
        from: Addr {
            name: None,
            email: sender.clone(),
        },
        to: vec![Addr {
            name: Some("Alice To".into()),
            email: to.into(),
        }],
        cc: vec![Addr {
            name: Some("Carol Cc".into()),
            email: cc.into(),
        }],
        bcc: vec![Addr {
            name: Some("Dave Bcc".into()),
            email: bcc.into(),
        }],
        subject: "Wire proof bcc handling".into(),
        in_reply_to: Vec::new(),
        references: Vec::new(),
        body_text: "Proving Bcc never crosses the wire.\n".into(),
        body_html: None,
        attachments: Vec::new(),
        message_id_domain: "sink.test".into(),
        message_id_token: "wireproof001".into(),
    };
    let raw = build(&draft);
    let bcc_hdr = format!("Bcc: \"Dave Bcc\" <{bcc}>");
    assert!(
        String::from_utf8_lossy(&raw).contains(&bcc_hdr),
        "sanity: the stored draft must contain the Bcc header"
    );

    let drafts = acc
        .create_mailbox(None, "Drafts", Some("drafts"))
        .await
        .unwrap();
    let _sent = acc
        .create_mailbox(None, "Sent", Some("sent"))
        .await
        .unwrap();
    let mid = acc.ingest(&drafts, &raw).await.unwrap();
    acc.set_keyword(&mid, "$draft", true).await.unwrap();

    let (sink_addr, sink) = spawn_sink().await;
    let identity =
        Identity::new(Arc::clone(&store), IdentityConfig::new("https://id.test")).unwrap();
    let state = AppState {
        media: None,
        turns: Default::default(),
        store: Arc::clone(&store),
        identity,
        push: PushHub::new(),
        limits: Limits::default(),
        base_url: "http://test".into(),
        submission_addr: Some(sink_addr),
        junk_learner: None,
        personal_domains: Vec::new(),
        signup_limiter: alo_identity::ratelimit::RateLimiter::new(),
    };
    let account = Account {
        tenant: tenant.clone(),
        user: user.clone(),
        acc: acc.clone(),
        is_admin: false,
        roles: Vec::new(),
        delegated: None,
    };

    let args = json!({
        "accountId": user.to_string(),
        "create": {
            "c1": {
                "emailId": mid.to_string(),
                "envelope": {
                    "mailFrom": { "email": sender },
                    "rcptTo": [ {"email": to}, {"email": cc}, {"email": bcc} ]
                }
            }
        }
    });
    let resp = alo_jmap::submission::set(&account, &args, &state)
        .await
        .expect("EmailSubmission/set returned a method-level error");
    assert!(
        resp["created"]["c1"].is_object(),
        "submission was not created: {resp}"
    );

    let (rcpts, data) = sink.await.unwrap();
    let wire = String::from_utf8_lossy(&data);
    let header_block = wire.split("\r\n\r\n").next().unwrap_or("");

    eprintln!("\n===== CAPTURED WIRE DATA header block the recipient received =====");
    eprintln!("{header_block}");
    eprintln!("===== CAPTURED ENVELOPE RCPT TO lines =====");
    for r in &rcpts {
        eprintln!("{r}");
    }

    let to_hdr = format!("To: \"Alice To\" <{to}>");
    let cc_hdr = format!("Cc: \"Carol Cc\" <{cc}>");
    assert!(header_block.contains(&to_hdr), "To missing on wire");
    assert!(header_block.contains(&cc_hdr), "Cc missing on wire");
    assert!(
        !header_block.to_ascii_lowercase().contains("bcc:"),
        "PRIVACY LEAK: a Bcc header crossed the wire:\n{header_block}"
    );
    assert!(
        !wire.contains(bcc),
        "the bcc address string appears in the wire bytes"
    );

    assert!(
        rcpts.iter().any(|r| r.contains(bcc)),
        "Bcc recipient not in envelope RCPT TO"
    );
    assert!(rcpts.iter().any(|r| r.contains(to)));
    assert!(rcpts.iter().any(|r| r.contains(cc)));

    let sent_bytes = acc.message_bytes(&mid).await.unwrap();
    let sent_str = String::from_utf8_lossy(&sent_bytes);
    let sent_header_block = sent_str.split("\r\n\r\n").next().unwrap_or("");
    eprintln!("===== SENDER SENT COPY header block from the store =====");
    eprintln!("{sent_header_block}");
    let sent_bcc_line = sent_header_block
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("bcc:"))
        .expect("Sent copy lost its Bcc header");
    eprintln!("Sent-copy Bcc header: {sent_bcc_line}");
    assert!(
        sent_bcc_line.contains(bcc),
        "Sent copy Bcc lacks the bcc address"
    );

    let boxes = acc.mailboxes(Page::first(MAX_PAGE)).await.unwrap();
    let sent_id = boxes
        .iter()
        .find(|m| m.role.as_deref() == Some("sent"))
        .unwrap()
        .id
        .clone();
    let member_of = acc.mailboxes_of_message(&mid).await.unwrap();
    assert!(
        member_of.iter().any(|m| m == &sent_id),
        "not filed into Sent"
    );
    assert!(
        !member_of.iter().any(|m| m == &drafts),
        "still in Drafts after send"
    );

    eprintln!("\nALL THREE CLAIMS HOLD.\n");
    store.delete_tenant(&tenant).await.unwrap();
}
