//! POP3 (RFC 1939) over implicit TLS (995), inbox-only. `USER`/`PASS`
//! authenticate through the same `alo-identity` credential authority as
//! IMAP; `UIDL` reuses the inbox's stable per-mailbox UIDs so a client's
//! "leave on server" bookkeeping matches IMAP.

use std::collections::HashSet;
use std::sync::Arc;

use alo_identity::Identity;
use alo_store::{AccountStore, MailboxId, MessageId, Store};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::config::Config;
use crate::stream::ImapStream;

/// Accept loop for the POP3 implicit-TLS listener.
pub async fn accept(
    listener: TcpListener,
    cfg: Arc<Config>,
    store: Arc<Store>,
    identity: Identity,
    acceptor: Option<TlsAcceptor>,
) {
    let Some(acceptor) = acceptor else {
        tracing::warn!("POP3 listener has no TLS acceptor; refusing to serve cleartext");
        return;
    };
    loop {
        let (tcp, peer) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "POP3 accept failed");
                continue;
            }
        };
        let cfg = cfg.clone();
        let store = store.clone();
        let identity = identity.clone();
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            match acceptor.accept(tcp).await {
                Ok(tls) => {
                    let session =
                        Pop3Session::new(ImapStream::Tls(Box::new(tls)), cfg, store, identity);
                    if let Err(e) = session.run().await {
                        tracing::debug!(%peer, error = %e, "POP3 session ended");
                    }
                }
                Err(e) => tracing::debug!(%peer, error = %e, "POP3 TLS handshake failed"),
            }
        });
    }
}

/// One inbox message in the POP3 session snapshot.
struct Pop3Msg {
    uid: i64,
    message: MessageId,
    size: i64,
}

/// A POP3 session over one connection.
pub struct Pop3Session {
    reader: BufReader<ImapStream>,
    cfg: Arc<Config>,
    store: Arc<Store>,
    identity: Identity,
    acc: Option<AccountStore>,
    inbox: Option<MailboxId>,
    pending_user: Option<String>,
    messages: Vec<Pop3Msg>,
    deleted: HashSet<usize>,
    auth_failures: u32,
}

impl Pop3Session {
    /// Builds a session over an already-TLS stream.
    pub fn new(
        stream: ImapStream,
        cfg: Arc<Config>,
        store: Arc<Store>,
        identity: Identity,
    ) -> Self {
        Self {
            reader: BufReader::new(stream),
            cfg,
            store,
            identity,
            acc: None,
            inbox: None,
            pending_user: None,
            messages: Vec::new(),
            deleted: HashSet::new(),
            auth_failures: 0,
        }
    }

    /// Runs the session until QUIT, timeout, or a fatal error.
    pub async fn run(mut self) -> std::io::Result<()> {
        self.send("+OK alo POP3 ready\r\n").await?;
        loop {
            let line = match tokio::time::timeout(self.cfg.idle_timeout, self.read_line()).await {
                Ok(r) => r?,
                Err(_) => {
                    let _ = self.send("-ERR autologout\r\n").await;
                    return Ok(());
                }
            };
            let Some(line) = line else {
                return Ok(());
            };
            let mut parts = line.trim_end().splitn(2, ' ');
            let cmd = parts.next().unwrap_or("").to_ascii_uppercase();
            let arg = parts.next().unwrap_or("").trim().to_owned();
            match cmd.as_str() {
                "QUIT" => {
                    self.commit_deletions().await;
                    self.send("+OK alo POP3 signing off\r\n").await?;
                    return Ok(());
                }
                "USER" => {
                    self.pending_user = Some(arg);
                    self.send("+OK\r\n").await?;
                }
                "PASS" => self.cmd_pass(&arg).await?,
                "CAPA" => self.cmd_capa().await?,
                "STAT" => self.cmd_stat().await?,
                "LIST" => self.cmd_list(&arg).await?,
                "UIDL" => self.cmd_uidl(&arg).await?,
                "RETR" => self.cmd_retr(&arg).await?,
                "TOP" => self.cmd_top(&arg).await?,
                "DELE" => self.cmd_dele(&arg).await?,
                "RSET" => {
                    self.deleted.clear();
                    self.send("+OK\r\n").await?;
                }
                "NOOP" => self.send("+OK\r\n").await?,
                _ => self.send("-ERR unknown command\r\n").await?,
            }
        }
    }

