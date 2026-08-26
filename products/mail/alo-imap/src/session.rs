//! The IMAP session: the NotAuthenticated → Authenticated → Selected state
//! machine (RFC 9051 §3) and command dispatch. Every data command reaches
//! the store only through an [`AccountStore`], so isolation is inherited,
//! not re-checked. Responses are byte buffers (FETCH body sections are
//! literals), written straight to the connection.

use std::sync::Arc;

use alo_identity::Identity;
use alo_store::{AccountStore, MailboxId, MessageId, Store};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio_rustls::TlsAcceptor;

use crate::config::Config;
use crate::parser::{Parser, ReadOutcome, Seg, SequenceSet, read_command};
use crate::stream::ImapStream;

/// Session state (RFC 9051 §3).
#[derive(PartialEq, Eq, Clone, Copy)]
enum State {
    NotAuth,
    Auth,
    Selected,
}

/// One message in the selected-mailbox view snapshot.
#[derive(Clone)]
struct ViewEntry {
    uid: i64,
    message: MessageId,
    flags: Vec<String>,
}

/// The selected mailbox and its session view.
struct Selected {
    id: MailboxId,
    read_only: bool,
    view: Vec<ViewEntry>,
    /// Account state token at last view sync (per-account modseq).
    synced_state: String,
}

/// A running IMAP session over one connection.
pub struct Session {
    reader: BufReader<ImapStream>,
    cfg: Arc<Config>,
    store: Arc<Store>,
    identity: Identity,
    acceptor: Option<TlsAcceptor>,
    state: State,
    acc: Option<AccountStore>,
    selected: Option<Selected>,
    auth_failures: u32,
    tls_active: bool,
}

/// What the run loop should do after a command.
enum Flow {
    Continue,
    Logout,
    StartTls,
}

impl Session {
    /// Builds a session over `stream`. `tls_active` is true for implicit-TLS
    /// ports; `acceptor` enables STARTTLS on cleartext ports.
    pub fn new(
        stream: ImapStream,
        cfg: Arc<Config>,
        store: Arc<Store>,
        identity: Identity,
        acceptor: Option<TlsAcceptor>,
    ) -> Self {
        let tls_active = stream.is_tls();
        Self {
            reader: BufReader::new(stream),
            cfg,
            store,
            identity,
            acceptor,
            state: State::NotAuth,
            acc: None,
            selected: None,
            auth_failures: 0,
            tls_active,
        }
    }

