//! The SMTP session state machine — pure protocol logic, no I/O
//! (RFC 5321 §4.1.4 ordering, RFC 3207 STARTTLS, RFC 4954 AUTH,
//! RFC 6409 submission policy).
//!
//! [`Session`] consumes command lines and produces [`Directive`]s; the
//! transport ([`crate::server`]) performs the I/O they imply — the TLS
//! handshake, the SASL dialog, the DATA byte stream. Keeping all
//! policy here makes every security decision unit-testable.

use crate::address::{ForwardPath, ReversePath};
use crate::auth::{AuthIdentity, Mechanism};
use crate::command::{self, Command, CommandError};
use crate::reply::Reply;

/// The role of the listener a session arrived on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Inbound relay (port 25). No authentication; STARTTLS offered.
    Mx,
    /// Message submission (ports 587/465). AUTH required over TLS,
    /// RFC 6409 policy applies.
    Submission,
}

/// After the transport writes the reply, what it must do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Keep reading commands.
    Continue,
    /// Close the connection (QUIT).
    Close,
    /// Switch to DATA collection (the 354 reply is already returned).
    EnterData,
}

/// What [`Session::on_line`] asks the transport to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Directive {
    /// Write `reply`, then perform `action`.
    Respond(Reply, Action),
    /// Write the 220 greeting-to-TLS, perform the handshake, then
    /// reset the session (RFC 3207 §4.2).
    StartTls,
    /// Run the SASL dialog for `mechanism` (with an optional base64
    /// initial response), then report the result back to the session.
    Authenticate {
        /// Selected, validated mechanism.
        mechanism: Mechanism,
        /// Base64 initial response, if the client supplied one.
        initial: Option<String>,
    },
    /// Resolve a local recipient against the store: the transport looks up
    /// `email`, then calls [`Session::resolve_pending`] with the result.
    /// Emitted only when local delivery is configured.
    CheckRecipient {
        /// The recipient address (`local@domain` display form) to resolve.
        email: String,
    },
}

/// Immutable per-listener parameters for a session.
#[derive(Debug, Clone)]
pub struct SessionParams {
    /// Announced hostname.
    pub hostname: String,
    /// Recipient cap per transaction (≥ 100, §4.5.3.1.8).
    pub max_rcpt: usize,
    /// Advertised/enforced maximum message size (RFC 1870 SIZE).
    pub max_message_size: usize,
    /// Listener role.
    pub role: Role,
    /// Whether STARTTLS can be offered (a certificate is configured).
    pub tls_available: bool,
    /// Whether the connection is already encrypted (implicit TLS/465).
    pub tls_active: bool,
    /// Whether a successful AUTH is required before MAIL (submission).
    pub require_auth: bool,
    /// Whether TLS must be active before MAIL (submission).
    pub require_tls_before_mail: bool,
    /// Domains this server hosts (lowercased). On the MX role, when
    /// non-empty, `RCPT TO:` for any other domain is refused (550) —
    /// the anti-open-relay guard (RFC 5321 §7.2). Empty means accept
    /// all (development); production MUST set this before enabling
    /// outbound (enforced in `config`/`server::run`).
    pub local_domains: Vec<String>,
    /// Whether a local-domain `RCPT TO:` must be resolved against the store
    /// (local delivery configured): the session emits
    /// [`Directive::CheckRecipient`] instead of accepting outright, so an
    /// unknown local user is refused `550 5.1.1` at RCPT.
    pub resolve_local_recipients: bool,
}

/// Transaction state (§4.1.4).
#[derive(Debug)]
enum Txn {
    Idle,
    InProgress {
        from: ReversePath,
        rcpts: Vec<ForwardPath>,
    },
}

/// One SMTP session's protocol state.
#[derive(Debug)]
pub struct Session {
    params: SessionParams,
    /// EHLO/HELO argument once greeted.
    helo: Option<String>,
    /// Whether the greeting was EHLO (ESMTP) vs HELO (SMTP), RFC 3848.
    esmtp: bool,
    /// Whether TLS is currently active (implicit, or after STARTTLS).
    tls_active: bool,
    /// The authenticated identity, once AUTH succeeds.
    identity: Option<AuthIdentity>,
    txn: Txn,
    /// SMTPUTF8 (RFC 6531) — not advertised yet, always false.
    utf8_enabled: bool,
    /// A recipient awaiting store resolution (between a `CheckRecipient`
    /// directive and [`Session::resolve_pending`]).
    pending_rcpt: Option<ForwardPath>,
}

