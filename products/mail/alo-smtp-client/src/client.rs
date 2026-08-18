//! The outbound SMTP client: one connection to a destination host,
//! delivering one or more messages over it (connection reuse within a
//! delivery pass), with the client-side timeouts of RFC 5321
//! §4.5.3.2 and outbound dot-stuffing (§4.5.2).

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use rustls::pki_types::ServerName;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::{TcpSocket, TcpStream};

use crate::client_reply::{ReplyError, ServerReply, read_reply};
use crate::tls::{MaybeTls, connector};

/// Client-side timeouts, RFC 5321 §4.5.3.2 (the sender values).
const GREETING_TIMEOUT: Duration = Duration::from_secs(300); // §4.5.3.2.1
const MAIL_TIMEOUT: Duration = Duration::from_secs(300); // §4.5.3.2.2
const RCPT_TIMEOUT: Duration = Duration::from_secs(300); // §4.5.3.2.3
const DATA_INIT_TIMEOUT: Duration = Duration::from_secs(120); // §4.5.3.2.4
const DATA_TERM_TIMEOUT: Duration = Duration::from_secs(600); // §4.5.3.2.6
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const WRITE_TIMEOUT: Duration = Duration::from_secs(60);

/// SMTP relay port (RFC 5321 §2.1).
pub const SMTP_PORT: u16 = 25;

/// Outcome for one recipient of one delivery attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RcptOutcome {
    /// Accepted by the remote server.
    Delivered,
    /// 4xx — keep the recipient pending and retry later.
    Transient(ServerReply),
    /// 5xx — never retry; goes into the DSN.
    Permanent(ServerReply),
}

/// A delivery attempt that failed before any per-recipient verdict
/// (connect, greeting, EHLO, MAIL). Always transient from the queue's
/// perspective unless the reply class says permanent.
#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    /// TCP connect failed or timed out for every address.
    #[error("could not connect to {host}: {reason}")]
    Connect {
        /// Destination host.
        host: String,
        /// Cause.
        reason: String,
    },
    /// The server replied with a failure before RCPT (greeting/EHLO/
    /// MAIL). Carries the reply so 5xx can be treated as permanent.
    #[error("server rejected the transaction at {stage}: {reply_code}")]
    Rejected {
        /// Which step failed (for diagnostics).
        stage: &'static str,
        /// Reply code.
        reply_code: u16,
        /// Full reply.
        reply: ServerReply,
    },
    /// Protocol/transport failure mid-session.
    #[error("session failure: {0}")]
    Session(#[from] ReplyError),
    /// Write-side transport failure.
    #[error("I/O writing to server: {0}")]
    Io(#[from] std::io::Error),
    /// The destination's TLS policy (DANE, RFC 7672) could not be
    /// satisfied: STARTTLS was required but not offered, or the
    /// certificate matched no TLSA record. Always transient — once TLS
    /// is required, delivery never falls back to cleartext.
    #[error("TLS policy failure for {host}: {reason}")]
    TlsPolicy {
        /// Destination host.
        host: String,
        /// What was violated.
        reason: String,
    },
}

/// How strongly TLS is required for one outbound connection
/// (RFC 7672 §2.2). The queue derives this per MX host from its
/// DNSSEC-validated TLSA lookup.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TlsRequirement {
    /// Encrypt when offered, deliver in cleartext otherwise — the
    /// default for hosts without (secure) TLSA records.
    #[default]
    Opportunistic,
    /// TLS is mandatory but unauthenticated: a secure TLSA set exists
    /// yet none of its records are usable by this client (§2.2 —
    /// "unusable" still forbids cleartext).
    Required,
    /// TLS is mandatory and the end-entity certificate must match one
    /// of these (DNSSEC-secured) TLSA records (DANE-EE, §3.1.1).
    DaneEe(Vec<crate::dane::TlsaRecord>),
}

/// One live outbound connection, greeted and EHLO'd (and STARTTLS-upgraded
/// when the peer offers it).
pub struct OutboundSession {
    stream: BufReader<MaybeTls>,
    peer: SocketAddr,
    /// Whether the channel was upgraded to TLS (RFC 3207).
    tls: bool,
}

