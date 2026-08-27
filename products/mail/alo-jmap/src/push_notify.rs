//! The Web Push dispatcher (mail M5.3): turns the same `StateChange` signals
//! the EventSource stream carries into encrypted pushes to the browser
//! installations a user opted in — so a CLOSED app can still say "something
//! arrived". Fed by the [`crate::push::PushHub`] tap, reads subscriptions
//! through the tenant door, and delivers with the crypto in
//! [`crate::web_push`].
//!
//! What a payload is allowed to carry is decided here, once: the RFC 8620
//! `StateChange` object — type names, the account id, an opaque state
//! string. Never a subject line, never a body, never an address; the
//! notification the service worker shows is generic, and the app fetches
//! real data itself when opened. A push endpoint URL is a capability —
//! logged nowhere, not even at debug.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alo_store::{Store, TenantId, UserId};

use crate::push::{PushHub, StateChangeMsg, state_change_json};
use crate::web_push::VapidKeys;

/// The quiet window per `(tenant, user)`: one push per half minute is
/// plenty for "open the app" — the payload carries no detail that could go
/// stale, push services collapse queued messages by `Topic` anyway, and the
/// window bounds how many outbound POSTs one busy mailbox can cause.
const THROTTLE: Duration = Duration::from_secs(30);

/// How long the push service should keep an undeliverable message (`TTL`),
/// in seconds: a day. A wake-up older than that is noise.
const TTL_SECS: u32 = 86_400;

/// The Web Push sending half: the VAPID identity and the HTTP client that
/// delivers to push services. Built once at startup; `None` in `AppState`
/// means the deployment has no VAPID key and the whole feature is dark.
pub struct WebPush {
    keys: VapidKeys,
    http: reqwest::Client,
}

impl WebPush {
    /// Builds the sender from `ALO_VAPID_KEY` (base64url PKCS#8, minted by
    /// `--generate-vapid-key`) + `ALO_VAPID_SUBJECT` (a `mailto:` contact).
    /// Both or nothing, like the media engine: a half-configured sender
    /// produces refusals harder to diagnose than an absent one — and a
    /// MALFORMED key is loudly refused rather than silently dark.
    #[must_use]
    pub fn from_env() -> Option<Arc<Self>> {
        let key = std::env::var("ALO_VAPID_KEY")
            .ok()
            .filter(|v| !v.trim().is_empty())?;
        let subject = std::env::var("ALO_VAPID_SUBJECT")
            .ok()
            .filter(|v| !v.trim().is_empty())?;
        match VapidKeys::new(&key, &subject) {
            Ok(keys) => Some(Arc::new(Self::new(keys))),
            Err(error) => {
                tracing::error!(%error, "ALO_VAPID_KEY is set but unusable — web push disabled");
                None
            }
        }
    }

    /// A sender over explicit keys (tests point this at a local endpoint).
    #[must_use]
    pub fn new(keys: VapidKeys) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();
        Self { keys, http }
    }

    /// The key browsers subscribe with (`applicationServerKey`), base64url.
    #[must_use]
    pub fn public_key_b64(&self) -> &str {
        self.keys.public_key_b64()
    }
}

/// Whether the dispatcher may POST to this endpoint. Push services are
/// `https`; the one exception is loopback, so a local test stack can stand
/// in for one. The server fetching a URL a USER stored is exactly the shape
/// of SSRF, so everything else — plain http, internal hosts, other schemes —
/// is refused here regardless of what got past the create route.
pub(crate) fn endpoint_allowed(url: &reqwest::Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
        _ => false,
    }
}

/// `scheme://host[:port]` — the VAPID token audience (RFC 8292 §2).
fn origin_of(url: &reqwest::Url) -> String {
    match (url.port(), url.host_str()) {
        (Some(port), Some(host)) => format!("{}://{host}:{port}", url.scheme()),
        (None, Some(host)) => format!("{}://{host}", url.scheme()),
        _ => String::new(),
    }
}

/// Wires the dispatcher into `hub` and starts it: every published state
/// change fans out to the changed account's subscribed devices. Called once
/// at startup when a VAPID key is configured (and by the test harness with
/// a loopback sender).
pub fn wire(hub: &PushHub, store: Arc<Store>, web_push: Arc<WebPush>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    hub.set_tap(tx);
    tokio::spawn(run(rx, store, web_push));
}