impl Session {
    /// Creates a session for the given listener parameters.
    pub fn new(params: SessionParams) -> Self {
        let tls_active = params.tls_active;
        Self {
            params,
            helo: None,
            esmtp: false,
            tls_active,
            identity: None,
            txn: Txn::Idle,
            utf8_enabled: false,
            pending_rcpt: None,
        }
    }

    /// The 220 greeting sent when the connection opens (§3.1).
    pub fn greeting(&self) -> Reply {
        Reply::service_ready(&self.params.hostname)
    }

    /// The capabilities to advertise in an EHLO reply — only those
    /// that are truthfully implemented AND usable in the current state.
    fn capabilities(&self) -> Vec<String> {
        let mut caps = Vec::new();
        if self.params.tls_available && !self.tls_active {
            caps.push("STARTTLS".to_owned());
        }
        // AUTH is offered only where it can be used: a submission role,
        // over TLS, not already authenticated (RFC 4954 §4).
        if self.params.role == Role::Submission && self.tls_active && self.identity.is_none() {
            caps.push("AUTH PLAIN LOGIN XOAUTH2".to_owned());
        }
        caps.push(format!("SIZE {}", self.params.max_message_size));
        caps.push("8BITMIME".to_owned());
        caps
    }

    /// Handles one complete command line (CRLF already stripped).
    pub fn on_line(&mut self, line: &str) -> Directive {
        let command = match command::parse(line, self.utf8_enabled) {
            Ok(command) => command,
            Err(CommandError::Empty) => return self.respond(Reply::command_unrecognized()),
            Err(CommandError::BadAddress(error)) => {
                tracing::debug!(%error, "address rejected");
                return self.respond(Reply::parameter_error());
            }
            Err(CommandError::MissingParameter { .. } | CommandError::BadParameter { .. }) => {
                return self.respond(Reply::parameter_error());
            }
        };

        match command {
            Command::Ehlo { client } => {
                self.txn = Txn::Idle;
                self.helo = Some(client.clone());
                self.esmtp = true;
                let caps = self.capabilities();
                Directive::Respond(
                    Reply::ehlo(&self.params.hostname, &client, &caps),
                    Action::Continue,
                )
            }
            Command::Helo { client } => {
                self.txn = Txn::Idle;
                self.helo = Some(client);
                self.esmtp = false;
                self.respond(Reply::helo_ok(&self.params.hostname))
            }
            Command::StartTls => self.on_starttls(),
            Command::Auth { mechanism, initial } => self.on_auth(&mechanism, initial),
            Command::Mail {
                reverse_path,
                params,
            } => self.on_mail(reverse_path, params),
            Command::Rcpt {
                forward_path,
                params,
            } => self.on_rcpt(forward_path, params),
            Command::Data => self.on_data(),
            Command::Rset => {
                self.txn = Txn::Idle;
                self.pending_rcpt = None;
                self.respond(Reply::ok())
            }
            Command::Noop => self.respond(Reply::ok()),
            Command::Vrfy => self.respond(Reply::vrfy_noncommittal()),
            Command::Quit => {
                Directive::Respond(Reply::closing(&self.params.hostname), Action::Close)
            }
            Command::NotImplemented { verb } => {
                tracing::debug!(%verb, "recognized but unimplemented command");
                self.respond(Reply::not_implemented())
            }
            Command::Unknown { verb } => {
                tracing::debug!(%verb, "unrecognized command");
                self.respond(Reply::command_unrecognized())
            }
        }
    }

    fn on_starttls(&mut self) -> Directive {
        if !self.params.tls_available {
            return self.respond(Reply::tls_unavailable());
        }
        if self.tls_active {
            return self.respond(Reply::bad_sequence("TLS already active"));
        }
        // The transport writes 220, handshakes, and calls
        // `reset_after_starttls` on success.
        Directive::StartTls
    }

