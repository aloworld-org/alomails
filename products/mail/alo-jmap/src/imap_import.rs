//! Outbound IMAP client for the import wizard: pull a user's existing
//! mail from Gmail/Outlook/any IMAP host into their alo mailboxes,
//! preserving folder structure and read/flagged/answered/draft state.
//!
//! Two layers, split so the fiddly protocol is testable without TLS or a
//! network:
//! - [`fetch_folders`] speaks IMAP over any async stream (LOGIN → LIST →
//!   for each selectable folder SELECT + FETCH the most-recent messages as
//!   `(FLAGS BODY.PEEK[])` → LOGOUT), handling literals by exact byte
//!   count. A plaintext mock exercises it.
//! - [`import`] resolves the host, refuses any non-public address
//!   (SSRF — the user names the host), pins the verified IP, opens
//!   **verified** implicit TLS (real Mozilla roots — the user's
//!   password is on this wire), runs `fetch_folders`, maps each remote
//!   folder to the matching alo mailbox (special-use → role, others
//!   created by name), and ingests each message with its flags, skipping
//!   any whose `Message-ID` is already present (idempotent re-import).
//!
//! Scope (recorded in `docs/interop.md`): the most-recent [`MAX_MESSAGES`]
//! across all selectable folders (Gmail's virtual `\All`/`\Flagged` are
//! skipped so mail is not imported twice), done synchronously. Unbounded
//! full-mailbox migration and background/resume remain follow-ups. The
//! password is never logged.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use alo_ai::egress::is_blocked_ip;
use alo_store::{AccountStore, MailboxId, Page};
use mail_parser::MessageParser;
use rustls::pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

/// The most-recent messages pulled in one import, summed across folders (a
/// bounded, synchronous operation; unbounded migration is a follow-up).
pub const MAX_MESSAGES: u32 = 500;
/// Largest single message accepted from the remote server.
const MAX_MESSAGE_BYTES: usize = 40 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const IO_TIMEOUT: Duration = Duration::from_secs(120);

/// What an import attempt did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct ImportOutcome {
    /// Newly-stored messages.
    pub imported: u32,
    /// Messages already present (by `Message-ID`) and skipped.
    pub skipped: u32,
    /// Messages that failed to store (logged, not fatal to the batch).
    pub failed: u32,
}

/// Why an import could not run.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The host did not resolve, or resolved only to blocked (private/
    /// loopback/link-local) addresses.
    #[error("the mail server address is invalid or not reachable")]
    Host,
    /// TCP/TLS transport failure.
    #[error("could not connect securely to the mail server")]
    Connect,
    /// The IMAP server rejected the credentials.
    #[error("the username or password was not accepted")]
    Auth,
    /// A protocol or I/O error mid-session.
    #[error("the mail server did not respond as expected")]
    Protocol,
}

/// Connection details the user supplies.
pub struct ImapConfig<'a> {
    pub host: &'a str,
    pub port: u16,
    pub username: &'a str,
    pub password: &'a str,
}

/// Resolves + SSRF-guards `config.host`, connects over verified TLS,
/// fetches recent mail from every selectable folder, and ingests it into
/// the matching alo mailboxes (with flags) under `Message-ID` dedup.
pub async fn import(
    acc: &AccountStore,
    config: &ImapConfig<'_>,
) -> Result<ImportOutcome, ImportError> {
    let addr = resolve_public(config.host, config.port).await?;
    let server_name =
        ServerName::try_from(config.host.to_owned()).map_err(|_| ImportError::Host)?;

    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(addr))
        .await
        .map_err(|_| ImportError::Connect)?
        .map_err(|_| ImportError::Connect)?;
    let connector = TlsConnector::from(Arc::new(tls_config()));
    let tls = tokio::time::timeout(CONNECT_TIMEOUT, connector.connect(server_name, tcp))
        .await
        .map_err(|_| ImportError::Connect)?
        .map_err(|_| ImportError::Connect)?;

    let folders = fetch_folders(tls, config.username, config.password, MAX_MESSAGES).await?;
    import_folders(acc, folders).await
}

/// Resolves `host:port` to a single **public** socket address, refusing
/// the host if it does not resolve or any resolved address is blocked
/// (loopback/private/link-local/…). Pinning the checked IP for the
/// caller's connect closes the DNS-rebind gap.
async fn resolve_public(host: &str, port: u16) -> Result<std::net::SocketAddr, ImportError> {
    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| ImportError::Host)?
        .collect();
    if addrs.is_empty() {
        return Err(ImportError::Host);
    }
    // Refuse if ANY resolved address is blocked — a host that resolves to
    // both a public and an internal address is not trustworthy.
    if addrs.iter().any(|a| is_blocked_ip(a.ip())) {
        return Err(ImportError::Host);
    }
    Ok(addrs[0])
}