impl OutboundSession {
    /// Connects to the first responsive address and completes the
    /// greeting + EHLO exchange (falling back to HELO on a 5xx to
    /// EHLO, per §2.2.1 for ancient servers).
    ///
    /// `source` pins the local address the connection leaves by (ADR 0044 §1 —
    /// a sending identity with its own IP); `None` lets the kernel choose, which
    /// is what every single-address deployment does.
    ///
    /// # Errors
    /// [`DeliveryError`] — connect failures and pre-MAIL rejections.
    pub async fn connect(
        host: &str,
        ips: &[IpAddr],
        port: u16,
        our_hostname: &str,
        tls: TlsRequirement,
        source: Option<IpAddr>,
    ) -> Result<Self, DeliveryError> {
        let mut last_reason = "no addresses to try".to_owned();
        for ip in ips {
            let addr = SocketAddr::new(*ip, port);
            match connect_from(addr, source).await {
                Ok(stream) => {
                    return Self::handshake(stream, addr, host, our_hostname, tls).await;
                }
                Err(reason) => last_reason = format!("{addr}: {reason}"),
            }
        }
        Err(DeliveryError::Connect {
            host: host.to_owned(),
            reason: last_reason,
        })
    }

    /// Connects to an explicit socket address (smarthost path), optionally from
    /// a pinned local address — see [`Self::connect`].
    ///
    /// # Errors
    /// [`DeliveryError`] — connect failures and pre-MAIL rejections.
    pub async fn connect_addr(
        addr: SocketAddr,
        our_hostname: &str,
        source: Option<IpAddr>,
    ) -> Result<Self, DeliveryError> {
        match connect_from(addr, source).await {
            Ok(stream) => {
                Self::handshake(
                    stream,
                    addr,
                    &addr.to_string(),
                    our_hostname,
                    TlsRequirement::Opportunistic,
                )
                .await
            }
            Err(reason) => Err(DeliveryError::Connect {
                host: addr.to_string(),
                reason,
            }),
        }
    }

    async fn handshake(
        stream: TcpStream,
        peer: SocketAddr,
        host: &str,
        our_hostname: &str,
        tls: TlsRequirement,
    ) -> Result<Self, DeliveryError> {
        let mut session = Self {
            stream: BufReader::new(MaybeTls::Plain(stream)),
            peer,
            tls: false,
        };
        let greeting = session.read_timed(GREETING_TIMEOUT, "greeting").await?;
        if !greeting.is_success() {
            return Err(reject("greeting", greeting));
        }
        let ehlo = session.ehlo(our_hostname).await?;
        // STARTTLS (RFC 3207): if the peer offers it, upgrade before
        // sending anything. A server that offers TLS but fails the
        // handshake is a transient failure — we never silently fall back
        // to cleartext once TLS was on the table (downgrade protection).
        // Under a DANE policy (RFC 7672 §2.2) the offer itself is
        // mandatory: a peer not advertising STARTTLS is a policy
        // violation, not a cleartext fallback.
        if ehlo.advertises("STARTTLS") {
            session.starttls(host, our_hostname, &tls).await?;
        } else if tls != TlsRequirement::Opportunistic {
            return Err(DeliveryError::TlsPolicy {
                host: host.to_owned(),
                reason: "TLSA records require TLS, but the server does not offer STARTTLS"
                    .to_owned(),
            });
        }
        tracing::debug!(peer = %peer, %host, tls = session.tls, "outbound session established");
        Ok(session)
    }

    /// EHLO, falling back to HELO on a 5xx (pre-ESMTP servers, §2.2.1). Returns
    /// the successful reply — its capability lines drive STARTTLS negotiation.
    async fn ehlo(&mut self, our_hostname: &str) -> Result<ServerReply, DeliveryError> {
        let ehlo = self
            .command(&format!("EHLO {our_hostname}"), MAIL_TIMEOUT, "EHLO")
            .await?;
        if ehlo.is_success() {
            return Ok(ehlo);
        }
        if ehlo.is_transient() {
            return Err(reject("EHLO", ehlo));
        }
        let helo = self
            .command(&format!("HELO {our_hostname}"), MAIL_TIMEOUT, "HELO")
            .await?;
        if !helo.is_success() {
            return Err(reject("HELO", helo));
        }
        Ok(helo)
    }