    fn on_auth(&mut self, mechanism: &str, initial: Option<String>) -> Directive {
        if self.params.role != Role::Submission {
            return self.respond(Reply::bad_sequence("AUTH not offered on this port"));
        }
        // Most MSAs require EHLO before AUTH; require it so AUTH cannot
        // race the greeting.
        if self.helo.is_none() {
            return self.respond(Reply::bad_sequence("send EHLO first"));
        }
        if !self.tls_active {
            return self.respond(Reply::auth_encryption_required());
        }
        if self.identity.is_some() {
            return self.respond(Reply::bad_sequence("already authenticated"));
        }
        if !matches!(self.txn, Txn::Idle) {
            return self.respond(Reply::bad_sequence("AUTH not permitted mid-transaction"));
        }
        match Mechanism::parse(mechanism) {
            Some(mechanism) => Directive::Authenticate { mechanism, initial },
            None => self.respond(Reply::auth_mechanism_unsupported()),
        }
    }

    fn on_mail(&mut self, reverse_path: ReversePath, params: Option<String>) -> Directive {
        if self.helo.is_none() {
            return self.respond(Reply::bad_sequence("send EHLO or HELO first"));
        }
        // Submission policy gates BEFORE a transaction can begin.
        if self.params.role == Role::Submission {
            if self.params.require_tls_before_mail && !self.tls_active {
                return self.respond(Reply::starttls_required());
            }
            if self.params.require_auth && self.identity.is_none() {
                return self.respond(Reply::auth_required());
            }
            // Sender authorization (binding MAIL FROM to the
            // authenticated identity / a send-as permission model,
            // RFC 6409 §6.1) is deferred to alo-identity (M9): a
            // strict "sender == login" rule breaks legitimate shared
            // mailboxes and aliases, so it needs the real permission
            // model, not an interim approximation.
        }
        if !matches!(self.txn, Txn::Idle) {
            return self.respond(Reply::bad_sequence("MAIL transaction already in progress"));
        }
        if let Some(params) = params.as_deref()
            && let Err(reply) = self.check_mail_params(params)
        {
            return self.respond(reply);
        }
        self.txn = Txn::InProgress {
            from: reverse_path,
            rcpts: Vec::new(),
        };
        self.pending_rcpt = None;
        self.respond(Reply::ok())
    }

    /// Validates MAIL parameters against the extensions we advertise
    /// (SIZE, 8BITMIME, AUTH). Unknown params → 555 (§4.1.1.11).
    fn check_mail_params(&self, params: &str) -> Result<(), Reply> {
        for param in params.split(' ').filter(|p| !p.is_empty()) {
            let (key, value) = match param.split_once('=') {
                Some((k, v)) => (k.to_ascii_uppercase(), Some(v)),
                None => (param.to_ascii_uppercase(), None),
            };
            match key.as_str() {
                // RFC 1870: reject early if the declared size exceeds
                // our fixed maximum.
                "SIZE" => match value.and_then(|v| v.parse::<usize>().ok()) {
                    Some(n) if n > self.params.max_message_size => {
                        return Err(Reply::message_too_large());
                    }
                    Some(_) => {}
                    None => return Err(Reply::params_not_recognized()),
                },
                // RFC 6152: we accept 7-bit and 8-bit bodies.
                "BODY" => match value.map(|v| v.to_ascii_uppercase()) {
                    Some(ref v) if v == "7BIT" || v == "8BITMIME" => {}
                    _ => return Err(Reply::params_not_recognized()),
                },
                // RFC 4954 §5: a submission AUTH= parameter is accepted
                // and ignored (we do not implement trusted relaying).
                "AUTH" => {}
                _ => return Err(Reply::params_not_recognized()),
            }
        }
        Ok(())
    }

