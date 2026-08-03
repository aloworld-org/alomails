//! `Authentication-Results` (RFC 8601) — the single header that records
//! every authentication verdict (SPF, DKIM, DMARC, ARC, spam). This is
//! a **public contract** from M4 on: downstream (store, JMAP, web UI)
//! parses THIS header, not our internal types. One `authserv-id` per
//! deployment; format changes are additive only.

use crate::dkim::DkimVerdict;
use crate::dmarc::DmarcVerdict;
use crate::spf::SpfVerdict;

/// One `method=result` clause with its properties (§2.2).
#[derive(Debug, Clone)]
pub struct MethodResult {
    /// Method name: `spf`, `dkim`, `dmarc`, `arc`, `x-spam`.
    pub method: String,
    /// Result token: `pass`, `fail`, `none`, …
    pub result: String,
    /// `key.type=value` properties, e.g. `smtp.mailfrom=a@b`.
    pub properties: Vec<(String, String)>,
    /// Optional free-text reason/comment.
    pub reason: Option<String>,
}

impl MethodResult {
    fn new(method: &str, result: &str) -> Self {
        Self {
            method: method.to_owned(),
            result: result.to_owned(),
            properties: Vec::new(),
            reason: None,
        }
    }

    fn prop(mut self, key: &str, value: &str) -> Self {
        self.properties
            .push((key.to_owned(), sanitize_pvalue(value)));
        self
    }
}

/// Accumulates verdicts and renders the header value under one
/// `authserv-id` (the receiving host's identity, §2.4).
pub struct AuthenticationResults {
    authserv_id: String,
    methods: Vec<MethodResult>,
}

impl AuthenticationResults {
    /// Creates a builder for the given `authserv-id` (our hostname).
    pub fn new(authserv_id: impl Into<String>) -> Self {
        Self {
            authserv_id: authserv_id.into(),
            methods: Vec::new(),
        }
    }

    /// Records the SPF verdict with its `smtp.mailfrom`/`smtp.helo`
    /// identity (§2.7.2).
    pub fn spf(mut self, verdict: &SpfVerdict, identity_key: &str, identity: &str) -> Self {
        self.methods
            .push(MethodResult::new("spf", verdict.result.as_str()).prop(identity_key, identity));
        self
    }

    /// Records one DKIM signature verdict (§2.7.1). Call once per sig.
    pub fn dkim(mut self, verdict: &DkimVerdict) -> Self {
        let mut m =
            MethodResult::new("dkim", verdict.result.as_str()).prop("header.d", &verdict.domain);
        if !verdict.selector.is_empty() {
            m = m.prop("header.s", &verdict.selector);
        }
        self.methods.push(m);
        self
    }

    /// Records the DMARC verdict, with the From-domain and applied
    /// policy (§2.7.4 via RFC 7489 §7.1).
    pub fn dmarc(mut self, verdict: &DmarcVerdict) -> Self {
        self.methods.push(
            MethodResult::new("dmarc", verdict.result.as_str())
                .prop("header.from", &verdict.from_domain),
        );
        self
    }

    /// Records a spam-filter (Rspamd) verdict as an `x-spam` method
    /// (non-IANA experimental method name, §2.3).
    pub fn spam(mut self, result: &str, score: Option<f64>) -> Self {
        let mut m = MethodResult::new("x-spam", result);
        if let Some(score) = score {
            m = m.prop("policy.score", &format!("{score:.2}"));
        }
        self.methods.push(m);
        self
    }

    /// Records an ARC chain-validation result (§2.7.6).
    pub fn arc(mut self, result: &str) -> Self {
        self.methods.push(MethodResult::new("arc", result));
        self
    }

    /// Renders the header *value* (no `Authentication-Results:` name,
    /// no trailing CRLF). When no method ran, emits `authserv-id; none`
    /// (§2.2). Clauses are separated by `;` and folded onto their own
    /// indented lines so the header stays within line limits.
    pub fn render(&self) -> String {
        let id = sanitize_value(&self.authserv_id);
        if self.methods.is_empty() {
            return format!("{id}; none");
        }
        let mut out = String::from(&id);
        for method in &self.methods {
            out.push_str(";\r\n\t");
            out.push_str(&method.method);
            out.push('=');
            out.push_str(&method.result);
            if let Some(reason) = &method.reason {
                out.push_str(&format!(" reason=\"{}\"", sanitize_value(reason)));
            }
            for (key, value) in &method.properties {
                out.push(' ');
                out.push_str(key);
                out.push('=');
                out.push_str(value);
            }
        }
        out
    }
}

/// Strips CR/LF and other control characters from free text (reason,
/// authserv-id) before it goes into the header.
fn sanitize_value(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
}

/// Sanitizes an attacker-influenced *property value* (`header.d`,
/// `smtp.mailfrom`, `header.from`, …). Beyond stripping control chars,
/// it drops the structural characters — SP, `=`, `;`, `"`, parens —
/// that a crafted DKIM `d=`/`s=` or envelope address could otherwise
/// use to inject a forged clause or property into the RFC 8601 header
/// (e.g. `d=evil header.from=bank.com`). Keeps only the dot-atom /
/// addr-spec character set that legitimate domains and addresses use.
fn sanitize_pvalue(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '@' | '+'))
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::dkim::DkimResult;
    use crate::dmarc::DmarcResult;
    use crate::spf::SpfResult;

    #[test]
    fn renders_multi_method_header() {
        let spf = SpfVerdict {
            result: SpfResult::Pass,
            domain: "example.com".to_owned(),
            explanation: String::new(),
        };
        let dkim = DkimVerdict {
            result: DkimResult::Pass,
            domain: "example.com".to_owned(),
            selector: "sel".to_owned(),
        };
        let dmarc = DmarcVerdict {
            result: DmarcResult::Pass,
            from_domain: "example.com".to_owned(),
            disposition: crate::dmarc::Disposition::None,
            pct: 100,
            spf_aligned: true,
            dkim_aligned: true,
        };
        let header = AuthenticationResults::new("mx.alo.test")
            .spf(&spf, "smtp.mailfrom", "bob@example.com")
            .dkim(&dkim)
            .dmarc(&dmarc)
            .spam("no", Some(1.5))
            .render();
        assert!(header.starts_with("mx.alo.test;"));
        assert!(header.contains("spf=pass smtp.mailfrom=bob@example.com"));
        assert!(header.contains("dkim=pass header.d=example.com header.s=sel"));
        assert!(header.contains("dmarc=pass header.from=example.com"));
        assert!(header.contains("x-spam=no policy.score=1.50"));
    }

    #[test]
    fn empty_is_none() {
        assert_eq!(
            AuthenticationResults::new("mx.alo.test").render(),
            "mx.alo.test; none"
        );
    }

    #[test]
    fn structural_injection_into_properties_is_stripped() {
        let spf = SpfVerdict {
            result: SpfResult::Pass,
            domain: "x".to_owned(),
            explanation: String::new(),
        };
        // CRLF, spaces, and `=` in a property value must not forge a new
        // header, clause, or property in the RFC 8601 contract.
        let header = AuthenticationResults::new("mx.alo.test")
            .spf(
                &spf,
                "smtp.mailfrom",
                "a@b\r\nEvil: header dkim=pass header.d=bank.com",
            )
            .render();
        assert!(!header.contains("\r\nEvil:"), "no injected header");
        assert!(
            !header.contains(" dkim=pass"),
            "no injected property/clause"
        );
        assert!(header.contains("smtp.mailfrom=a@bEvilheaderdkimpassheader.dbank.com"));
    }
}
