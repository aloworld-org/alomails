//! A scripted, local, **offline** model for suites that drive an agent turn.
//!
//! No live provider is ever called from a test: the tenant's AI backend is a
//! socket on `127.0.0.1` that hands back fixture completions in order and
//! records what it was asked. Recording the request is the point — an agent
//! suite's sharpest assertions are about what the model was *shown* (its
//! grounding, its tool list), which a mock returning canned text alone cannot
//! prove.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::Harness;

/// The request bodies the fake backend has been sent, in order.
pub type Seen = Arc<Mutex<Vec<Value>>>;

/// A minimal OpenAI-compatible chat-completions endpoint on localhost that
/// answers `script` in order (the last entry repeats), recording what it was
/// asked. It speaks just enough HTTP/1.1 for `reqwest`.
pub async fn scripted_model(script: Vec<String>) -> (String, Seen) {
    scripted_model_paced(script, std::time::Duration::ZERO).await
}

/// The same, with every answer held back for `pace`.
///
/// For the one property that cannot be seen from an instant backend: a
/// **multi-step run being stopped in the middle of itself** (A3.1). With no
/// delay the whole run is over before a Stop could be sent, so the suite would
/// be testing whether it can win a race rather than whether stopping works.
pub async fn scripted_model_paced(
    script: Vec<String>,
    pace: std::time::Duration,
) -> (String, Seen) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let record = Arc::clone(&record);
            let script = script.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                let body = loop {
                    let Ok(n) = sock.read(&mut chunk).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
                        continue;
                    };
                    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
                    let length: usize = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + length {
                        break buf[end + 4..end + 4 + length].to_vec();
                    }
                };
                let turn = {
                    let mut seen = record.lock().unwrap();
                    seen.push(serde_json::from_slice(&body).unwrap_or(Value::Null));
                    seen.len() - 1
                };
                let content = script
                    .get(turn)
                    .or_else(|| script.last())
                    .cloned()
                    .unwrap_or_default();
                if !pace.is_zero() {
                    tokio::time::sleep(pace).await;
                }
                let answer =
                    json!({ "choices": [{ "message": { "role": "assistant", "content": content } }] })
                        .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    answer.len(),
                    answer
                );
                let _ = sock.write_all(response.as_bytes()).await;
                let _ = sock.flush().await;
            });
        }
    });
    (format!("http://{addr}"), seen)
}

/// The decision envelope for a tool the model wants used.
#[must_use]
pub fn wants(tool: &str, args: Value, say: &str) -> String {
    json!({ "kind": "action", "say": say, "action": { "tool": tool, "args": args } }).to_string()
}

/// The decision envelope for a sentence.
#[must_use]
pub fn says(answer: &str) -> String {
    json!({ "kind": "answer", "answer": answer }).to_string()
}

/// Points the tenant's default AI provider at `base_url`.
///
/// The provider id carries the tenant, because these suites share one Postgres
/// and a provider id is unique across it.
pub async fn use_model(h: &Harness, base_url: &str) {
    let id = format!("ai-{}", h.tenant.as_str());
    h.acc
        .upsert_ai_provider(
            &id,
            "openai",
            "scripted",
            base_url,
            "test-model",
            None,
            true,
        )
        .await
        .unwrap();
    h.acc.set_default_ai_provider(&id).await.unwrap();
}