    fn on_rcpt(&mut self, forward_path: ForwardPath, params: Option<String>) -> Directive {
        // Anti-open-relay (RFC 5321 §7.2): on the MX role with a
        // configured local-domains allowlist, only accept recipients
        // in a hosted domain. Authenticated submission relays anywhere
        // (that is the point of submission), so this applies to MX
        // only. `<postmaster>` (no domain) is always accepted
        // (§4.1.1.3). This is enforced in code so the safety no longer
        // rests on outbound being off by default.
        if self.params.role == Role::Mx
            && !self.params.local_domains.is_empty()
            && !self.recipient_is_local(&forward_path)
        {
            return self.respond(Reply::relay_denied());
        }
        let max_rcpt = self.params.max_rcpt;
        let Txn::InProgress { rcpts, .. } = &mut self.txn else {
            return self.respond(Reply::bad_sequence("need MAIL before RCPT"));
        };
        // We advertise no RCPT extensions (no DSN), so any RCPT
        // parameter is unrecognized (§4.1.1.11).
        if params.is_some() {
            return self.respond(Reply::params_not_recognized());
        }
        if rcpts.len() >= max_rcpt {
            return self.respond(Reply::too_many_recipients());
        }
        // With local delivery configured, EVERY accepted recipient is
        // resolved against the store before acceptance (an unknown local
        // user → 550 5.1.1 here, not after DATA — and so it can never reach
        // DATA as an unresolvable recipient that would defer the whole
        // message). `<postmaster>` (§4.1.1.3) resolves as `postmaster@` the
        // first hosted domain — it needs a backing mailbox to receive.
        // Non-local recipients only reach here via authenticated submission
        // and are not resolved (that user relays anywhere).
        if self.params.resolve_local_recipients {
            let email = match &forward_path {
                ForwardPath::Mailbox(m)
                    if self
                        .params
                        .local_domains
                        .iter()
                        .any(|d| d == &m.host.to_string().to_ascii_lowercase()) =>
                {
                    Some(m.to_string())
                }
                ForwardPath::Postmaster => self
                    .params
                    .local_domains
                    .first()
                    .map(|d| format!("postmaster@{d}")),
                _ => None,
            };
            if let Some(email) = email {
                self.pending_rcpt = Some(forward_path);
                return Directive::CheckRecipient { email };
            }
        }
        rcpts.push(forward_path);
        self.respond(Reply::ok())
    }

    /// Completes a [`Directive::CheckRecipient`]: with `accepted`, the
    /// pending recipient joins the transaction (`250`); otherwise it is
    /// dropped (`550 5.1.1`). A pending recipient with no in-progress
    /// transaction (RSET raced in) is a bad sequence.
    pub fn resolve_pending(&mut self, accepted: bool) -> Reply {
        let Some(path) = self.pending_rcpt.take() else {
            return Reply::bad_sequence("no recipient awaiting resolution");
        };
        if !accepted {
            return Reply::no_such_user();
        }
        match &mut self.txn {
            Txn::InProgress { rcpts, .. } => {
                rcpts.push(path);
                Reply::ok()
            }
            Txn::Idle => Reply::bad_sequence("need MAIL before RCPT"),
        }
    }

    /// Whether a recipient is in a hosted domain (or is the domainless
    /// `<postmaster>`, always local). Comparison is case-insensitive.
    fn recipient_is_local(&self, forward_path: &ForwardPath) -> bool {
        match forward_path {
            ForwardPath::Postmaster => true,
            ForwardPath::Mailbox(m) => {
                let host = m.host.to_string().to_ascii_lowercase();
                self.params.local_domains.iter().any(|d| d == &host)
            }
        }
    }

    fn on_data(&mut self) -> Directive {
        match &self.txn {
            Txn::InProgress { rcpts, .. } if !rcpts.is_empty() => {
                Directive::Respond(Reply::start_mail_input(), Action::EnterData)
            }
            Txn::InProgress { .. } => {
                self.respond(Reply::bad_sequence("need at least one RCPT before DATA"))
            }
            Txn::Idle => self.respond(Reply::bad_sequence("need MAIL before DATA")),
        }
    }

    fn respond(&self, reply: Reply) -> Directive {
        Directive::Respond(reply, Action::Continue)
    }

    /// Resets protocol state after a successful STARTTLS handshake
    /// (RFC 3207 §4.2): the server MUST discard any knowledge obtained
    /// from the client before TLS. The client must EHLO again.
    pub fn reset_after_starttls(&mut self) {
        self.pending_rcpt = None;
        self.helo = None;
        self.esmtp = false;
        self.tls_active = true;
        self.txn = Txn::Idle;
        // Identity is cleared too: nothing before TLS is trusted.
        self.identity = None;
    }

