//! SMTP reply construction and wire formatting (RFC 5321 §4.2).
//!
//! Every reply the server can send is built here, so reply codes have
//! exactly one source and each carries its RFC citation. Replies are
//! one or more lines sharing a code; multiline replies (EHLO
//! capabilities) render per §4.2.1 (`code-` on every line but the
//! last, `code SP` on the last).

use std::fmt;

/// An SMTP reply: a three-digit code and one or more text lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    code: u16,
    lines: Vec<String>,
}

impl Reply {
    /// A single-line reply.
    fn line(code: u16, text: impl Into<String>) -> Self {
        Self {
            code,
            lines: vec![text.into()],
        }
    }

    /// 220: service ready — the opening banner (RFC 5321 §3.1, §4.2.3).
    pub fn service_ready(hostname: &str) -> Self {
        Self::line(220, format!("{hostname} ESMTP alo"))
    }

    /// 250: EHLO accepted with advertised capabilities (RFC 5321
    /// §4.1.1.1, §4.2.1). The first line greets the client; each
    /// capability is its own continuation line. Only truthfully
    /// implemented, currently-usable capabilities are passed in.
    pub fn ehlo(hostname: &str, client: &str, capabilities: &[String]) -> Self {
        let mut lines = Vec::with_capacity(capabilities.len() + 1);
        lines.push(format!("{hostname} greets {client}"));
        lines.extend(capabilities.iter().cloned());
        Self { code: 250, lines }
    }

    /// 250: HELO accepted (RFC 5321 §4.1.1.1 — the reply carries only
    /// our domain for pre-ESMTP clients).
    pub fn helo_ok(hostname: &str) -> Self {
        Self::line(250, hostname.to_owned())
    }

    /// 250: generic success for MAIL/RCPT/RSET/NOOP (§4.1.1).
    pub fn ok() -> Self {
        Self::line(250, "OK")
    }

    /// 250: message accepted and durably spooled, with its id.
    pub fn ok_queued(id: &str) -> Self {
        Self::line(250, format!("OK: queued as {id}"))
    }

    /// 354: start mail input (RFC 5321 §4.1.1.4).
    pub fn start_mail_input() -> Self {
        Self::line(354, "Start mail input; end with <CRLF>.<CRLF>")
    }

    /// 220: ready to begin TLS (RFC 3207 §4). After this the server
    /// performs the handshake.
    pub fn tls_ready() -> Self {
        Self::line(220, "2.0.0 Ready to start TLS")
    }

    /// 454: TLS not available right now (RFC 3207 §4) — sent when
    /// STARTTLS is requested but no certificate is configured.
    pub fn tls_unavailable() -> Self {
        Self::line(454, "4.7.0 TLS not available")
    }

    /// 334: SASL continuation challenge (RFC 4954 §4). `data` is the
    /// base64 challenge (may be empty).
    pub fn auth_challenge(data: &str) -> Self {
        Self::line(334, data.to_owned())
    }

    /// 235: authentication succeeded (RFC 4954 §6).
    pub fn auth_ok() -> Self {
        Self::line(235, "2.7.0 Authentication successful")
    }

    /// 535: authentication credentials invalid (RFC 4954 §6). The same
    /// reply for wrong password and unknown user (anti-enumeration).
    pub fn auth_failed() -> Self {
        Self::line(535, "5.7.8 Authentication credentials invalid")
    }

    /// 501: malformed SASL/base64 in an AUTH exchange (RFC 4954 §4).
    pub fn auth_malformed() -> Self {
        Self::line(501, "5.5.2 Cannot decode authentication exchange")
    }

    /// 454: the credential authority is temporarily unavailable (RFC 4954
    /// §6) — a store fault, not a credential rejection, so the client may
    /// retry rather than treat the login as wrong.
    pub fn auth_temporary_failure() -> Self {
        Self::line(454, "4.7.0 Temporary authentication failure")
    }

    /// 504: unrecognized AUTH mechanism (RFC 4954 §4).
    pub fn auth_mechanism_unsupported() -> Self {
        Self::line(504, "5.5.4 Unrecognized authentication mechanism")
    }

    /// 538: encryption required for this mechanism (RFC 4954 §4) —
    /// AUTH attempted before STARTTLS.
    pub fn auth_encryption_required() -> Self {
        Self::line(
            538,
            "5.7.11 Encryption required for requested authentication mechanism",
        )
    }

