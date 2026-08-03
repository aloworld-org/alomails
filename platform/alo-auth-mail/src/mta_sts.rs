//! MTA-STS (RFC 8461) — the policy document and its DNS record.
//!
//! This module *renders* a validated policy; serving it over HTTPS on
//! `mta-sts.<domain>/.well-known/mta-sts.txt` and publishing the
//! `_mta-sts.<domain>` TXT record are the caller's/operator's job. The
//! policy `id` is derived from the policy content by default, so it
//! changes exactly when the policy changes (RFC 8461 §3.1).

use sha2::{Digest, Sha256};

/// Policy enforcement mode (RFC 8461 §3.2 `mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StsMode {
    /// Sending MTAs must refuse delivery to a non-matching/insecure MX.
    Enforce,
    /// Failures are reported (TLS-RPT) but delivery proceeds — the
    /// rollout mode.
    Testing,
    /// No active policy (used to unpublish; MX list not required).
    None,
}

impl StsMode {
    /// The `mode:` token.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enforce => "enforce",
            Self::Testing => "testing",
            Self::None => "none",
        }
    }

    /// Parses `enforce`/`testing`/`none` (case-insensitive).
    pub fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "enforce" => Some(Self::Enforce),
            "testing" => Some(Self::Testing),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// RFC 8461 §3.2: `max_age` is between 1 and 31557600 seconds (one
/// year). We recommend a week once a policy is stable.
const MAX_AGE_MIN: u32 = 1;
const MAX_AGE_MAX: u32 = 31_557_600;
/// RFC 8461 §3.1: the policy `id` is `1*32(ALPHA / DIGIT)`.
const ID_MAX_LEN: usize = 32;

/// Why a policy could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MtaStsError {
    /// A non-`none` policy needs at least one MX pattern.
    #[error("mta-sts policy in mode {mode} requires at least one mx pattern")]
    NoMx {
        /// The offending mode.
        mode: &'static str,
    },
    /// An MX pattern was empty or carried CR/LF/whitespace that would
    /// break the line-oriented policy format.
    #[error("mta-sts mx pattern is empty or contains whitespace")]
    BadMx,
    /// `max_age` outside the RFC 8461 §3.2 range.
    #[error("mta-sts max_age must be {MAX_AGE_MIN}..={MAX_AGE_MAX} seconds")]
    BadMaxAge,
    /// The explicit `id` was empty, too long, or not alphanumeric.
    #[error("mta-sts id must be 1..={ID_MAX_LEN} alphanumeric characters")]
    BadId,
}

/// A validated MTA-STS policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtaStsPolicy {
    mode: StsMode,
    mx: Vec<String>,
    max_age: u32,
    id: String,
}

impl MtaStsPolicy {
    /// Builds and validates a policy. When `id` is `None` it is derived
    /// deterministically from the policy content (mode + mx + max_age),
    /// so the DNS `id` rotates automatically on any policy change.
    ///
    /// # Errors
    /// [`MtaStsError`] when the MX list is required-but-empty or a
    /// field is out of range / malformed.
    pub fn new(
        mode: StsMode,
        mx: Vec<String>,
        max_age: u32,
        id: Option<String>,
    ) -> Result<Self, MtaStsError> {
        if !(MAX_AGE_MIN..=MAX_AGE_MAX).contains(&max_age) {
            return Err(MtaStsError::BadMaxAge);
        }
        for pattern in &mx {
            if pattern.is_empty() || pattern.chars().any(char::is_whitespace) {
                return Err(MtaStsError::BadMx);
            }
        }
        if mode != StsMode::None && mx.is_empty() {
            return Err(MtaStsError::NoMx {
                mode: mode.as_str(),
            });
        }
        let mx: Vec<String> = mx.into_iter().map(|m| m.to_ascii_lowercase()).collect();
        let id = match id {
            Some(id) => {
                let id = id.trim().to_owned();
                // RFC 8461 §3.1: only ALPHA/DIGIT — a `;` (graphic) would
                // otherwise start a spurious field in the TXT record.
                if id.is_empty()
                    || id.len() > ID_MAX_LEN
                    || !id.chars().all(|c| c.is_ascii_alphanumeric())
                {
                    return Err(MtaStsError::BadId);
                }
                id
            }
            None => derive_id(mode, &mx, max_age),
        };
        Ok(Self {
            mode,
            mx,
            max_age,
            id,
        })
    }