    /// Upgrades the channel with STARTTLS (RFC 3207) and re-issues EHLO over the
    /// encrypted link. Any failure is returned (transient to the queue).
    /// Under [`TlsRequirement::DaneEe`] the handshake authenticates the
    /// peer against the TLSA records instead of accepting any cert.
    async fn starttls(
        &mut self,
        host: &str,
        our_hostname: &str,
        requirement: &TlsRequirement,
    ) -> Result<(), DeliveryError> {
        let reply = self.command("STARTTLS", MAIL_TIMEOUT, "STARTTLS").await?;
        if reply.code != 220 {
            // A refused STARTTLS under a DANE policy is a policy
            // violation (no cleartext fallback), not a mere rejection.
            if *requirement != TlsRequirement::Opportunistic {
                return Err(DeliveryError::TlsPolicy {
                    host: host.to_owned(),
                    reason: format!("STARTTLS refused with {}", reply.code),
                });
            }
            return Err(reject("STARTTLS", reply));
        }
        // STARTTLS-injection guard (CVE-2011-0411 class): a server must send
        // nothing after the 220 before the TLS handshake. Buffered plaintext
        // here is a smuggled command — abort rather than trust it.
        if !self.stream.buffer().is_empty() {
            return Err(DeliveryError::Session(ReplyError::Malformed {
                reason: "server pipelined data before the TLS handshake".to_owned(),
            }));
        }
        // The connector embodies the policy: DANE-EE installs the TLSA
        // verifier; Opportunistic/Required (encrypt-only) accept any cert.
        let tls = match requirement {
            TlsRequirement::DaneEe(records) => crate::dane::dane_connector(records.clone()),
            TlsRequirement::Opportunistic | TlsRequirement::Required => connector(),
        }
        .ok_or_else(|| DeliveryError::Io(std::io::Error::other("TLS provider unavailable")))?;
        // Swap the plaintext socket out (buffer verified empty above) and wrap
        // it — the buffered reader is discarded only after that check.
        let plain = std::mem::replace(&mut self.stream, BufReader::new(MaybeTls::Taken));
        let MaybeTls::Plain(tcp) = plain.into_inner() else {
            return Err(DeliveryError::Io(std::io::Error::other(
                "STARTTLS attempted on a non-plaintext stream",
            )));
        };
        let sni = server_name(host, self.peer.ip());
        let upgraded = tokio::time::timeout(CONNECT_TIMEOUT, tls.connect(sni, tcp))
            .await
            .map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "TLS handshake timed out")
            })?
            .map_err(|error| {
                // Under DANE, a failed handshake is a policy violation
                // (typically the TLSA mismatch raised by our verifier).
                if matches!(requirement, TlsRequirement::DaneEe(_)) {
                    DeliveryError::TlsPolicy {
                        host: host.to_owned(),
                        reason: format!("TLS handshake failed under DANE: {error}"),
                    }
                } else {
                    DeliveryError::Io(error)
                }
            })?;
        self.stream = BufReader::new(MaybeTls::Tls(Box::new(upgraded)));
        self.tls = true;
        // §4.2: discard the prior EHLO state and re-issue it over TLS.
        self.ehlo(our_hostname).await?;
        Ok(())
    }

    /// Delivers one message on this session. Returns the outcome per
    /// recipient, in the order given. The session remains usable for
    /// another message afterwards (connection reuse) unless an error
    /// was returned.
    ///
    /// # Errors
    /// [`DeliveryError`] when the transaction failed before any
    /// recipient verdict (MAIL rejected, transport lost).
    pub async fn deliver(
        &mut self,
        mail_from: Option<&str>,
        rcpts: &[String],
        message: &[u8],
    ) -> Result<Vec<RcptOutcome>, DeliveryError> {
        let from = mail_from.unwrap_or("");
        let mail = self
            .command(&format!("MAIL FROM:<{from}>"), MAIL_TIMEOUT, "MAIL")
            .await?;
        if !mail.is_success() {
            return Err(reject("MAIL", mail));
        }

        let mut outcomes = Vec::with_capacity(rcpts.len());
        let mut any_accepted = false;
        for rcpt in rcpts {
            let reply = self
                .command(&format!("RCPT TO:<{rcpt}>"), RCPT_TIMEOUT, "RCPT")
                .await?;
            let outcome = if reply.is_success() {
                any_accepted = true;
                RcptOutcome::Delivered // provisional until DATA accepted
            } else if reply.is_transient() {
                RcptOutcome::Transient(reply)
            } else {
                RcptOutcome::Permanent(reply)
            };
            outcomes.push(outcome);
        }

        if !any_accepted {
            // Nothing to send; reset so the session stays reusable.
            let _reset = self.command("RSET", MAIL_TIMEOUT, "RSET").await?;
            return Ok(outcomes);
        }

        let data = self.command("DATA", DATA_INIT_TIMEOUT, "DATA").await?;
        if data.code != 354 {
            let class_outcome = if data.is_transient() {
                RcptOutcome::Transient(data)
            } else {
                RcptOutcome::Permanent(data)
            };
            // DATA refusal applies to every provisionally-accepted rcpt.
            for outcome in &mut outcomes {
                if *outcome == RcptOutcome::Delivered {
                    *outcome = class_outcome.clone();
                }
            }
            return Ok(outcomes);
        }

        self.write_body(message).await?;
        let verdict = self.read_timed(DATA_TERM_TIMEOUT, "end-of-data").await?;
        if !verdict.is_success() {
            let class_outcome = if verdict.is_transient() {
                RcptOutcome::Transient(verdict)
            } else {
                RcptOutcome::Permanent(verdict)
            };
            for outcome in &mut outcomes {
                if *outcome == RcptOutcome::Delivered {
                    *outcome = class_outcome.clone();
                }
            }
        }
        Ok(outcomes)
    }

    /// Whether this session's channel was upgraded to TLS (RFC 3207) — for
    /// delivery logging and the `Received:`/reporting story.
    pub fn is_tls(&self) -> bool {
        self.tls
    }

    /// Politely ends the session.
    pub async fn quit(mut self) {
        // Best effort — the messages are already accepted or not.
        let _quit = self.command("QUIT", MAIL_TIMEOUT, "QUIT").await;
    }

    /// Writes the message with outbound dot-stuffing (§4.5.2) and the
    /// CRLF.CRLF terminator. Content is CRLF-normalized by the spool.
    async fn write_body(&mut self, message: &[u8]) -> Result<(), DeliveryError> {
        let mut wire = Vec::with_capacity(message.len() + 128);
        for line in message.split_inclusive(|&b| b == b'\n') {
            if line.first() == Some(&b'.') {
                wire.push(b'.');
            }
            wire.extend_from_slice(line);
        }
        // Ensure the terminator sits on its own line even if the
        // content lacked a trailing CRLF.
        if !wire.ends_with(b"\r\n") {
            wire.extend_from_slice(b"\r\n");
        }
        wire.extend_from_slice(b".\r\n");
        self.write_timed(&wire).await?;
        Ok(())
    }

    async fn command(
        &mut self,
        line: &str,
        timeout: Duration,
        stage: &'static str,
    ) -> Result<ServerReply, DeliveryError> {
        self.write_timed(format!("{line}\r\n").as_bytes()).await?;
        self.read_timed(timeout, stage).await
    }

    async fn read_timed(
        &mut self,
        timeout: Duration,
        stage: &'static str,
    ) -> Result<ServerReply, DeliveryError> {
        match tokio::time::timeout(timeout, read_reply(&mut self.stream)).await {
            Ok(result) => Ok(result?),
            Err(_elapsed) => Err(DeliveryError::Session(ReplyError::Malformed {
                reason: format!("timeout awaiting {stage} reply from {}", self.peer),
            })),
        }
    }

    async fn write_timed(&mut self, bytes: &[u8]) -> Result<(), DeliveryError> {
        tokio::time::timeout(WRITE_TIMEOUT, async {
            self.stream.write_all(bytes).await?;
            self.stream.flush().await
        })
        .await
        .map_err(|_elapsed| {
            DeliveryError::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "write to server timed out",
            ))
        })??;
        Ok(())
    }
}