    /// 530: authentication required (RFC 4954 §6) — MAIL on a
    /// submission port before a successful AUTH.
    pub fn auth_required() -> Self {
        Self::line(530, "5.7.1 Authentication required")
    }

    /// 530: STARTTLS required first (RFC 3207) — MAIL on a submission
    /// port that mandates TLS before the connection is encrypted.
    pub fn starttls_required() -> Self {
        Self::line(530, "5.7.0 Must issue a STARTTLS command first")
    }

    /// 550: relaying denied — a `RCPT TO:` on the MX port for a domain
    /// this server does not host (RFC 5321 §7.2 anti-open-relay).
    pub fn relay_denied() -> Self {
        Self::line(550, "5.7.1 Relaying denied: recipient not local")
    }

    /// 550 5.1.1: the recipient is in a hosted domain but no such mailbox
    /// exists (RFC 3463 §3.2). Refused at RCPT so a sender gets an honest,
    /// immediate answer and no mail is silently dropped (the recipient-
    /// enumeration trade-off is documented in `docs/interop.md`).
    pub fn no_such_user() -> Self {
        Self::line(550, "5.1.1 No such user here")
    }

    /// 451 4.3.0: a transient failure delivering into the store — the
    /// message is NOT accepted, so the sender retries (no mail loss). Used
    /// when a recipient's store/blob write fails at DATA.
    pub fn delivery_tempfail() -> Self {
        Self::line(
            451,
            "4.3.0 Temporary failure delivering to mailbox, try again later",
        )
    }

    /// 550: rejected by DMARC policy (RFC 7489 — the sending domain
    /// publishes `p=reject` and the message failed authentication).
    pub fn dmarc_reject() -> Self {
        Self::line(550, "5.7.1 Message rejected by DMARC policy")
    }

    /// 550: rejected by the spam filter (Rspamd `reject` action).
    pub fn spam_reject() -> Self {
        Self::line(550, "5.7.1 Message rejected as spam")
    }

    /// 550: rejected by the malware scanner (a ClamAV signature
    /// matched). The signature name is pre-sanitized by the scan
    /// client before it can reach this reply.
    pub fn virus_reject(signature: &str) -> Self {
        Self::line(
            550,
            format!("5.7.1 Message rejected: malware detected ({signature})"),
        )
    }

    /// 451: temporarily deferred by the spam filter — Rspamd asked to
    /// soft-reject/greylist, or the scanner was unreachable and policy
    /// is fail-closed. Transient so a legitimate sender retries.
    pub fn spam_tempfail() -> Self {
        Self::line(451, "4.7.1 Message temporarily deferred, try again later")
    }

    /// 421: too many failed authentication attempts; the server is
    /// closing the connection (RFC 4954 anti-brute-force).
    pub fn too_many_auth_failures(hostname: &str) -> Self {
        Self::line(
            421,
            format!("{hostname} too many authentication failures, closing connection"),
        )
    }

    /// 503: bad sequence of commands (RFC 5321 §4.1.4).
    pub fn bad_sequence(hint: &str) -> Self {
        Self::line(503, format!("bad sequence of commands: {hint}"))
    }

    /// 555: MAIL/RCPT parameters not recognized or not implemented
    /// (RFC 5321 §4.1.1.11) — sent because no extension is advertised.
    pub fn params_not_recognized() -> Self {
        Self::line(
            555,
            "MAIL FROM/RCPT TO parameters not recognized or not implemented",
        )
    }

    /// 452: too many recipients (RFC 5321 §4.5.3.1.10 — transient by
    /// design so the client retries the rest in a new transaction).
    pub fn too_many_recipients() -> Self {
        Self::line(452, "too many recipients")
    }

    /// 552: message exceeds the fixed maximum message size
    /// (RFC 1870 semantics; the limit is enforced during read).
    pub fn message_too_large() -> Self {
        Self::line(552, "message exceeds fixed maximum message size")
    }

    /// 502: command recognized but not implemented (RFC 5321 §4.2.4).
    pub fn not_implemented() -> Self {
        Self::line(502, "command not implemented")
    }