    /// The policy document served at
    /// `https://mta-sts.<domain>/.well-known/mta-sts.txt` (RFC 8461
    /// §3.2). Lines are CRLF-separated, one `mx:` line per pattern.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("version: STSv1\r\n");
        out.push_str(&format!("mode: {}\r\n", self.mode.as_str()));
        for pattern in &self.mx {
            out.push_str(&format!("mx: {pattern}\r\n"));
        }
        out.push_str(&format!("max_age: {}\r\n", self.max_age));
        out
    }

    /// The `_mta-sts.<domain>` TXT record value (RFC 8461 §3.1).
    pub fn dns_txt(&self) -> String {
        format!("v=STSv1; id={}", self.id)
    }

    /// The policy id (as it appears in the DNS record).
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Derives a stable 16-hex-character id from the policy content, so the
/// DNS `id` changes exactly when the policy would.
fn derive_id(mode: StsMode, mx: &[String], max_age: u32) -> String {
    let mut hasher = Sha256::new();
    hasher.update(mode.as_str().as_bytes());
    hasher.update(b"\0");
    for pattern in mx {
        hasher.update(pattern.as_bytes());
        hasher.update(b"\0");
    }
    hasher.update(max_age.to_le_bytes());
    let digest = hasher.finalize();
    let mut id = String::with_capacity(16);
    for byte in &digest[..8] {
        id.push_str(&format!("{byte:02x}"));
    }
    id
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn renders_rfc8461_policy() {
        let policy = MtaStsPolicy::new(
            StsMode::Enforce,
            vec!["mx1.example.com".to_owned(), "*.example.com".to_owned()],
            604_800,
            Some("20260727a".to_owned()),
        )
        .unwrap();
        assert_eq!(
            policy.render(),
            "version: STSv1\r\nmode: enforce\r\nmx: mx1.example.com\r\nmx: *.example.com\r\nmax_age: 604800\r\n"
        );
        assert_eq!(policy.dns_txt(), "v=STSv1; id=20260727a");
    }

    #[test]
    fn derived_id_changes_with_policy() {
        let a = MtaStsPolicy::new(
            StsMode::Enforce,
            vec!["mx.example".to_owned()],
            86_400,
            None,
        )
        .unwrap();
        let b = MtaStsPolicy::new(
            StsMode::Enforce,
            vec!["mx.example".to_owned()],
            86_400,
            None,
        )
        .unwrap();
        // Deterministic for identical policy...
        assert_eq!(a.id(), b.id());
        assert_eq!(a.id().len(), 16);
        // ...and different when the policy differs.
        let c = MtaStsPolicy::new(
            StsMode::Enforce,
            vec!["mx2.example".to_owned()],
            86_400,
            None,
        )
        .unwrap();
        assert_ne!(a.id(), c.id());
        let d = MtaStsPolicy::new(
            StsMode::Testing,
            vec!["mx.example".to_owned()],
            86_400,
            None,
        )
        .unwrap();
        assert_ne!(a.id(), d.id());
    }

    #[test]
    fn mode_none_needs_no_mx_but_others_do() {
        assert!(MtaStsPolicy::new(StsMode::None, vec![], 86_400, None).is_ok());
        assert_eq!(
            MtaStsPolicy::new(StsMode::Enforce, vec![], 86_400, None),
            Err(MtaStsError::NoMx { mode: "enforce" })
        );
    }

    #[test]
    fn rejects_out_of_range_and_malformed() {
        assert_eq!(
            MtaStsPolicy::new(StsMode::Enforce, vec!["mx.example".to_owned()], 0, None),
            Err(MtaStsError::BadMaxAge)
        );
        assert_eq!(
            MtaStsPolicy::new(
                StsMode::Enforce,
                vec!["mx.example".to_owned()],
                MAX_AGE_MAX + 1,
                None
            ),
            Err(MtaStsError::BadMaxAge)
        );
        // A CRLF-injecting mx pattern is rejected (no policy-line forgery).
        assert_eq!(
            MtaStsPolicy::new(
                StsMode::Enforce,
                vec!["mx.example\r\nmode: none".to_owned()],
                86_400,
                None
            ),
            Err(MtaStsError::BadMx)
        );
        assert_eq!(
            MtaStsPolicy::new(
                StsMode::Enforce,
                vec!["mx.example".to_owned()],
                86_400,
                Some(String::new())
            ),
            Err(MtaStsError::BadId)
        );
        // A `;` in the id would forge a second TXT field (RFC 8461 §3.1
        // restricts the id to ALPHA/DIGIT).
        assert_eq!(
            MtaStsPolicy::new(
                StsMode::Enforce,
                vec!["mx.example".to_owned()],
                86_400,
                Some("a;b".to_owned())
            ),
            Err(MtaStsError::BadId)
        );
    }

    #[test]
    fn mode_parses_case_insensitively() {
        assert_eq!(StsMode::parse("Enforce"), Some(StsMode::Enforce));
        assert_eq!(StsMode::parse(" testing "), Some(StsMode::Testing));
        assert_eq!(StsMode::parse("none"), Some(StsMode::None));
        assert_eq!(StsMode::parse("bogus"), None);
    }
}