/// Opens one TCP connection to `addr`, from `source` when a local address is
/// pinned. Returns a non-sensitive reason on failure.
///
/// **A pinned source of the wrong address family fails the attempt** rather than
/// falling back to the kernel's choice. A campaign identity's SPF record
/// authorises one address and ends in `-all`; leaving by any other one is a
/// message that arrives failing SPF, and mail deferred with a logged reason is
/// recoverable where mail delivered under a failing identity is not.
async fn connect_from(addr: SocketAddr, source: Option<IpAddr>) -> Result<TcpStream, String> {
    let attempt = async {
        match source {
            None => TcpStream::connect(addr).await,
            Some(source) => {
                let socket = match (addr, source) {
                    (SocketAddr::V4(_), IpAddr::V4(_)) => TcpSocket::new_v4()?,
                    (SocketAddr::V6(_), IpAddr::V6(_)) => TcpSocket::new_v6()?,
                    _ => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AddrNotAvailable,
                            format!(
                                "egress address {source} cannot reach this destination \
                                 (different IP family)"
                            ),
                        ));
                    }
                };
                // Port 0: the kernel picks the source port, we pick only the
                // address.
                socket.bind(SocketAddr::new(source, 0))?;
                socket.connect(addr).await
            }
        }
    };
    match tokio::time::timeout(CONNECT_TIMEOUT, attempt).await {
        Ok(Ok(stream)) => Ok(stream),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_elapsed) => Err("connect timed out".to_owned()),
    }
}

fn reject(stage: &'static str, reply: ServerReply) -> DeliveryError {
    DeliveryError::Rejected {
        stage,
        reply_code: reply.code,
        reply,
    }
}

/// The SNI name for the TLS handshake: the peer hostname (minus any `:port`)
/// when it parses as a DNS name, else the peer IP. Advisory only — the
/// certificate is not verified for opportunistic delivery TLS.
fn server_name(host: &str, ip: IpAddr) -> ServerName<'static> {
    let name = host.split(':').next().unwrap_or(host);
    ServerName::try_from(name.to_owned()).unwrap_or_else(|_| ServerName::IpAddress(ip.into()))
}