    /// 252: VRFY answered without disclosing user existence
    /// (RFC 5321 §3.5.3, §7.3 — anti-enumeration).
    pub fn vrfy_noncommittal() -> Self {
        Self::line(
            252,
            "cannot VRFY user, but will accept message and attempt delivery",
        )
    }

    /// 451: local error in processing (RFC 5321 §4.2.4) — transient,
    /// used when the spool write fails so the client retries.
    pub fn local_error() -> Self {
        Self::line(451, "local error in processing, try again later")
    }

    /// 221: closing transmission channel, response to QUIT
    /// (RFC 5321 §4.1.1.10).
    pub fn closing(hostname: &str) -> Self {
        Self::line(221, format!("{hostname} closing transmission channel"))
    }

    /// 500: syntax error, command unrecognized (RFC 5321 §4.2.4).
    pub fn command_unrecognized() -> Self {
        Self::line(500, "syntax error, command unrecognized")
    }

    /// 500: command line exceeded the 512-octet limit of
    /// RFC 5321 §4.5.3.1.4.
    pub fn line_too_long() -> Self {
        Self::line(500, "line too long")
    }

    /// 500: line ending was not CRLF. RFC 5321 §2.3.8 requires CRLF;
    /// accepting bare LF/CR enables SMTP smuggling, so we reject
    /// rather than guess (protocol skill: reject when ambiguity has
    /// security consequences).
    pub fn bare_line_ending() -> Self {
        Self::line(500, "line ending must be CRLF")
    }

    /// 501: syntax error in parameters or arguments (RFC 5321 §4.2.4).
    pub fn parameter_error() -> Self {
        Self::line(501, "syntax error in parameters or arguments")
    }

    /// 421: service closing — sent when the server must drop the
    /// connection (idle timeout per RFC 5321 §4.5.3.2, or flooding).
    pub fn service_closing(hostname: &str) -> Self {
        Self::line(
            421,
            format!("{hostname} service closing transmission channel"),
        )
    }

    /// The reply's three-digit code.
    pub fn code(&self) -> u16 {
        self.code
    }
}

impl fmt::Display for Reply {
    /// Wire form (RFC 5321 §4.2, §4.2.1): every line but the last is
    /// `code-text CRLF`; the last is `code SP text CRLF`. CRLF always.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `saturating_sub` guards the (constructor-guaranteed) invariant
        // of ≥1 line so this public `Display` can never underflow-panic.
        let last = self.lines.len().saturating_sub(1);
        for (i, line) in self.lines.iter().enumerate() {
            let sep = if i == last { ' ' } else { '-' };
            write!(f, "{}{sep}{line}\r\n", self.code)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_wire_form_ends_with_crlf() {
        let wire = Reply::service_ready("mx.example").to_string();
        assert_eq!(wire, "220 mx.example ESMTP alo\r\n");
    }

    #[test]
    fn multiline_ehlo_uses_dash_then_space() {
        // RFC 5321 §4.2.1: continuations use `code-`, the last `code `.
        let caps = vec!["STARTTLS".to_owned(), "SIZE 1000".to_owned()];
        let wire = Reply::ehlo("mx.example", "client", &caps).to_string();
        assert_eq!(
            wire,
            "250-mx.example greets client\r\n250-STARTTLS\r\n250 SIZE 1000\r\n"
        );
    }

    #[test]
    fn ehlo_with_no_capabilities_is_single_line() {
        let wire = Reply::ehlo("mx.example", "client", &[]).to_string();
        assert_eq!(wire, "250 mx.example greets client\r\n");
    }

    #[test]
    fn codes_match_rfc() {
        assert_eq!(Reply::service_ready("h").code(), 220);
        assert_eq!(Reply::ehlo("h", "c", &[]).code(), 250);
        assert_eq!(Reply::tls_ready().code(), 220);
        assert_eq!(Reply::tls_unavailable().code(), 454);
        assert_eq!(Reply::auth_challenge("").code(), 334);
        assert_eq!(Reply::auth_ok().code(), 235);
        assert_eq!(Reply::auth_failed().code(), 535);
        assert_eq!(Reply::auth_encryption_required().code(), 538);
        assert_eq!(Reply::auth_required().code(), 530);
        assert_eq!(Reply::closing("h").code(), 221);
        assert_eq!(Reply::parameter_error().code(), 501);
        assert_eq!(Reply::service_closing("h").code(), 421);
    }
}