/// A rustls client config that verifies the server certificate against
/// the bundled Mozilla roots (the user's password is on this wire — an
/// accept-any verifier would invite a MITM).
fn tls_config() -> rustls::ClientConfig {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

/// One message fetched from the remote server: its raw bytes plus the
/// IMAP flags that decide which alo keywords it keeps.
#[derive(Debug, Clone)]
pub struct FetchedMessage {
    pub raw: Vec<u8>,
    pub flags: FetchedFlags,
}

/// The subset of IMAP system flags we carry over as JMAP keywords.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FetchedFlags {
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub draft: bool,
}

impl FetchedFlags {
    /// The JMAP keywords (RFC 8621) these flags map to.
    fn keywords(self) -> Vec<&'static str> {
        let mut k = Vec::new();
        if self.seen {
            k.push("$seen");
        }
        if self.flagged {
            k.push("$flagged");
        }
        if self.answered {
            k.push("$answered");
        }
        if self.draft {
            k.push("$draft");
        }
        k
    }

    /// Union of two flag sets (a message's flags can appear both before and
    /// after the body literal in a FETCH response).
    fn or(self, o: FetchedFlags) -> FetchedFlags {
        FetchedFlags {
            seen: self.seen || o.seen,
            flagged: self.flagged || o.flagged,
            answered: self.answered || o.answered,
            draft: self.draft || o.draft,
        }
    }
}

/// Where a remote folder's messages should land in alo. Special-use
/// folders map to a role (get-or-create the canonical mailbox); anything
/// else is created as a top-level mailbox by its leaf name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderTarget {
    /// A canonical mailbox by JMAP role, with the display name to use when
    /// it must be created.
    Role {
        role: &'static str,
        name: &'static str,
    },
    /// A user folder created (or matched) by name.
    Named(String),
}

/// A remote folder's fetched messages, tagged with where they belong.
#[derive(Debug, Clone)]
pub struct RawFolder {
    pub target: FolderTarget,
    pub messages: Vec<FetchedMessage>,
}

/// Runs the IMAP session over `stream`: LOGIN, LIST all folders, and for
/// each selectable folder (in a sensible priority order, virtual folders
/// skipped) SELECT + FETCH the most-recent messages with their flags,
/// stopping once `budget` messages have been collected in total. Generic
/// over the stream so the protocol is unit-tested against a plaintext mock.
pub async fn fetch_folders<S>(
    stream: S,
    username: &str,
    password: &str,
    budget: u32,
) -> Result<Vec<RawFolder>, ImportError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut conn = ImapConn::new(stream);
    conn.read_greeting().await?;
    conn.login(username, password).await?;

    // Classify every LIST entry, drop the skips, and order so the primary
    // mail (INBOX, then Sent/Drafts/…) gets first claim on the budget.
    let mut classified: Vec<(u8, String, FolderTarget)> = conn
        .list_folders()
        .await?
        .into_iter()
        .filter_map(|(attrs, delim, name)| {
            classify(&attrs, &name, delim).map(|(prio, target)| (prio, name, target))
        })
        .collect();
    classified.sort_by(|a, b| a.0.cmp(&b.0));

    let mut folders = Vec::new();
    let mut remaining = budget;
    for (_prio, name, target) in classified {
        if remaining == 0 {
            break;
        }
        let Some(exists) = conn.select(&name).await? else {
            continue; // \Noselect raced in, or SELECT refused — skip, don't fail
        };
        if exists == 0 {
            continue;
        }
        let take = exists.min(remaining);
        let lo = exists.saturating_sub(take).saturating_add(1).max(1);
        let messages = conn.fetch_messages(lo, exists).await?;
        remaining = remaining.saturating_sub(messages.len() as u32);
        folders.push(RawFolder { target, messages });
    }
    conn.logout().await;
    Ok(folders)
}