    /// Runs the session until logout, timeout, or a fatal error.
    pub async fn run(mut self) -> std::io::Result<()> {
        let caps = self.capabilities();
        self.send_line(&format!("* OK [CAPABILITY {caps}] alo IMAP ready"))
            .await?;
        loop {
            let outcome = match tokio::time::timeout(
                self.cfg.idle_timeout,
                read_command(&mut self.reader, self.cfg.max_line, self.cfg.max_literal),
            )
            .await
            {
                Ok(r) => r?,
                Err(_) => {
                    let _ = self.send_line("* BYE Autologout; idle for too long").await;
                    return Ok(());
                }
            };
            let segs = match outcome {
                ReadOutcome::Line(segs) => segs,
                ReadOutcome::Eof => return Ok(()),
                ReadOutcome::TooLong => {
                    self.send_line("* BAD Command line too long").await?;
                    continue;
                }
                ReadOutcome::LiteralTooLarge => {
                    self.send_line("* BYE Literal too large").await?;
                    return Ok(());
                }
                ReadOutcome::BareNewline => {
                    self.send_line("* BAD CRLF required").await?;
                    continue;
                }
                ReadOutcome::BadLiteral => {
                    self.send_line("* BAD Invalid literal").await?;
                    continue;
                }
            };
            match self.dispatch(segs).await? {
                Flow::Continue => {}
                Flow::Logout => return Ok(()),
                Flow::StartTls => {
                    if self.upgrade_tls().await.is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }

    // ---- transport helpers --------------------------------------------

    async fn send(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.reader.get_mut().write_all(bytes).await?;
        self.reader.get_mut().flush().await
    }

    async fn send_line(&mut self, line: &str) -> std::io::Result<()> {
        let mut b = line.as_bytes().to_vec();
        b.extend_from_slice(b"\r\n");
        self.send(&b).await
    }

    /// Performs the STARTTLS handshake, discarding any buffered plaintext
    /// (buffered bytes after our `OK` are a command-injection attempt).
    async fn upgrade_tls(&mut self) -> std::io::Result<()> {
        let Some(acceptor) = self.acceptor.clone() else {
            return Err(std::io::Error::other("no TLS acceptor"));
        };
        if !self.reader.buffer().is_empty() {
            // CVE-2011-0411 class: plaintext queued before the handshake.
            let _ = self.send_line("* BYE Buffered data after STARTTLS").await;
            return Err(std::io::Error::other("starttls injection"));
        }
        let placeholder = BufReader::new(ImapStream::Closed);
        let old = std::mem::replace(&mut self.reader, placeholder);
        let stream = old.into_inner();
        let ImapStream::Plain(tcp) = stream else {
            return Err(std::io::Error::other("starttls on non-plain stream"));
        };
        let tls = acceptor.accept(tcp).await?;
        self.reader = BufReader::new(ImapStream::Tls(Box::new(tls)));
        self.tls_active = true;
        Ok(())
    }

    fn capabilities(&self) -> String {
        let mut caps = vec!["IMAP4rev2".to_owned(), "IMAP4rev1".to_owned()];
        if !self.tls_active && self.acceptor.is_some() {
            caps.push("STARTTLS".to_owned());
            caps.push("LOGINDISABLED".to_owned());
        }
        if self.tls_active {
            caps.push("AUTH=PLAIN".to_owned());
            caps.push("AUTH=LOGIN".to_owned());
            caps.push("AUTH=XOAUTH2".to_owned());
            // RFC 4959: the AUTHENTICATE initial response every mechanism
            // above accepts (XOAUTH2 clients in particular always send one).
            caps.push("SASL-IR".to_owned());
        }
        for c in [
            "IDLE",
            "MOVE",
            "UIDPLUS",
            "LITERAL+",
            "SPECIAL-USE",
            "ENABLE",
            "NAMESPACE",
        ] {
            caps.push(c.to_owned());
        }
        caps.join(" ")
    }

    // ---- dispatch -----------------------------------------------------

    async fn dispatch(&mut self, segs: Vec<Seg>) -> std::io::Result<Flow> {
        let mut p = Parser::new(segs);
        let tag = p.read_atom();
        if tag.is_empty() {
            self.send_line("* BAD Missing tag").await?;
            return Ok(Flow::Continue);
        }
        p.skip_sp();
        let cmd = p.read_atom().to_ascii_uppercase();
        if cmd.is_empty() {
            self.tagged(&tag, "BAD", "Missing command").await?;
            return Ok(Flow::Continue);
        }
        p.skip_sp();
        match cmd.as_str() {
            "CAPABILITY" => {
                let caps = self.capabilities();
                self.send_line(&format!("* CAPABILITY {caps}")).await?;
                self.tagged(&tag, "OK", "CAPABILITY completed").await?;
            }
            "NOOP" => {
                self.resync_if_selected().await?;
                self.tagged(&tag, "OK", "NOOP completed").await?;
            }
            "LOGOUT" => {
                self.send_line("* BYE alo IMAP logging out").await?;
                self.tagged(&tag, "OK", "LOGOUT completed").await?;
                return Ok(Flow::Logout);
            }
            "ENABLE" => {
                // We enable nothing extra; accept and report none enabled.
                self.send_line("* ENABLED").await?;
                self.tagged(&tag, "OK", "ENABLE completed").await?;
            }
            "STARTTLS" => {
                if self.tls_active {
                    self.tagged(&tag, "BAD", "TLS already active").await?;
                } else if self.acceptor.is_none() {
                    self.tagged(&tag, "NO", "STARTTLS not available").await?;
                } else {
                    self.tagged(&tag, "OK", "Begin TLS negotiation now").await?;
                    return Ok(Flow::StartTls);
                }
            }
            "LOGIN" => self.cmd_login(&tag, &mut p).await?,
            "AUTHENTICATE" => self.cmd_authenticate(&tag, &mut p).await?,
            "NAMESPACE" => {
                self.send_line("* NAMESPACE ((\"\" \"/\")) NIL NIL").await?;
                self.tagged(&tag, "OK", "NAMESPACE completed").await?;
            }
            _ => return self.dispatch_authed(&tag, &cmd, &mut p).await,
        }
        Ok(Flow::Continue)
    }

    /// Commands that require authentication (and, for some, a selection).
    async fn dispatch_authed(
        &mut self,
        tag: &str,
        cmd: &str,
        p: &mut Parser,
    ) -> std::io::Result<Flow> {
        let acc = match self.acc.clone() {
            Some(a) => a,
            None => {
                self.tagged(tag, "NO", "Not authenticated").await?;
                return Ok(Flow::Continue);
            }
        };
        match cmd {
            "SELECT" => self.cmd_select(tag, p, false, &acc).await?,
            "EXAMINE" => self.cmd_select(tag, p, true, &acc).await?,
            "CREATE" => self.cmd_create(tag, p, &acc).await?,
            "DELETE" => self.cmd_delete(tag, p, &acc).await?,
            "RENAME" => self.cmd_rename(tag, p, &acc).await?,
            "LIST" => self.cmd_list(tag, p, false, &acc).await?,
            "LSUB" => self.cmd_list(tag, p, true, &acc).await?,
            "SUBSCRIBE" | "UNSUBSCRIBE" => {
                // Subscription state is not modelled; accept as a no-op so
                // clients that manage it do not error.
                let _ = p.read_astring();
                self.tagged(tag, "OK", &format!("{cmd} completed")).await?;
            }
            "STATUS" => self.cmd_status(tag, p, &acc).await?,
            "APPEND" => self.cmd_append(tag, p, &acc).await?,
            "IDLE" => return self.cmd_idle(tag, &acc).await.map(|()| Flow::Continue),
            "CHECK" => {
                if self.selected.is_some() {
                    self.resync(&acc).await?;
                    self.tagged(tag, "OK", "CHECK completed").await?;
                } else {
                    self.tagged(tag, "BAD", "No mailbox selected").await?;
                }
            }
            "CLOSE" => self.cmd_close(tag, true, &acc).await?,
            "UNSELECT" => self.cmd_close(tag, false, &acc).await?,
            "EXPUNGE" => self.cmd_expunge(tag, None, &acc).await?,
            "SEARCH" => self.cmd_search(tag, p, false, &acc).await?,
            "FETCH" => self.cmd_fetch(tag, p, false, &acc).await?,
            "STORE" => self.cmd_store(tag, p, false, &acc).await?,
            "COPY" => self.cmd_copy(tag, p, false, &acc).await?,
            "MOVE" => self.cmd_move(tag, p, false, &acc).await?,
            "UID" => self.cmd_uid(tag, p, &acc).await?,
            _ => {
                self.tagged(tag, "BAD", "Unknown command").await?;
            }
        }
        Ok(Flow::Continue)
    }

    async fn cmd_uid(
        &mut self,
        tag: &str,
        p: &mut Parser,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        p.skip_sp();
        let sub = p.read_atom().to_ascii_uppercase();
        p.skip_sp();
        match sub.as_str() {
            "FETCH" => self.cmd_fetch(tag, p, true, acc).await,
            "STORE" => self.cmd_store(tag, p, true, acc).await,
            "SEARCH" => self.cmd_search(tag, p, true, acc).await,
            "COPY" => self.cmd_copy(tag, p, true, acc).await,
            "MOVE" => self.cmd_move(tag, p, true, acc).await,
            "EXPUNGE" => {
                let set = p.read_atom();
                self.cmd_expunge(tag, SequenceSet::parse(&set), acc).await
            }
            _ => self.tagged(tag, "BAD", "Unknown UID command").await,
        }
    }

    // ---- authentication ------------------------------------------------

    async fn cmd_login(&mut self, tag: &str, p: &mut Parser) -> std::io::Result<()> {
        if !self.tls_active {
            self.tagged(tag, "NO", "[PRIVACYREQUIRED] STARTTLS first")
                .await?;
            return Ok(());
        }
        let user = p.read_astring().map(|a| a.as_string());
        p.skip_sp();
        let pass = p.read_astring().map(|a| a.as_string());
        let (Some(user), Some(pass)) = (user, pass) else {
            self.tagged(tag, "BAD", "LOGIN requires a username and password")
                .await?;
            return Ok(());
        };
        self.try_login(tag, &user, &pass).await
    }

    async fn cmd_authenticate(&mut self, tag: &str, p: &mut Parser) -> std::io::Result<()> {
        if !self.tls_active {
            self.tagged(tag, "NO", "[PRIVACYREQUIRED] STARTTLS first")
                .await?;
            return Ok(());
        }
        let mechanism = p.read_atom().to_ascii_uppercase();
        p.skip_sp();
        // An initial response may be present (SASL-IR); else prompt.
        let initial = p.read_atom();
        match mechanism.as_str() {
            "PLAIN" => {
                let b64 = if initial.is_empty() {
                    self.send("+ \r\n".as_bytes()).await?;
                    self.read_continuation().await?
                } else {
                    initial
                };
                match decode_sasl_plain(&b64) {
                    Some((user, pass)) => self.try_login(tag, &user, &pass).await,
                    None => self.tagged(tag, "BAD", "Invalid SASL response").await,
                }
            }
            "LOGIN" => {
                // AUTH=LOGIN: two base64 prompts (username, then password).
                self.send("+ VXNlcm5hbWU6\r\n".as_bytes()).await?; // "Username:"
                let user = decode_b64(&self.read_continuation().await?);
                self.send("+ UGFzc3dvcmQ6\r\n".as_bytes()).await?; // "Password:"
                let pass = decode_b64(&self.read_continuation().await?);
                match (user, pass) {
                    (Some(u), Some(pw)) => self.try_login(tag, &u, &pw).await,
                    _ => self.tagged(tag, "BAD", "Invalid SASL response").await,
                }
            }
            "XOAUTH2" => {
                let b64 = if initial.is_empty() {
                    self.send("+ \r\n".as_bytes()).await?;
                    self.read_continuation().await?
                } else {
                    initial
                };
                let parsed = B64
                    .decode(b64.trim())
                    .ok()
                    .and_then(|raw| alo_identity::xoauth2::parse_client_response(&raw));
                match parsed {
                    Some(resp) => self.try_xoauth2(tag, &resp.username, &resp.token).await,
                    None => self.tagged(tag, "BAD", "Invalid SASL response").await,
                }
            }
            _ => {
                self.tagged(tag, "NO", "Unsupported authentication mechanism")
                    .await
            }
        }
    }

    async fn read_continuation(&mut self) -> std::io::Result<String> {
        let outcome =
            read_command(&mut self.reader, self.cfg.max_line, self.cfg.max_literal).await?;
        match outcome {
            ReadOutcome::Line(segs) => {
                let mut p = Parser::new(segs);
                Ok(p.rest())
            }
            _ => Ok(String::new()),
        }
    }

    async fn try_login(&mut self, tag: &str, user: &str, pass: &str) -> std::io::Result<()> {
        // Legacy-protocol auth: accepts the primary password or an app
        // password; a 2FA account's primary is refused (fail closed — app
        // password or OIDC instead), with per-username backoff. See
        // docs/design/identity.md.
        match self.identity.authenticate_legacy(user, pass).await {
            Ok(Some(principal)) => self.auth_success(tag, principal).await,
            Ok(None) => {
                self.tagged(tag, "NO", "[AUTHENTICATIONFAILED] invalid credentials")
                    .await?;
                self.auth_failure().await
            }
            Err(_) => {
                self.tagged(tag, "NO", "Temporary authentication failure")
                    .await
            }
        }
    }

    /// Verifies an XOAUTH2 bearer login. On a credential failure the
    /// mechanism's own error dialog runs first: a continuation carrying a
    /// base64 error status, which the client acknowledges with an empty
    /// line before the tagged `NO` (the de-facto XOAUTH2 contract real
    /// clients implement — see `docs/interop.md`).
    async fn try_xoauth2(&mut self, tag: &str, user: &str, token: &str) -> std::io::Result<()> {
        // Bearer verification through the introspection seam (ADR 0025):
        // revoked and expired tokens fail on the next connection, and the
        // token must belong to exactly the asserted user.
        match self.identity.authenticate_xoauth2(user, token).await {
            Ok(Some(principal)) => self.auth_success(tag, principal).await,
            Ok(None) => {
                let status = alo_identity::xoauth2::error_status_b64();
                self.send(format!("+ {status}\r\n").as_bytes()).await?;
                // The client's (empty) acknowledgement line; its content is
                // irrelevant — the exchange has already failed.
                let _ = self.read_continuation().await?;
                self.tagged(tag, "NO", "[AUTHENTICATIONFAILED] invalid credentials")
                    .await?;
                self.auth_failure().await
            }
            Err(_) => {
                self.tagged(tag, "NO", "Temporary authentication failure")
                    .await
            }
        }
    }

    /// Enters the Authenticated state for a verified principal and sends
    /// the tagged OK (with the post-auth capability list, RFC 9051 §6.2).
    async fn auth_success(
        &mut self,
        tag: &str,
        principal: alo_identity::Principal,
    ) -> std::io::Result<()> {
        let acc = self.store.for_account(principal.tenant, principal.user);
        // Guarantee INBOX exists (RFC 9051 §5.1 — INBOX is always
        // present); the store provisions it lazily otherwise.
        let _ = acc.inbox().await;
        self.acc = Some(acc);
        self.state = State::Auth;
        let caps = self.capabilities();
        self.tagged(tag, "OK", &format!("[CAPABILITY {caps}] LOGIN completed"))
            .await
    }

    /// Counts one failed authentication toward the per-connection cap,
    /// closing the connection when it is reached.
    async fn auth_failure(&mut self) -> std::io::Result<()> {
        self.auth_failures += 1;
        if self.auth_failures >= self.cfg.max_auth_failures {
            let _ = self
                .send_line("* BYE Too many authentication failures")
                .await;
            return Err(std::io::Error::other("auth failure cap"));
        }
        Ok(())
    }

    // ---- tagged completion --------------------------------------------

    async fn tagged(&mut self, tag: &str, status: &str, text: &str) -> std::io::Result<()> {
        self.send_line(&format!("{tag} {status} {text}")).await
    }

    /// Resyncs the selected view if one is selected and authenticated,
    /// emitting any pending untagged EXPUNGE/EXISTS/FETCH (used by NOOP).
    async fn resync_if_selected(&mut self) -> std::io::Result<()> {
        if self.selected.is_some()
            && let Some(acc) = self.acc.clone()
        {
            self.resync(&acc).await?;
        }
        Ok(())
    }
}

/// Decodes a base64 SASL PLAIN response into `(authcid, password)`,
/// ignoring the authzid (RFC 4616).
fn decode_sasl_plain(b64: &str) -> Option<(String, String)> {
    let raw = B64.decode(b64.trim()).ok()?;
    let mut parts = raw.split(|&b| b == 0);
    let _authzid = parts.next()?;
    let authcid = String::from_utf8(parts.next()?.to_vec()).ok()?;
    let password = String::from_utf8(parts.next()?.to_vec()).ok()?;
    Some((authcid, password))
}

fn decode_b64(s: &str) -> Option<String> {
    String::from_utf8(B64.decode(s.trim()).ok()?).ok()
}

mod commands;
mod idle;
mod render;