    /// Records a successful authentication.
    pub fn set_authenticated(&mut self, identity: AuthIdentity) {
        self.identity = Some(identity);
    }

    /// The authenticated login name, if any (for tracing/tenant seam).
    pub fn authenticated_username(&self) -> Option<&str> {
        self.identity.as_ref().map(|i| i.username.as_str())
    }

    /// The HELO/EHLO identity, for the `Received:` stamp.
    pub fn helo_client(&self) -> &str {
        self.helo.as_deref().unwrap_or("unknown")
    }

    /// WITH-clause protocol name for the `Received:` stamp
    /// (RFC 3848: `ESMTP`/`ESMTPS` after EHLO, `SMTP` after HELO;
    /// the `S` suffix marks a TLS-protected session).
    pub fn protocol_name(&self) -> &'static str {
        match (self.esmtp, self.tls_active) {
            (true, true) => "ESMTPS",
            (true, false) => "ESMTP",
            (false, _) => "SMTP",
        }
    }

    /// Envelope fields of the in-flight transaction: sender display
    /// form (`None` = null path) and recipient display forms. `None`
    /// when no transaction is in progress.
    pub fn envelope_fields(&self) -> Option<(Option<String>, Vec<String>)> {
        match &self.txn {
            Txn::InProgress { from, rcpts } => {
                let from = match from {
                    ReversePath::Null => None,
                    ReversePath::Mailbox(m) => Some(m.to_string()),
                };
                let rcpts = rcpts
                    .iter()
                    .map(|r| match r {
                        ForwardPath::Postmaster => "postmaster".to_owned(),
                        ForwardPath::Mailbox(m) => m.to_string(),
                    })
                    .collect();
                Some((from, rcpts))
            }
            Txn::Idle => None,
        }
    }

    /// Ends the DATA phase (success or failure): the transaction is
    /// consumed either way (§4.1.1.4).
    pub fn end_data(&mut self) {
        self.pending_rcpt = None;
        self.txn = Txn::Idle;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn params(role: Role) -> SessionParams {
        SessionParams {
            hostname: "mx.alo.test".to_owned(),
            max_rcpt: 100,
            max_message_size: 25 * 1024 * 1024,
            role,
            tls_available: true,
            tls_active: false,
            require_auth: role == Role::Submission,
            require_tls_before_mail: role == Role::Submission,
            // Most tests exercise policy unrelated to the relay guard;
            // the dedicated relay tests set this explicitly.
            local_domains: Vec::new(),
            resolve_local_recipients: false,
        }
    }

    /// Extracts the reply from a Respond directive (test helper).
    fn reply(d: &Directive) -> &Reply {
        match d {
            Directive::Respond(reply, _) => reply,
            other => panic!("expected Respond, got {other:?}"),
        }
    }

    fn code(d: &Directive) -> u16 {
        reply(d).code()
    }

    #[test]
    fn local_delivery_resolves_recipients_including_postmaster_at_rcpt() {
        // With local delivery on, a local mailbox and <postmaster> both emit
        // CheckRecipient (resolved at RCPT) rather than an outright 250 — so
        // no accepted recipient can turn out unresolvable at DATA.
        let mut p = params(Role::Mx);
        p.local_domains = vec!["alo.test".to_owned()];
        p.resolve_local_recipients = true;
        let mut s = Session::new(p);
        s.on_line("EHLO c.example");
        s.on_line("MAIL FROM:<a@ext.test>");
        match s.on_line("RCPT TO:<alice@alo.test>") {
            Directive::CheckRecipient { email } => assert_eq!(email, "alice@alo.test"),
            other => panic!("expected CheckRecipient, got {other:?}"),
        }
        match s.on_line("RCPT TO:<postmaster>") {
            Directive::CheckRecipient { email } => assert_eq!(email, "postmaster@alo.test"),
            other => panic!("expected CheckRecipient for postmaster, got {other:?}"),
        }
        // Resolution result drives the reply: found → 250, unknown → 550 5.1.1.
        assert_eq!(s.resolve_pending(true).code(), 250);
    }

    #[test]
    fn mx_ehlo_advertises_starttls_and_size_but_not_auth() {
        let mut s = Session::new(params(Role::Mx));
        let d = s.on_line("EHLO client.example");
        let wire = reply(&d).to_string();
        assert!(wire.contains("STARTTLS"));
        assert!(wire.contains("SIZE "));
        assert!(wire.contains("8BITMIME"));
        assert!(!wire.contains("AUTH"), "AUTH must not appear on an MX port");
    }

    #[test]
    fn submission_advertises_auth_only_after_tls() {
        let mut s = Session::new(params(Role::Submission));
        let before = s.on_line("EHLO client.example");
        assert!(
            !reply(&before).to_string().contains("AUTH"),
            "AUTH must not be offered before TLS"
        );
        // Simulate the STARTTLS upgrade.
        s.reset_after_starttls();
        let after = s.on_line("EHLO client.example");
        let wire = reply(&after).to_string();
        assert!(wire.contains("AUTH PLAIN LOGIN"));
        assert!(
            !wire.contains("STARTTLS"),
            "STARTTLS not offered once active"
        );
    }

    #[test]
    fn starttls_lifecycle() {
        let mut s = Session::new(params(Role::Mx));
        s.on_line("EHLO client.example");
        assert_eq!(s.on_line("STARTTLS"), Directive::StartTls);
        s.reset_after_starttls();
        // After reset, a greeting is required again (§4.2.2).
        assert_eq!(code(&s.on_line("MAIL FROM:<a@b.example>")), 503);
        // And STARTTLS is no longer available.
        assert_eq!(code(&s.on_line("STARTTLS")), 503);
    }

    #[test]
    fn starttls_unavailable_when_no_cert() {
        let mut p = params(Role::Mx);
        p.tls_available = false;
        let mut s = Session::new(p);
        s.on_line("EHLO client.example");
        assert_eq!(code(&s.on_line("STARTTLS")), 454);
    }

    #[test]
    fn auth_requires_submission_tls_and_no_txn() {
        // On MX: AUTH is refused outright.
        let mut mx = Session::new(params(Role::Mx));
        mx.on_line("EHLO c.example");
        assert_eq!(code(&mx.on_line("AUTH PLAIN abc")), 503);

        // On submission before TLS: 538 encryption required.
        let mut sub = Session::new(params(Role::Submission));
        sub.on_line("EHLO c.example");
        assert_eq!(code(&sub.on_line("AUTH PLAIN abc")), 538);

        // After TLS: a valid mechanism yields an Authenticate directive.
        sub.reset_after_starttls();
        sub.on_line("EHLO c.example");
        assert!(matches!(
            sub.on_line("AUTH PLAIN dGVzdA=="),
            Directive::Authenticate {
                mechanism: Mechanism::Plain,
                initial: Some(_)
            }
        ));
        // An unknown mechanism → 504.
        assert_eq!(code(&sub.on_line("AUTH CRAM-MD5")), 504);
    }

    #[test]
    fn mx_relay_guard_rejects_non_local_recipients() {
        // With a hosted-domain allowlist, the MX (unauthenticated) may
        // only accept recipients in a local domain — the anti-open-relay
        // guard (RFC 5321 §7.2). This is the security-critical property.
        let mut p = params(Role::Mx);
        p.local_domains = vec!["alo.test".to_owned()];
        let mut s = Session::new(p);
        s.on_line("EHLO client.example");
        assert_eq!(code(&s.on_line("MAIL FROM:<a@b.example>")), 250);
        // Local recipient: accepted.
        assert_eq!(code(&s.on_line("RCPT TO:<alice@alo.test>")), 250);
        // Case-insensitive domain match.
        assert_eq!(code(&s.on_line("RCPT TO:<bob@ALO.TEST>")), 250);
        // Non-local recipient: relaying denied (550).
        assert_eq!(code(&s.on_line("RCPT TO:<victim@external.com>")), 550);
        // <postmaster> (no domain) is always local (§4.1.1.3).
        assert_eq!(code(&s.on_line("RCPT TO:<postmaster>")), 250);
    }

    #[test]
    fn mx_with_empty_allowlist_accepts_all_dev_default() {
        // Empty allowlist = development accept-all (config refuses to
        // pair this with outbound enabled).
        let mut s = Session::new(params(Role::Mx));
        s.on_line("EHLO client.example");
        s.on_line("MAIL FROM:<a@b.example>");
        assert_eq!(code(&s.on_line("RCPT TO:<anyone@external.com>")), 250);
    }

    #[test]
    fn submission_blocks_mail_until_tls_then_auth() {
        let mut s = Session::new(params(Role::Submission));
        s.on_line("EHLO c.example");
        // Before TLS: must STARTTLS first (530).
        assert_eq!(code(&s.on_line("MAIL FROM:<a@b.example>")), 530);
        s.reset_after_starttls();
        s.on_line("EHLO c.example");
        // After TLS but before AUTH: authentication required (530).
        assert_eq!(code(&s.on_line("MAIL FROM:<a@b.example>")), 530);
        s.set_authenticated(AuthIdentity {
            username: "alice@b.example".to_owned(),
        });
        // Once authenticated, MAIL is accepted. Sender authorization
        // (binding MAIL FROM to the identity) is deferred to M9 with
        // the real send-as permission model — see the design note — so
        // M3 does not restrict the envelope sender here.
        assert_eq!(code(&s.on_line("MAIL FROM:<alice@b.example>")), 250);
    }

    #[test]
    fn mx_accepts_mail_without_auth() {
        // Inbound relay must not require auth.
        let mut s = Session::new(params(Role::Mx));
        s.on_line("EHLO c.example");
        assert_eq!(code(&s.on_line("MAIL FROM:<a@b.example>")), 250);
        assert_eq!(code(&s.on_line("RCPT TO:<c@alo.test>")), 250);
    }

    #[test]
    fn mx_allowlist_refuses_foreign_recipients() {
        // Anti-open-relay (S1): with a hosted-domains allowlist, MX
        // accepts local recipients and postmaster, refuses others.
        let mut p = params(Role::Mx);
        p.local_domains = vec!["alo.test".to_owned()];
        let mut s = Session::new(p);
        s.on_line("EHLO c.example");
        s.on_line("MAIL FROM:<a@b.example>");
        assert_eq!(code(&s.on_line("RCPT TO:<bob@alo.test>")), 250);
        assert_eq!(code(&s.on_line("RCPT TO:<BOB@alo.Test>")), 250);
        assert_eq!(code(&s.on_line("RCPT TO:<postmaster>")), 250);
        assert_eq!(code(&s.on_line("RCPT TO:<eve@elsewhere.example>")), 550);
    }

    #[test]
    fn empty_allowlist_accepts_any_recipient() {
        // Development default: no allowlist configured → accept all.
        let mut s = Session::new(params(Role::Mx));
        s.on_line("EHLO c.example");
        s.on_line("MAIL FROM:<a@b.example>");
        assert_eq!(code(&s.on_line("RCPT TO:<eve@elsewhere.example>")), 250);
    }

    #[test]
    fn size_and_body_params_accepted_oversize_rejected() {
        let mut s = Session::new(params(Role::Mx));
        s.on_line("EHLO c.example");
        assert_eq!(
            code(&s.on_line("MAIL FROM:<a@b.example> SIZE=1000 BODY=8BITMIME")),
            250
        );
        s.on_line("RSET");
        // Oversize declaration is rejected early (552).
        assert_eq!(
            code(&s.on_line("MAIL FROM:<a@b.example> SIZE=999999999999")),
            552
        );
        s.on_line("RSET");
        // An unadvertised parameter is still 555.
        assert_eq!(code(&s.on_line("MAIL FROM:<a@b.example> SMTPUTF8")), 555);
    }

    #[test]
    fn protocol_name_reflects_tls() {
        let mut s = Session::new(params(Role::Mx));
        s.on_line("EHLO c.example");
        assert_eq!(s.protocol_name(), "ESMTP");
        s.reset_after_starttls();
        s.on_line("EHLO c.example");
        assert_eq!(s.protocol_name(), "ESMTPS");
    }
}