/// The dispatch loop: throttle, read the account's devices, deliver to
/// each. Best-effort throughout — a push that cannot be delivered changes
/// nothing about mail, and the client refetches on next open anyway.
async fn run(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<(String, StateChangeMsg)>,
    store: Arc<Store>,
    web_push: Arc<WebPush>,
) {
    let mut last_sent: HashMap<(String, String), Instant> = HashMap::new();
    while let Some((tenant, msg)) = rx.recv().await {
        let key = (tenant.clone(), msg.account_id.clone());
        if let Some(at) = last_sent.get(&key)
            && at.elapsed() < THROTTLE
        {
            continue;
        }
        let deliveries = match store
            .for_tenant(TenantId::new(&tenant))
            .push_deliveries(&UserId::new(&msg.account_id))
            .await
        {
            Ok(d) => d,
            Err(_) => continue,
        };
        if deliveries.is_empty() {
            continue;
        }
        last_sent.insert(key, Instant::now());
        // The occasional sweep keeps the throttle map from growing with
        // every user who ever changed state.
        if last_sent.len() > 4096 {
            last_sent.retain(|_, at| at.elapsed() < THROTTLE);
        }
        let payload = state_change_json(&msg).to_string();
        for delivery in deliveries {
            send_one(&store, &web_push, &delivery, payload.as_bytes()).await;
        }
    }
}

/// Delivers one encrypted payload to one device, and drops the
/// subscription when the push service says it no longer exists (404/410).
/// Failures are logged by subscription id only — the endpoint URL is a
/// capability and stays out of logs.
async fn send_one(
    store: &Store,
    web_push: &WebPush,
    delivery: &alo_store::PushDelivery,
    payload: &[u8],
) {
    let Ok(url) = reqwest::Url::parse(&delivery.endpoint) else {
        // Not a URL at all: it can never deliver, so it is dead weight.
        let _ = store.drop_dead_push_subscription(&delivery.id).await;
        return;
    };
    if !endpoint_allowed(&url) {
        let _ = store.drop_dead_push_subscription(&delivery.id).await;
        return;
    }
    let Ok(authorization) = web_push.keys.authorization(&origin_of(&url)) else {
        return;
    };
    let Ok(body) = crate::web_push::encrypt(&delivery.p256dh, &delivery.auth, payload) else {
        // Key material that cannot encrypt will never start working.
        let _ = store.drop_dead_push_subscription(&delivery.id).await;
        return;
    };
    let sent = web_push
        .http
        .post(url)
        .header("authorization", authorization)
        .header("content-encoding", "aes128gcm")
        .header("content-type", "application/octet-stream")
        .header("ttl", TTL_SECS.to_string())
        .header("urgency", "normal")
        // One topic per purpose: queued wake-ups collapse into the newest
        // (RFC 8030 §5.4) — a phone offline overnight gets one, not forty.
        .header("topic", "alo-state")
        .body(body.to_vec())
        .send()
        .await;
    match sent {
        Ok(response) => {
            let status = response.status().as_u16();
            if status == 404 || status == 410 {
                let _ = store.drop_dead_push_subscription(&delivery.id).await;
            } else if !response.status().is_success() {
                tracing::debug!(id = %delivery.id, status, "push delivery refused");
            }
        }
        Err(_) => {
            tracing::debug!(id = %delivery.id, "push delivery failed");
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn only_https_or_loopback_http_endpoints_are_allowed() {
        let ok = |u: &str| endpoint_allowed(&reqwest::Url::parse(u).unwrap());
        assert!(ok("https://push.example/send/abc"));
        assert!(ok("http://127.0.0.1:9999/push"));
        assert!(ok("http://localhost:9999/push"));
        // Internal HTTP is exactly the SSRF shape this guard exists for.
        assert!(!ok("http://10.0.0.5/admin"));
        assert!(!ok("http://push.example/send/abc"));
        assert!(!ok("ftp://push.example/x"));
        assert!(!ok("file:///etc/passwd"));
    }

    #[test]
    fn the_vapid_audience_is_the_endpoint_origin() {
        let url = reqwest::Url::parse("https://push.example/send/abc?x=1").unwrap();
        assert_eq!(origin_of(&url), "https://push.example");
        let url = reqwest::Url::parse("http://127.0.0.1:9999/push/dev").unwrap();
        assert_eq!(origin_of(&url), "http://127.0.0.1:9999");
    }
}