/// Classifies a LIST entry into a skip (`None`) or a `(priority, target)`.
/// Priority orders the budget: INBOX first, then the standard folders, then
/// user folders. `\Noselect`/`\NonExistent` and the virtual `\All`/
/// `\Flagged`/`\Important` folders (which overlap real ones and would
/// double-import) are skipped.
fn classify(attrs: &[String], name: &str, delim: char) -> Option<(u8, FolderTarget)> {
    if has_attr(attrs, "Noselect") || has_attr(attrs, "NonExistent") {
        return None;
    }
    if has_attr(attrs, "All") || has_attr(attrs, "Flagged") || has_attr(attrs, "Important") {
        return None;
    }
    if name.eq_ignore_ascii_case("INBOX") {
        return Some((
            0,
            FolderTarget::Role {
                role: "inbox",
                name: "Inbox",
            },
        ));
    }
    // The leaf name after the hierarchy delimiter (e.g. "[Gmail]/Sent" → "Sent").
    let leaf = if delim != '\0' {
        name.rsplit(delim).next().unwrap_or(name)
    } else {
        name
    };
    let l = leaf.to_ascii_lowercase();
    let role = if has_attr(attrs, "Sent") || l == "sent" || l == "sent mail" || l == "sent items" {
        Some((1u8, "sent", "Sent"))
    } else if has_attr(attrs, "Drafts") || l == "drafts" {
        Some((2, "drafts", "Drafts"))
    } else if has_attr(attrs, "Junk") || l == "junk" || l == "spam" {
        Some((3, "junk", "Junk"))
    } else if has_attr(attrs, "Trash") || l == "trash" || l == "deleted" || l == "deleted items" {
        Some((4, "trash", "Trash"))
    } else if has_attr(attrs, "Archive") || l == "archive" {
        Some((5, "archive", "Archive"))
    } else {
        None
    };
    match role {
        Some((prio, role, name)) => Some((prio, FolderTarget::Role { role, name })),
        None => Some((6, FolderTarget::Named(leaf.to_owned()))),
    }
}

/// Case-insensitive test for an IMAP mailbox attribute, ignoring the
/// leading backslash (`\Sent` matches `"Sent"`).
fn has_attr(attrs: &[String], name: &str) -> bool {
    attrs
        .iter()
        .any(|a| a.trim_start_matches('\\').eq_ignore_ascii_case(name))
}

/// Resolves [`FolderTarget`]s to mailbox ids, get-or-creating as needed and
/// caching so a folder is created at most once per import.
struct Targets<'a> {
    acc: &'a AccountStore,
    by_role: HashMap<String, MailboxId>,
    by_name: HashMap<String, MailboxId>,
}