    async fn cmd_pass(&mut self, pass: &str) -> std::io::Result<()> {
        let Some(user) = self.pending_user.take() else {
            return self.send("-ERR USER first\r\n").await;
        };
        // Legacy-protocol auth: primary or app password; a 2FA account's
        // primary is refused (fail closed), per-username backoff (see
        // docs/design/identity.md).
        match self.identity.authenticate_legacy(&user, pass).await {
            Ok(Some(principal)) => {
                let acc = self.store.for_account(principal.tenant, principal.user);
                let inbox = match acc.inbox().await {
                    Ok(i) => i,
                    Err(_) => return self.send("-ERR server error\r\n").await,
                };
                match acc.imap_search_rows(&inbox).await {
                    Ok(rows) => {
                        self.messages = rows
                            .into_iter()
                            .map(|r| Pop3Msg {
                                uid: r.uid,
                                message: r.message,
                                size: r.size,
                            })
                            .collect();
                    }
                    Err(_) => return self.send("-ERR server error\r\n").await,
                }
                self.inbox = Some(inbox);
                self.acc = Some(acc);
                self.send("+OK mailbox ready\r\n").await
            }
            Ok(None) => {
                self.auth_failures += 1;
                self.send("-ERR invalid credentials\r\n").await?;
                if self.auth_failures >= self.cfg.max_auth_failures {
                    // Drop the connection so one socket is not an unbounded
                    // password-guessing oracle (mirrors the IMAP cap).
                    return Err(std::io::Error::other("auth failure cap"));
                }
                Ok(())
            }
            Err(_) => self.send("-ERR temporary failure\r\n").await,
        }
    }

    async fn cmd_capa(&mut self) -> std::io::Result<()> {
        self.send("+OK Capability list follows\r\nUSER\r\nUIDL\r\nTOP\r\n.\r\n")
            .await
    }

