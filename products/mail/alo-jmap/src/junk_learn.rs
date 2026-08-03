//! Junk training: user verdicts teach the spam filter.
//!
//! When `Email/set` moves a message **into** the Junk folder that is a
//! spam report; moving it **out** is a ham report. Both feed Rspamd's
//! Bayes classifier through the controller's `/learnspam` and
//! `/learnham` endpoints, so the filter converges on each deployment's
//! real mail instead of shipping static rules only.
//!
//! Design constraints:
//! - **Best-effort, never blocking.** Learning runs in a spawned task
//!   after the move already succeeded; a scanner outage degrades
//!   training, never mail management. (Contrast with inbound scanning,
//!   which is deliberately fail-closed.)
//! - The message body goes to the same Rspamd instance that already
//!   scanned it at ingress — no new data exposure, and nothing is
//!   logged beyond sizes and verdicts (Law #1).
//! - The Bayes store is deployment-global (one classifier per host,
//!   like inbound scoring) — a small deployment's users train it
//!   together. Per-tenant classifiers are a later refinement.

use std::sync::Arc;
use std::time::Duration;

/// Environment variable naming the Rspamd **controller** URL
/// (`http://host:11334`); unset disables junk training.
pub const ENV_RSPAMD_URL: &str = "ALO_JMAP_RSPAMD_URL";
/// Environment variable for an optional controller password. Our
/// deployment authorizes by network position (`secure_ip` on the
/// private compose network) instead; other deployments may prefer a
/// password.
pub const ENV_RSPAMD_PASSWORD: &str = "ALO_JMAP_RSPAMD_PASSWORD";

/// Calls Rspamd's learn endpoints. Cheap to clone via [`Arc`].
pub struct JunkLearner {
    base: String,
    password: Option<String>,
    client: reqwest::Client,
}

impl JunkLearner {
    /// Builds a learner for the controller at `base` (scheme + host +
    /// port, no path).
    pub fn new(base: impl Into<String>, password: Option<String>) -> Option<Arc<Self>> {
        let base = base.into().trim_end_matches('/').to_owned();
        if !base.starts_with("http://") && !base.starts_with("https://") {
            tracing::error!(%base, "junk training disabled: rspamd URL must be http(s)");
            return None;
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .ok()?;
        Some(Arc::new(Self {
            base,
            password,
            client,
        }))
    }

    /// Builds from the environment, or `None` (training disabled).
    pub fn from_env() -> Option<Arc<Self>> {
        let base = std::env::var(ENV_RSPAMD_URL)
            .ok()
            .filter(|s| !s.is_empty())?;
        let learner = Self::new(base, std::env::var(ENV_RSPAMD_PASSWORD).ok());
        if learner.is_some() {
            tracing::info!("junk training enabled (rspamd learn on Junk moves)");
        }
        learner
    }

    /// Reports one message as spam (`true`) or ham (`false`).
    /// Best-effort: outcomes are logged, never returned — the mailbox
    /// operation this trails has already succeeded.
    pub async fn learn(&self, as_spam: bool, raw_message: Vec<u8>) {
        let endpoint = if as_spam { "learnspam" } else { "learnham" };
        let url = format!("{}/{endpoint}", self.base);
        let size = raw_message.len();
        let mut request = self.client.post(&url).body(raw_message);
        if let Some(password) = &self.password {
            request = request.header("Password", password);
        }
        match request.send().await {
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                // 200 = learned; 208 (or the matching error text) =
                // already learned as this class — success either way.
                if status.is_success() || status.as_u16() == 208 || body.contains("already learned")
                {
                    tracing::info!(spam = as_spam, size, "rspamd learned message");
                } else {
                    // The body is rspamd's error JSON, never message
                    // content; truncate defensively all the same.
                    let reason: String = body.chars().take(200).collect();
                    tracing::warn!(spam = as_spam, %status, reason, "rspamd learn rejected");
                }
            }
            Err(error) => {
                tracing::warn!(spam = as_spam, %error, "rspamd learn unreachable (training skipped)");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn from_parts_validates_scheme() {
        assert!(JunkLearner::new("http://rspamd:11334", None).is_some());
        assert!(JunkLearner::new("http://rspamd:11334/", None).is_some());
        assert!(JunkLearner::new("rspamd:11334", None).is_none());
        assert!(JunkLearner::new("ftp://x", None).is_none());
    }

    #[tokio::test]
    async fn posts_message_to_the_right_endpoint_with_password() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).into_owned();
            let body = br#"{"success":true}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
            sock.write_all(body).await.unwrap();
            req
        });
        let learner =
            JunkLearner::new(format!("http://{addr}"), Some("sekrit".to_owned())).unwrap();
        learner
            .learn(true, b"From: a@b\r\n\r\nspam body\r\n".to_vec())
            .await;
        let req = server.await.unwrap();
        assert!(req.starts_with("POST /learnspam HTTP/1.1"), "{req}");
        assert!(
            req.to_ascii_lowercase().contains("password: sekrit"),
            "{req}"
        );
        assert!(req.contains("spam body"), "{req}");
    }
}