impl<'a> Targets<'a> {
    async fn load(acc: &'a AccountStore) -> Result<Targets<'a>, ImportError> {
        let boxes = acc
            .mailboxes(Page::new(alo_store::MAX_PAGE, 0))
            .await
            .map_err(|_| ImportError::Protocol)?;
        let mut by_role = HashMap::new();
        let mut by_name = HashMap::new();
        for m in boxes {
            if let Some(role) = m.role.clone() {
                by_role.insert(role, m.id.clone());
            }
            by_name.insert(m.name.to_ascii_lowercase(), m.id);
        }
        Ok(Targets {
            acc,
            by_role,
            by_name,
        })
    }

    async fn resolve(&mut self, target: &FolderTarget) -> Result<MailboxId, ImportError> {
        match target {
            FolderTarget::Role { role, name } => {
                if let Some(id) = self.by_role.get(*role) {
                    return Ok(id.clone());
                }
                let id = self
                    .acc
                    .create_mailbox(None, name, Some(role))
                    .await
                    .map_err(|_| ImportError::Protocol)?;
                self.by_role.insert((*role).to_owned(), id.clone());
                self.by_name.insert(name.to_ascii_lowercase(), id.clone());
                Ok(id)
            }
            FolderTarget::Named(name) => {
                let key = name.to_ascii_lowercase();
                if let Some(id) = self.by_name.get(&key) {
                    return Ok(id.clone());
                }
                let id = self
                    .acc
                    .create_mailbox(None, name, None)
                    .await
                    .map_err(|_| ImportError::Protocol)?;
                self.by_name.insert(key, id.clone());
                Ok(id)
            }
        }
    }
}

/// Ingests fetched folders into the matching alo mailboxes, applying each
/// message's flags and skipping any whose `Message-ID` is already stored —
/// or was imported earlier in this same run (a message that lives in two
/// remote folders is stored once). Public so the dedup/ingest half can be
/// tested without a live IMAP server.
pub async fn import_folders(
    acc: &AccountStore,
    folders: Vec<RawFolder>,
) -> Result<ImportOutcome, ImportError> {
    let mut targets = Targets::load(acc).await?;

    // One dedup query for every id across all folders.
    let all_ids: Vec<String> = folders
        .iter()
        .flat_map(|f| f.messages.iter())
        .filter_map(|m| message_id(&m.raw))
        .collect();
    let mut present: HashSet<String> = acc
        .existing_message_ids(&all_ids)
        .await
        .map_err(|_| ImportError::Protocol)?;

    let mut out = ImportOutcome::default();
    for folder in &folders {
        let mailbox = targets.resolve(&folder.target).await?;
        for msg in &folder.messages {
            let id = message_id(&msg.raw);
            if let Some(id) = &id
                && present.contains(id)
            {
                out.skipped += 1;
                continue;
            }
            match acc.ingest(&mailbox, &msg.raw).await {
                Ok(mid) => {
                    out.imported += 1;
                    if let Some(id) = id {
                        present.insert(id); // don't re-store it from another folder
                    }
                    for kw in msg.flags.keywords() {
                        if let Err(error) = acc.set_keyword(&mid, kw, true).await {
                            tracing::warn!(%error, keyword = kw, "imap import: could not set flag");
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "imap import: one message failed to store");
                    out.failed += 1;
                }
            }
        }
    }
    Ok(out)
}

/// Ingests a flat batch into the account's Inbox (no per-message flags) —
/// a thin wrapper over [`import_folders`] kept for callers/tests that only
/// need the single-folder path.
pub async fn import_messages(
    acc: &AccountStore,
    messages: Vec<Vec<u8>>,
) -> Result<ImportOutcome, ImportError> {
    let folder = RawFolder {
        target: FolderTarget::Role {
            role: "inbox",
            name: "Inbox",
        },
        messages: messages
            .into_iter()
            .map(|raw| FetchedMessage {
                raw,
                flags: FetchedFlags::default(),
            })
            .collect(),
    };
    import_folders(acc, vec![folder]).await
}

/// Extracts the `Message-ID` in the **bracketed** form the store keeps
/// (`<id@host>`), so the dedup query matches `messages.message_id_hdr`.
/// mail-parser returns the id without brackets, so we re-add them.
fn message_id(raw: &[u8]) -> Option<String> {
    let parsed = MessageParser::default().parse(raw)?;
    let bare = parsed
        .message_id()?
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_owned();
    (!bare.is_empty()).then(|| format!("<{bare}>"))
}

/// A buffered IMAP connection with a monotonically increasing command tag.
struct ImapConn<S> {
    stream: BufReader<S>,
    tag: u32,
}

impl<S> ImapConn<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn new(stream: S) -> Self {
        Self {
            stream: BufReader::new(stream),
            tag: 0,
        }
    }

    async fn read_greeting(&mut self) -> Result<(), ImportError> {
        let line = self.read_line().await?;
        // `* OK ...` — anything else (e.g. `* BYE`) is a refusal.
        if line.starts_with("* OK") {
            Ok(())
        } else {
            Err(ImportError::Protocol)
        }
    }

    async fn login(&mut self, user: &str, pass: &str) -> Result<(), ImportError> {
        let tag = self.next_tag();
        let cmd = format!("{tag} LOGIN {} {}\r\n", quote(user), quote(pass));
        self.write(cmd.as_bytes()).await?;
        // Consume untagged lines until our tagged completion.
        match self.read_completion(&tag).await? {
            Completion::Ok => Ok(()),
            Completion::No => Err(ImportError::Auth),
            Completion::Bad => Err(ImportError::Protocol),
        }
    }

    /// Issues `LIST "" "*"` and returns each entry's `(attributes,
    /// hierarchy delimiter, mailbox name)`. A `NIL` delimiter is reported as
    /// `'\0'`.
    async fn list_folders(&mut self) -> Result<Vec<(Vec<String>, char, String)>, ImportError> {
        let tag = self.next_tag();
        self.write(format!("{tag} LIST \"\" \"*\"\r\n").as_bytes())
            .await?;
        let mut out = Vec::new();
        loop {
            let line = self.read_line().await?;
            if let Some(rest) = tagged(&line, &tag) {
                return if rest.starts_with("OK") {
                    Ok(out)
                } else {
                    Err(ImportError::Protocol)
                };
            }
            if let Some(entry) = parse_list_line(&line) {
                out.push(entry);
            }
        }
    }

    /// SELECTs `name` and returns its message count (`* n EXISTS`), or
    /// `None` if the server refused the SELECT (e.g. a `\Noselect` folder),
    /// which the caller skips rather than treating as fatal.
    async fn select(&mut self, name: &str) -> Result<Option<u32>, ImportError> {
        let tag = self.next_tag();
        self.write(format!("{tag} SELECT {}\r\n", quote(name)).as_bytes())
            .await?;
        let mut exists = 0u32;
        loop {
            let line = self.read_line().await?;
            if let Some(rest) = tagged(&line, &tag) {
                return if rest.starts_with("OK") {
                    Ok(Some(exists))
                } else {
                    Ok(None)
                };
            }
            // `* <n> EXISTS`
            if let Some(n) = line
                .strip_prefix("* ")
                .and_then(|r| r.strip_suffix(" EXISTS"))
            {
                exists = n.trim().parse().unwrap_or(exists);
            }
        }
    }

    /// FETCHes `lo:hi (FLAGS BODY.PEEK[])`, returning each message's raw
    /// bytes and parsed flags. Flags may appear on the line that opens the
    /// body literal or on its trailer; both are considered.
    async fn fetch_messages(
        &mut self,
        lo: u32,
        hi: u32,
    ) -> Result<Vec<FetchedMessage>, ImportError> {
        let tag = self.next_tag();
        self.write(format!("{tag} FETCH {lo}:{hi} (FLAGS BODY.PEEK[])\r\n").as_bytes())
            .await?;
        let mut messages = Vec::new();
        loop {
            let line = self.read_line().await?;
            if let Some(rest) = tagged(&line, &tag) {
                return if rest.starts_with("OK") {
                    Ok(messages)
                } else {
                    Err(ImportError::Protocol)
                };
            }
            // A FETCH response line ends with a literal `{size}` for BODY[].
            if let Some(size) = literal_size(&line) {
                if size > MAX_MESSAGE_BYTES {
                    return Err(ImportError::Protocol);
                }
                let flags = parse_flags(&line);
                let mut body = vec![0u8; size];
                tokio::time::timeout(IO_TIMEOUT, self.stream.read_exact(&mut body))
                    .await
                    .map_err(|_| ImportError::Protocol)?
                    .map_err(|_| ImportError::Protocol)?;
                // The literal is followed by the rest of the response line
                // (`)` and possibly trailing FLAGS), then CRLF — consume it.
                let trailer = self.read_line().await?;
                messages.push(FetchedMessage {
                    raw: body,
                    flags: flags.or(parse_flags(&trailer)),
                });
            }
        }
    }

    async fn logout(&mut self) {
        let tag = self.next_tag();
        // Best-effort; the session is done regardless.
        let _ = self.write(format!("{tag} LOGOUT\r\n").as_bytes()).await;
    }

    /// Reads untagged lines until the tagged completion, returning its
    /// status. Used where untagged content is irrelevant (LOGIN).
    async fn read_completion(&mut self, tag: &str) -> Result<Completion, ImportError> {
        loop {
            let line = self.read_line().await?;
            if let Some(rest) = tagged(&line, tag) {
                return Ok(if rest.starts_with("OK") {
                    Completion::Ok
                } else if rest.starts_with("NO") {
                    Completion::No
                } else {
                    Completion::Bad
                });
            }
        }
    }

    fn next_tag(&mut self) -> String {
        self.tag += 1;
        format!("a{}", self.tag)
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<(), ImportError> {
        tokio::time::timeout(IO_TIMEOUT, self.stream.get_mut().write_all(bytes))
            .await
            .map_err(|_| ImportError::Protocol)?
            .map_err(|_| ImportError::Protocol)?;
        Ok(())
    }

    /// Reads one CRLF-terminated protocol line (without the CRLF).
    /// Bounded so a hostile server cannot stream an unbounded line.
    async fn read_line(&mut self) -> Result<String, ImportError> {
        let mut buf = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            let n = tokio::time::timeout(IO_TIMEOUT, self.stream.read_exact(&mut byte))
                .await
                .map_err(|_| ImportError::Protocol)?
                .map_err(|_| ImportError::Protocol)?;
            let _ = n;
            if byte[0] == b'\n' {
                if buf.last() == Some(&b'\r') {
                    buf.pop();
                }
                break;
            }
            buf.push(byte[0]);
            if buf.len() > 64 * 1024 {
                return Err(ImportError::Protocol);
            }
        }
        String::from_utf8(buf).map_err(|_| ImportError::Protocol)
    }
}

enum Completion {
    Ok,
    No,
    Bad,
}

/// If `line` is our tagged completion (`<tag> ...`), returns the rest.
fn tagged<'a>(line: &'a str, tag: &str) -> Option<&'a str> {
    line.strip_prefix(tag)
        .and_then(|r| r.strip_prefix(' '))
        .map(str::trim_start)
}