    async fn cmd_stat(&mut self) -> std::io::Result<()> {
        if self.acc.is_none() {
            return self.send("-ERR not authenticated\r\n").await;
        }
        let (count, size): (usize, i64) = self
            .messages
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.deleted.contains(i))
            .fold((0, 0), |(c, s), (_, m)| (c + 1, s + m.size));
        self.send(&format!("+OK {count} {size}\r\n")).await
    }

    async fn cmd_list(&mut self, arg: &str) -> std::io::Result<()> {
        if self.acc.is_none() {
            return self.send("-ERR not authenticated\r\n").await;
        }
        if arg.is_empty() {
            let mut out = String::from("+OK scan listing follows\r\n");
            for (i, m) in self.messages.iter().enumerate() {
                if !self.deleted.contains(&i) {
                    out.push_str(&format!("{} {}\r\n", i + 1, m.size));
                }
            }
            out.push_str(".\r\n");
            self.send(&out).await
        } else {
            match self.index(arg) {
                Some(i) => {
                    self.send(&format!("+OK {} {}\r\n", i + 1, self.messages[i].size))
                        .await
                }
                None => self.send("-ERR no such message\r\n").await,
            }
        }
    }

    async fn cmd_uidl(&mut self, arg: &str) -> std::io::Result<()> {
        if self.acc.is_none() {
            return self.send("-ERR not authenticated\r\n").await;
        }
        if arg.is_empty() {
            let mut out = String::from("+OK unique-id listing follows\r\n");
            for (i, m) in self.messages.iter().enumerate() {
                if !self.deleted.contains(&i) {
                    out.push_str(&format!("{} {}\r\n", i + 1, m.uid));
                }
            }
            out.push_str(".\r\n");
            self.send(&out).await
        } else {
            match self.index(arg) {
                Some(i) => {
                    self.send(&format!("+OK {} {}\r\n", i + 1, self.messages[i].uid))
                        .await
                }
                None => self.send("-ERR no such message\r\n").await,
            }
        }
    }

    async fn cmd_retr(&mut self, arg: &str) -> std::io::Result<()> {
        let Some(i) = self.index(arg) else {
            return self.send("-ERR no such message\r\n").await;
        };
        let Some(acc) = self.acc.clone() else {
            return self.send("-ERR not authenticated\r\n").await;
        };
        let msg = self.messages[i].message.clone();
        match acc.message_bytes(&msg).await {
            Ok(bytes) => {
                self.send(&format!("+OK {} octets\r\n", bytes.len()))
                    .await?;
                self.send_multiline(&bytes).await
            }
            Err(_) => self.send("-ERR message unavailable\r\n").await,
        }
    }

    async fn cmd_top(&mut self, arg: &str) -> std::io::Result<()> {
        let mut it = arg.split_whitespace();
        let num = it.next().unwrap_or("");
        let lines: usize = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let Some(i) = self.index(num) else {
            return self.send("-ERR no such message\r\n").await;
        };
        let Some(acc) = self.acc.clone() else {
            return self.send("-ERR not authenticated\r\n").await;
        };
        let msg = self.messages[i].message.clone();
        match acc.message_bytes(&msg).await {
            Ok(bytes) => {
                let (header, body, _) = crate::fetch::split_header_body(&bytes);
                let mut out = header.to_vec();
                out.extend_from_slice(b"\r\n\r\n");
                for line in body.split_inclusive(|&b| b == b'\n').take(lines) {
                    out.extend_from_slice(line);
                }
                self.send("+OK top follows\r\n").await?;
                self.send_multiline(&out).await
            }
            Err(_) => self.send("-ERR message unavailable\r\n").await,
        }
    }

    async fn cmd_dele(&mut self, arg: &str) -> std::io::Result<()> {
        match self.index(arg) {
            Some(i) => {
                self.deleted.insert(i);
                self.send("+OK marked for deletion\r\n").await
            }
            None => self.send("-ERR no such message\r\n").await,
        }
    }

    async fn commit_deletions(&mut self) {
        let (Some(acc), Some(inbox)) = (self.acc.clone(), self.inbox.clone()) else {
            return;
        };
        for i in &self.deleted {
            if let Some(m) = self.messages.get(*i) {
                let _ = acc.imap_expunge(&inbox, &m.message).await;
            }
        }
    }

    /// Resolves a 1-based message number argument to a live index, honoring
    /// deletions.
    fn index(&self, arg: &str) -> Option<usize> {
        let n: usize = arg.trim().parse().ok()?;
        if n == 0 || n > self.messages.len() {
            return None;
        }
        let i = n - 1;
        if self.deleted.contains(&i) {
            return None;
        }
        Some(i)
    }

    // ---- transport ----------------------------------------------------

    async fn send(&mut self, s: &str) -> std::io::Result<()> {
        self.reader.get_mut().write_all(s.as_bytes()).await?;
        self.reader.get_mut().flush().await
    }

    /// Sends a multiline response body: dot-stuffed, terminated by `.CRLF`.
    async fn send_multiline(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let mut out = Vec::with_capacity(bytes.len() + 8);
        for line in bytes.split_inclusive(|&b| b == b'\n') {
            if line.first() == Some(&b'.') {
                out.push(b'.'); // byte-stuff
            }
            out.extend_from_slice(line);
        }
        if !out.ends_with(b"\n") {
            out.extend_from_slice(b"\r\n");
        }
        out.extend_from_slice(b".\r\n");
        self.reader.get_mut().write_all(&out).await?;
        self.reader.get_mut().flush().await
    }

    async fn read_line(&mut self) -> std::io::Result<Option<String>> {
        let mut buf = Vec::new();
        let mut limited = (&mut self.reader).take(self.cfg.max_line as u64 + 2);
        let n = limited.read_until(b'\n', &mut buf).await?;
        if n == 0 {
            return Ok(None);
        }
        Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
    }
}