/// The trailing IMAP literal size `{n}` on a response line, if present.
fn literal_size(line: &str) -> Option<usize> {
    let inner = line.trim_end().strip_suffix('}')?;
    let brace = inner.rfind('{')?;
    inner[brace + 1..].parse().ok()
}

/// Parses the `\Seen \Flagged …` set from a `FLAGS (…)` occurrence on a
/// response line into the system flags we carry over. Absent → all false.
fn parse_flags(line: &str) -> FetchedFlags {
    let upper = line.to_ascii_uppercase();
    let Some(pos) = upper.find("FLAGS (") else {
        return FetchedFlags::default();
    };
    let after = &line[pos + "FLAGS (".len()..];
    let Some(end) = after.find(')') else {
        return FetchedFlags::default();
    };
    let inner = after[..end].to_ascii_lowercase();
    FetchedFlags {
        seen: inner.contains("\\seen"),
        flagged: inner.contains("\\flagged"),
        answered: inner.contains("\\answered"),
        draft: inner.contains("\\draft"),
    }
}

/// Parses a `* LIST (attrs) "delim" name` response into
/// `(attributes, delimiter, mailbox name)`. Returns `None` for any other
/// untagged line.
fn parse_list_line(line: &str) -> Option<(Vec<String>, char, String)> {
    let rest = strip_prefix_ci(line, "* LIST ")?;
    let (attrs, rest) = parse_paren_list(rest.trim_start())?;
    let (delim_tok, rest) = parse_astring(rest.trim_start())?;
    let (name, _rest) = parse_astring(rest.trim_start())?;
    let delim = if delim_tok.eq_ignore_ascii_case("NIL") {
        '\0'
    } else {
        delim_tok.chars().next().unwrap_or('\0')
    };
    Some((attrs, delim, name))
}

/// Case-insensitive prefix strip, returning the remainder.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// Parses a parenthesized, whitespace-separated flag list `(a b c)` at the
/// start of `s`, returning the tokens and the remainder after `)`. The
/// attribute list never contains quoted strings or nested parens.
fn parse_paren_list(s: &str) -> Option<(Vec<String>, &str)> {
    let s = s.strip_prefix('(')?;
    let end = s.find(')')?;
    let attrs = s[..end].split_whitespace().map(str::to_owned).collect();
    Some((attrs, &s[end + 1..]))
}

/// Parses one IMAP astring at the start of `s` — a quoted string (with
/// `\`-escapes) or an unquoted atom — returning it and the remainder.
fn parse_astring(s: &str) -> Option<(String, &str)> {
    if let Some(rest) = s.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '\\' => {
                    if let Some((_, n)) = chars.next() {
                        out.push(n);
                    }
                }
                '"' => return Some((out, &rest[i + 1..])),
                _ => out.push(c),
            }
        }
        None // unterminated quoted string
    } else {
        let end = s.find(char::is_whitespace).unwrap_or(s.len());
        (end > 0).then(|| (s[..end].to_owned(), &s[end..]))
    }
}

/// Quotes a string as an IMAP quoted-string (RFC 3501 §4.3), escaping
/// `\` and `"`. Refuses CR/LF (they would break the command line — such
/// a value simply cannot be a valid credential).
fn quote(s: &str) -> String {
    let escaped: String = s
        .chars()
        .filter(|c| *c != '\r' && *c != '\n')
        .flat_map(|c| match c {
            '\\' => vec!['\\', '\\'],
            '"' => vec!['\\', '"'],
            c => vec![c],
        })
        .collect();
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn literal_size_parsing() {
        assert_eq!(literal_size("* 1 FETCH (BODY[] {2748}"), Some(2748));
        assert_eq!(literal_size("* 1 FETCH (UID 5 BODY[] {12}\r\n"), Some(12));
        assert_eq!(literal_size("a3 OK done"), None);
        assert_eq!(literal_size("* 2 EXISTS"), None);
    }

    #[test]
    fn quoting_escapes_and_strips_crlf() {
        assert_eq!(quote("user"), "\"user\"");
        assert_eq!(quote("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(quote("x\r\nA LOGIN evil"), "\"xA LOGIN evil\"");
    }

    #[test]
    fn tagged_completion() {
        assert_eq!(tagged("a1 OK LOGIN done", "a1"), Some("OK LOGIN done"));
        assert_eq!(tagged("* 1 EXISTS", "a1"), None);
        assert_eq!(tagged("a10 OK", "a1"), None); // not a prefix-false-match
    }

    #[test]
    fn message_id_extraction() {
        // Bracketed form, matching what the store keeps in message_id_hdr.
        let raw = b"Subject: hi\r\nMessage-ID: <abc@x.eu>\r\n\r\nbody\r\n";
        assert_eq!(message_id(raw).as_deref(), Some("<abc@x.eu>"));
        assert_eq!(message_id(b"Subject: none\r\n\r\nx"), None);
    }

    #[test]
    fn flag_parsing_and_keywords() {
        let f = parse_flags("* 2 FETCH (FLAGS (\\Seen \\Flagged) BODY[] {5}");
        assert_eq!(
            f,
            FetchedFlags {
                seen: true,
                flagged: true,
                answered: false,
                draft: false
            }
        );
        assert_eq!(f.keywords(), vec!["$seen", "$flagged"]);
        // Case-insensitive, order-independent, and a bare set → nothing.
        assert!(parse_flags("(flags (\\answered \\draft))").answered);
        assert_eq!(
            parse_flags("* 1 FETCH (FLAGS () BODY[] {5}"),
            FetchedFlags::default()
        );
        assert_eq!(parse_flags("no flags here"), FetchedFlags::default());
    }

    #[test]
    fn list_line_parsing() {
        let (attrs, delim, name) =
            parse_list_line("* LIST (\\HasNoChildren \\Sent) \"/\" \"[Gmail]/Sent Mail\"").unwrap();
        assert_eq!(attrs, vec!["\\HasNoChildren", "\\Sent"]);
        assert_eq!(delim, '/');
        assert_eq!(name, "[Gmail]/Sent Mail");
        // Unquoted atom name and a NIL delimiter.
        let (_a, delim, name) = parse_list_line("* LIST () NIL INBOX").unwrap();
        assert_eq!(delim, '\0');
        assert_eq!(name, "INBOX");
        assert!(parse_list_line("* 3 EXISTS").is_none());
    }

    #[test]
    fn classify_maps_special_use_and_skips_virtual() {
        // INBOX first, special-use by attribute, user folder by leaf name.
        assert_eq!(
            classify(&["\\HasNoChildren".into()], "INBOX", '/'),
            Some((
                0,
                FolderTarget::Role {
                    role: "inbox",
                    name: "Inbox"
                }
            ))
        );
        assert_eq!(
            classify(&["\\Sent".into()], "[Gmail]/Sent Mail", '/'),
            Some((
                1,
                FolderTarget::Role {
                    role: "sent",
                    name: "Sent"
                }
            ))
        );
        // Name-based fallback when no special-use attribute is present.
        assert_eq!(
            classify(&[], "Spam", '/'),
            Some((
                3,
                FolderTarget::Role {
                    role: "junk",
                    name: "Junk"
                }
            ))
        );
        assert_eq!(
            classify(&[], "Projects/Client", '/'),
            Some((6, FolderTarget::Named("Client".into())))
        );
        // Virtual and non-selectable folders are skipped.
        assert_eq!(classify(&["\\All".into()], "[Gmail]/All Mail", '/'), None);
        assert_eq!(classify(&["\\Noselect".into()], "[Gmail]", '/'), None);
    }

    /// A scripted, tag-aware mock IMAP server: greeting → LOGIN → LIST (INBOX,
    /// Sent, a \Noselect parent) → SELECT+FETCH INBOX (2 msgs, one \Seen) →
    /// SELECT+FETCH Sent (1 msg, \Answered) → LOGOUT. Exercises multi-folder
    /// fetch, special-use classification, \Noselect skipping, and flags.
    #[tokio::test]
    async fn fetch_folders_protocol_over_a_mock() {
        let (client, mut server) = tokio::io::duplex(64 * 1024);
        let mock = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            async fn line(s: &mut tokio::io::DuplexStream) -> String {
                let mut buf = Vec::new();
                loop {
                    let mut b = [0u8; 1];
                    if s.read_exact(&mut b).await.is_err() {
                        break;
                    }
                    if b[0] == b'\n' {
                        break;
                    }
                    if b[0] != b'\r' {
                        buf.push(b[0]);
                    }
                }
                String::from_utf8_lossy(&buf).into_owned()
            }
            let inbox_m1 = b"Subject: one\r\nMessage-ID: <1@x>\r\n\r\nfirst\r\n".to_vec();
            let inbox_m2 = b"Subject: two\r\nMessage-ID: <2@x>\r\n\r\nsecond\r\n".to_vec();
            let sent_m1 = b"Subject: re\r\nMessage-ID: <3@x>\r\n\r\nreply\r\n".to_vec();
            server.write_all(b"* OK mock ready\r\n").await.unwrap();
            let mut last_select = String::new();
            loop {
                let cmd = line(&mut server).await;
                if cmd.is_empty() {
                    break;
                }
                let tag = cmd.split(' ').next().unwrap_or("").to_owned();
                if cmd.contains("LOGIN") {
                    server
                        .write_all(format!("{tag} OK LOGIN\r\n").as_bytes())
                        .await
                        .unwrap();
                } else if cmd.contains("LIST") {
                    let body = "* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n\
                        * LIST (\\HasNoChildren \\Sent) \"/\" \"Sent\"\r\n\
                        * LIST (\\Noselect \\HasChildren) \"/\" \"[Gmail]\"\r\n";
                    server.write_all(body.as_bytes()).await.unwrap();
                    server
                        .write_all(format!("{tag} OK LIST\r\n").as_bytes())
                        .await
                        .unwrap();
                } else if cmd.contains("SELECT") {
                    last_select = if cmd.contains("Sent") {
                        "Sent"
                    } else {
                        "INBOX"
                    }
                    .to_owned();
                    let n = if last_select == "Sent" { 1 } else { 2 };
                    server
                        .write_all(format!("* {n} EXISTS\r\n{tag} OK [READ-WRITE]\r\n").as_bytes())
                        .await
                        .unwrap();
                } else if cmd.contains("FETCH") {
                    if last_select == "INBOX" {
                        for (seq, flags, m) in [
                            (1u32, "()", &inbox_m1),
                            (2, "(\\Seen \\Flagged)", &inbox_m2),
                        ] {
                            server
                                .write_all(
                                    format!(
                                        "* {seq} FETCH (FLAGS {flags} BODY[] {{{}}}\r\n",
                                        m.len()
                                    )
                                    .as_bytes(),
                                )
                                .await
                                .unwrap();
                            server.write_all(m).await.unwrap();
                            server.write_all(b")\r\n").await.unwrap();
                        }
                    } else {
                        server
                            .write_all(
                                format!(
                                    "* 1 FETCH (FLAGS (\\Answered) BODY[] {{{}}}\r\n",
                                    sent_m1.len()
                                )
                                .as_bytes(),
                            )
                            .await
                            .unwrap();
                        server.write_all(&sent_m1).await.unwrap();
                        server.write_all(b")\r\n").await.unwrap();
                    }
                    server
                        .write_all(format!("{tag} OK FETCH\r\n").as_bytes())
                        .await
                        .unwrap();
                } else if cmd.contains("LOGOUT") {
                    // Best-effort on the client; tolerate a closed pipe.
                    let _ = server
                        .write_all(format!("{tag} OK BYE\r\n").as_bytes())
                        .await;
                    break;
                }
            }
        });

        let folders = fetch_folders(client, "me@x.eu", "pw", 50).await.unwrap();
        mock.await.unwrap();

        assert_eq!(folders.len(), 2, "INBOX and Sent, [Gmail] skipped");
        assert_eq!(
            folders[0].target,
            FolderTarget::Role {
                role: "inbox",
                name: "Inbox"
            }
        );
        assert_eq!(folders[0].messages.len(), 2);
        assert!(!folders[0].messages[0].flags.seen);
        assert!(folders[0].messages[1].flags.seen && folders[0].messages[1].flags.flagged);
        assert_eq!(
            folders[1].target,
            FolderTarget::Role {
                role: "sent",
                name: "Sent"
            }
        );
        assert!(folders[1].messages[0].flags.answered);
        assert!(String::from_utf8_lossy(&folders[1].messages[0].raw).contains("reply"));
    }

    #[tokio::test]
    async fn login_failure_maps_to_auth_error() {
        let (client, mut server) = tokio::io::duplex(4096);
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            server.write_all(b"* OK ready\r\n").await.unwrap();
            let mut buf = [0u8; 256];
            let _ = server.read(&mut buf).await;
            server
                .write_all(b"a1 NO [AUTHENTICATIONFAILED] bad\r\n")
                .await
                .unwrap();
        });
        let err = fetch_folders(client, "u", "wrong", 10).await.unwrap_err();
        assert!(matches!(err, ImportError::Auth), "{err:?}");
    }
}
