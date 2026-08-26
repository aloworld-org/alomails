//! TCP/TLS transport: listeners per role (MX, submission, implicit
//! TLS), per-connection wire I/O, the STARTTLS upgrade and SASL AUTH
//! dialog, and the read-side limits and timeouts RFC 5321 requires.
//!
//! All protocol *decisions* live in [`crate::session`]; DATA content
//! collection lives in [`crate::data`]. This module performs the I/O
//! the session's [`Directive`]s imply and stitches everything to the
//! spool.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tracing::Instrument;

use crate::auth::{self, AuthIdentity, Mechanism};
use crate::authmail::{AuthMail, InboundOutcome, SigningConfig};
use crate::config::SmtpConfig;
use crate::data::{self, DataError};
use crate::envelope::Envelope;
use crate::error::SmtpError;
use crate::line::{RawLine, read_raw_line};
use crate::reply::Reply;
use crate::session::{Action, Directive, Role, Session, SessionParams};
use crate::spool::Spool;
use crate::stream::SmtpStream;
use crate::{received, submission, tls};
use alo_auth_mail::dkim::keystore::{FileKeyStore, KeyAlgorithm};
use alo_auth_mail::resolver::Resolver as AuthResolver;
use alo_identity::{Identity, IdentityConfig};

/// Command line limit: 512 octets including CRLF (RFC 5321 §4.5.3.1.4).
const MAX_COMMAND_LINE: usize = 512;
/// Hard ceiling on octets drained hunting for a line ending.
const FLOOD_LIMIT: usize = 64 * 1024;
/// Idle timeout awaiting a command (RFC 5321 §4.5.3.2).
const COMMAND_TIMEOUT: Duration = Duration::from_secs(300);
/// Total budget for receiving one message body (anti-flood policy;
/// stricter than §4.5.3.2 per-block, recorded in docs/interop.md).
const DATA_TIMEOUT: Duration = Duration::from_secs(600);
/// Budget for writing one reply so a non-reading peer cannot pin us.
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
/// TLS handshake budget.
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
/// Delay before re-accepting after a transient `accept()` error.
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);
/// Failed AUTH attempts tolerated on one connection before it is dropped
/// (RFC 4954 §4 anti-brute-force). AUTH now verifies through
/// `alo-identity` (constant-time argon2); cross-connection throttling of
/// the SMTP AUTH path is a future hardening (the OAuth/token endpoints
/// already carry per-(client, username) backoff).
const MAX_AUTH_FAILURES: u32 = 3;

/// A buffered SMTP connection (plaintext or TLS). `BufReader` supplies
/// both bounded line reading and pass-through writing.
type Conn = BufReader<SmtpStream>;

/// Everything a listener needs to serve one connection.
pub struct Runtime {
    /// Per-listener session template (role, TLS/auth policy).
    params: SessionParams,
    /// Durable spool.
    spool: Arc<Spool>,
    /// TLS acceptor for STARTTLS upgrades and implicit TLS.
    tls_acceptor: Arc<TlsAcceptor>,
    /// Credential authority for submission AUTH. `None` on roles that
    /// never authenticate (MX).
    identity: Option<Identity>,
    /// Concurrent-connection cap.
    max_connections: usize,
    /// Per-source-IP concurrent-connection cap (`0` disables). Set from
    /// config in [`run`]; constructors default it off.
    max_connections_per_ip: usize,
    /// Apply RFC 6409 submission fixups before spooling (submission).
    apply_fixups: bool,
    /// Trust stack: SPF/DKIM/DMARC verification (MX) and DKIM signing
    /// (submission). Disabled by default; `run` installs a real one.
    auth: Arc<AuthMail>,
    /// Local delivery into the store (MX only). `None` keeps the receive-
    /// only spool behaviour.
    local_delivery: Option<Arc<crate::local_delivery::LocalDelivery>>,
}

impl Runtime {
    /// Assembles a listener runtime from its parts. `apply_fixups`
    /// should be true only for submission roles (RFC 6409). Used by
    /// [`run`] and by embedders/tests that drive a single [`serve`].
    pub fn new(
        params: SessionParams,
        spool: Arc<Spool>,
        tls_acceptor: Arc<TlsAcceptor>,
        identity: Option<Identity>,
        max_connections: usize,
        apply_fixups: bool,
    ) -> Self {
        let auth = Arc::new(AuthMail::disabled(&params.hostname));
        Self {
            params,
            spool,
            tls_acceptor,
            identity,
            max_connections,
            max_connections_per_ip: 0,
            apply_fixups,
            auth,
            local_delivery: None,
        }
    }

    /// Installs local delivery into the store (MX only). Attaching it turns
    /// on recipient resolution (an unknown local user → 550 at RCPT).
    #[must_use]
    pub fn with_local_delivery(
        mut self,
        local_delivery: Option<Arc<crate::local_delivery::LocalDelivery>>,
    ) -> Self {
        self.params.resolve_local_recipients = local_delivery.is_some();
        self.local_delivery = local_delivery;
        self
    }

    /// Installs the trust-stack context (SPF/DKIM/DMARC + signing).
    #[must_use]
    pub fn with_auth(mut self, auth: Arc<AuthMail>) -> Self {
        self.auth = auth;
        self
    }

    /// Sets the per-source-IP concurrent-connection cap (`0` disables).
    #[must_use]
    pub fn with_max_connections_per_ip(mut self, max: usize) -> Self {
        self.max_connections_per_ip = max;
        self
    }

    /// Builds an MX (relay-in) runtime: STARTTLS offered, no AUTH,
    /// no submission fixups.
    #[allow(clippy::too_many_arguments)]
    pub fn mx(
        hostname: impl Into<String>,
        spool: Arc<Spool>,
        tls_acceptor: Arc<TlsAcceptor>,
        identity: Option<Identity>,
        max_message_size: usize,
        max_rcpt: usize,
        max_connections: usize,
    ) -> Self {
        let params = SessionParams {
            hostname: hostname.into(),
            max_rcpt,
            max_message_size,
            role: Role::Mx,
            tls_available: true,
            tls_active: false,
            require_auth: false,
            require_tls_before_mail: false,
            // Constructor default: accept all recipients. `run` sets the
            // configured allowlist; embedders/tests that need the
            // anti-relay guard call `with_local_domains`.
            local_domains: Vec::new(),
            resolve_local_recipients: false,
        };
        Self::new(
            params,
            spool,
            tls_acceptor,
            identity,
            max_connections,
            false,
        )
    }

    /// Sets the MX hosted-domains allowlist (anti-open-relay guard).
    #[must_use]
    pub fn with_local_domains(mut self, domains: Vec<String>) -> Self {
        self.params.local_domains = domains;
        self
    }

    /// Builds a submission runtime: AUTH required over TLS, RFC 6409
    /// fixups applied. `implicit_tls` selects port-465 semantics
    /// (TLS from the first byte) versus STARTTLS on 587.
    #[allow(clippy::too_many_arguments)]
    pub fn submission(
        hostname: impl Into<String>,
        spool: Arc<Spool>,
        tls_acceptor: Arc<TlsAcceptor>,
        identity: Option<Identity>,
        implicit_tls: bool,
        max_message_size: usize,
        max_rcpt: usize,
        max_connections: usize,
    ) -> Self {
        let hostname = hostname.into();
        let auth = Arc::new(AuthMail::disabled(&hostname));
        Self {
            params: SessionParams {
                hostname,
                max_rcpt,
                max_message_size,
                role: Role::Submission,
                tls_available: true,
                tls_active: implicit_tls,
                require_auth: true,
                require_tls_before_mail: true,
                // Irrelevant on submission: an authenticated user
                // relays anywhere (the relay guard is MX-only).
                local_domains: Vec::new(),
                resolve_local_recipients: false,
            },
            spool,
            tls_acceptor,
            identity,
            max_connections,
            max_connections_per_ip: 0,
            apply_fixups: true,
            auth,
            local_delivery: None,
        }
    }
}

/// Binds every configured listener, starts the outbound queue if
/// enabled, and serves forever.
///
/// # Errors
/// [`SmtpError::Bind`] / [`SmtpError::Spool`] / [`SmtpError::Tls`] /
/// [`SmtpError::Config`] during startup; once running, per-connection
/// failures are logged and never fatal.
pub async fn run(config: SmtpConfig) -> Result<(), SmtpError> {
    let spool = Arc::new(
        Spool::new(&config.spool_dir).map_err(|source| SmtpError::Spool {
            path: config.spool_dir.display().to_string(),
            source,
        })?,
    );

    // Local delivery into the store (MX only). The ARC sealer (built
    // below, once the trust stack exists) is attached and the one-shot
    // spool → store migration is run BEFORE the outbound queue runner
    // starts, so there is no concurrent claim on the spool.
    let local_delivery = match &config.database_url {
        Some(url) => {
            let ld = crate::local_delivery::LocalDelivery::connect(
                url,
                &config.blob_dir,
                Arc::clone(&spool),
                config.hostname.clone(),
            )
            .await?;
            tracing::info!("local delivery into the store is enabled");
            Some(ld)
        }
        None => None,
    };

    let (cert, key) = match &config.tls {
        Some(paths) => (Some(paths.cert.as_path()), Some(paths.key.as_path())),
        None => (None, None),
    };
    let tls_acceptor = Arc::new(tls::build_acceptor(
        cert,
        key,
        &config.hostname,
        config.allow_self_signed,
    )?);

    // Submission AUTH verifies through alo-identity over the store. It
    // is available only when a database is configured (identity is
    // DB-backed); a submission listener without it is a config error.
    let identity: Option<Identity> = match &local_delivery {
        Some(ld) => {
            let cfg = IdentityConfig::from_env()
                .unwrap_or_else(|_| IdentityConfig::new(format!("https://{}", config.hostname)));
            Some(
                Identity::new(ld.store().clone(), cfg).map_err(|e| SmtpError::Config {
                    message: format!("identity initialisation failed: {e}"),
                })?,
            )
        }
        None => None,
    };
    if identity.is_none()
        && (config.submission_addr.is_some() || config.implicit_tls_addr.is_some())
    {
        return Err(SmtpError::Config {
            message: "submission listeners require DATABASE_URL (alo-identity backs AUTH)"
                .to_owned(),
        });
    }

    // Trust stack (M4): one shared DNS resolver for SPF/DKIM/DMARC. If
    // it cannot be built, verification is disabled (mail still flows —
    // an absent resolver must not stop receiving).
    let auth_resolver: Option<Arc<dyn AuthResolver>> =
        match alo_auth_mail::resolver::DnsResolver::from_system() {
            Ok(resolver) => Some(Arc::new(resolver)),
            Err(error) => {
                tracing::error!(%error, "trust-stack resolver unavailable; SPF/DKIM/DMARC disabled");
                None
            }
        };
    let mx_auth = Arc::new(build_mx_auth(&config, auth_resolver.clone()));
    let dkim_store = local_delivery.as_ref().map(|ld| ld.store().clone());
    let submission_auth = Arc::new(build_submission_auth(
        &config,
        auth_resolver.clone(),
        dkim_store,
    )?);

    // ARC sealing for Sieve redirects (RFC 8617): the submission
    // trust-stack context holds the signing keys (per-tenant store +
    // configured fallback). Then run the spool → store migration —
    // still before the queue runner spawns below.
    let local_delivery = match local_delivery {
        Some(ld) => {
            let ld = if config.arc_sealing {
                tracing::info!("ARC sealing of Sieve redirects enabled");
                Arc::new(ld.with_arc_sealer(Arc::clone(&submission_auth)))
            } else {
                tracing::warn!(
                    "ARC sealing disabled ({}) — forwards will fail downstream DMARC",
                    crate::config::ENV_ARC_SEALING
                );
                Arc::new(ld)
            };
            if let Err(error) = ld.migrate_spool().await {
                tracing::error!(%error, "spool → store migration failed; continuing");
            }
            Some(ld)
        }
        None => None,
    };

    if let Some(outbound) = config.outbound.clone() {
        crate::queue_runner::spawn(Arc::clone(&spool), config.hostname.clone(), outbound);
    } else {
        tracing::warn!("outbound delivery disabled; received mail accumulates in the spool");
    }

    // DMARC aggregate-report delivery (RFC 7489 §7.2): needs the event
    // store (local delivery), the outbound queue to carry the reports,
    // and DNS for policy discovery + destination verification.
    if config.dmarc_reports {
        if let (Some(ld), Some(resolver), true) =
            (&local_delivery, &auth_resolver, config.outbound.is_some())
        {
            let report_domain = config
                .local_domains
                .first()
                .cloned()
                .unwrap_or_else(|| config.hostname.clone());
            crate::dmarc_reporter::spawn(
                ld.store().clone(),
                Arc::clone(&spool),
                Arc::clone(resolver),
                Arc::clone(&submission_auth),
                crate::dmarc_reporter::ReporterConfig {
                    org_name: config.hostname.clone(),
                    report_from: format!("dmarc-reports@{report_domain}"),
                    min_age: config.dmarc_report_min_age,
                    tick: config.dmarc_report_tick,
                },
            );
        } else {
            tracing::info!(
                "DMARC aggregate reporting inactive (needs DATABASE_URL, outbound, and DNS)"
            );
        }
    } else {
        tracing::warn!(
            "DMARC aggregate reporting disabled ({})",
            crate::config::ENV_DMARC_REPORTS
        );
    }

    // Optional submission listeners bind first and run as spawned
    // tasks; the MX listener runs on this task (and never returns).
    if let Some(addr) = config.submission_addr {
        let listener = bind(addr).await?;
        let runtime = Arc::new(
            submission_runtime(&config, &spool, &tls_acceptor, &identity, false)
                .with_auth(Arc::clone(&submission_auth)),
        );
        tracing::info!(%addr, "submission (STARTTLS) listener");
        tokio::spawn(serve(listener, runtime));
    }
    if let Some(addr) = config.implicit_tls_addr {
        let listener = bind(addr).await?;
        let runtime = Arc::new(
            submission_runtime(&config, &spool, &tls_acceptor, &identity, true)
                .with_auth(Arc::clone(&submission_auth)),
        );
        tracing::info!(%addr, "implicit-TLS submission listener");
        tokio::spawn(serve(listener, runtime));
    }
    // Trusted internal submission listener (no auth): the full submission
    // pipeline (RFC 6409 fixups + DKIM signing + spool) but with AUTH and the
    // TLS-before-mail requirement disabled. It exists solely for the co-located
    // `alo-jmap`, which authenticates the user and binds MAIL FROM to that
    // user. It MUST be network-isolated (never a published port) — anything
    // that reaches it can relay outbound. See docs/design/email-submission.md.
    if let Some(addr) = config.internal_submission_addr {
        let listener = bind(addr).await?;
        let mut runtime = submission_runtime(&config, &spool, &tls_acceptor, &identity, false);
        runtime.params.require_auth = false;
        runtime.params.require_tls_before_mail = false;
        let runtime = Arc::new(runtime.with_auth(Arc::clone(&submission_auth)));
        tracing::warn!(
            %addr,
            "TRUSTED internal submission listener (no auth) — must be network-isolated, never published"
        );
        tokio::spawn(serve(listener, runtime));
    }

    // MTA-STS policy endpoint (M4b): serves the rendered policy over
    // plaintext HTTP behind the deploy TLS-terminating proxy.
    if let Some(mta_sts) = &config.mta_sts {
        let listener = bind(mta_sts.addr).await?;
        let policy: Arc<str> = Arc::from(mta_sts.policy.render());
        tracing::info!(
            addr = %mta_sts.addr,
            id = %mta_sts.policy.id(),
            "MTA-STS policy listener"
        );
        crate::mta_sts::spawn(listener, policy);
    }

    let mx_listener = bind(config.bind_addr).await?;
    let mx_runtime = Arc::new(Runtime {
        params: SessionParams {
            hostname: config.hostname.clone(),
            max_rcpt: config.max_rcpt,
            max_message_size: config.max_message_size,
            role: Role::Mx,
            tls_available: true,
            tls_active: false,
            require_auth: false,
            require_tls_before_mail: false,
            local_domains: config.local_domains.clone(),
            resolve_local_recipients: config.database_url.is_some(),
        },
        spool: Arc::clone(&spool),
        tls_acceptor: Arc::clone(&tls_acceptor),
        identity: identity.clone(),
        max_connections: config.max_connections,
        max_connections_per_ip: config.max_connections_per_ip,
        apply_fixups: false,
        auth: mx_auth,
        local_delivery: local_delivery.clone(),
    });
    tracing::info!(
        addr = %config.bind_addr,
        hostname = %config.hostname,
        spool = %config.spool_dir.display(),
        outbound = config.outbound.is_some(),
        "alo-smtp MX listener"
    );
    serve(mx_listener, mx_runtime).await
}

/// Builds a submission `Runtime` (STARTTLS or implicit TLS).
fn submission_runtime(
    config: &SmtpConfig,
    spool: &Arc<Spool>,
    tls_acceptor: &Arc<TlsAcceptor>,
    identity: &Option<Identity>,
    implicit_tls: bool,
) -> Runtime {
    Runtime {
        params: SessionParams {
            hostname: config.hostname.clone(),
            max_rcpt: config.max_rcpt,
            max_message_size: config.max_message_size,
            role: Role::Submission,
            tls_available: true,
            tls_active: implicit_tls,
            require_auth: true,
            require_tls_before_mail: true,
            // Submission relays for authenticated users to any domain;
            // the MX allowlist does not apply here.
            local_domains: Vec::new(),
            resolve_local_recipients: false,
        },
        spool: Arc::clone(spool),
        tls_acceptor: Arc::clone(tls_acceptor),
        identity: identity.clone(),
        max_connections: config.max_connections,
        max_connections_per_ip: config.max_connections_per_ip,
        apply_fixups: true,
        auth: Arc::new(AuthMail::disabled(&config.hostname)),
        local_delivery: None,
    }
}

/// Builds the MX trust-stack context: SPF/DKIM/DMARC verification plus
/// Rspamd spam scoring when configured.
fn build_mx_auth(config: &SmtpConfig, resolver: Option<Arc<dyn AuthResolver>>) -> AuthMail {
    let mut auth = AuthMail::disabled(&config.hostname);
    if let Some(resolver) = resolver {
        auth = auth.with_resolver(resolver);
    }
    if let Some(rspamd) = &config.rspamd {
        // The client was built and validated at config load — no second,
        // fail-open parse here in a fail-closed feature.
        auth = auth.with_rspamd(Arc::clone(&rspamd.client));
        tracing::info!(url = %rspamd.url, "Rspamd spam scoring enabled (fail-closed)");
    }
    if let Some(clamav) = &config.clamav {
        auth = auth.with_clamav(Arc::clone(&clamav.client));
        tracing::info!(addr = %clamav.addr, "ClamAV malware scanning enabled (fail-closed)");
    }
    auth
}

/// Builds the submission trust-stack context: verification (harmless on
/// submission) plus DKIM signing when configured.
fn build_submission_auth(
    config: &SmtpConfig,
    resolver: Option<Arc<dyn AuthResolver>>,
    store: Option<Arc<alo_store::Store>>,
) -> Result<AuthMail, SmtpError> {
    let mut auth = AuthMail::disabled(&config.hostname);
    if let Some(resolver) = resolver {
        auth = auth.with_resolver(resolver);
    }
    // Per-tenant DKIM keys (ADR 0014): resolve the signing key by the From
    // domain, with the configured file key below as the fallback.
    if let Some(store) = store {
        auth = auth.with_dkim_store(store);
    }
    if let Some(dkim) = &config.dkim {
        let algorithm = if dkim.ed25519 {
            KeyAlgorithm::Ed25519Sha256
        } else {
            KeyAlgorithm::RsaSha256
        };
        let keys = FileKeyStore::new().with_key(
            &dkim.domain,
            &dkim.selector,
            dkim.key_path.clone(),
            algorithm,
        );
        auth = auth.with_signing(SigningConfig {
            keys: Arc::new(keys),
            domain: dkim.domain.clone(),
            selector: dkim.selector.clone(),
        });
        tracing::info!(domain = %dkim.domain, selector = %dkim.selector, "DKIM signing enabled");
    }
    Ok(auth)
}

async fn bind(addr: SocketAddr) -> Result<TcpListener, SmtpError> {
    TcpListener::bind(addr)
        .await
        .map_err(|source| SmtpError::Bind { addr, source })
}

/// Accept loop over a bound listener (also the seam integration tests
/// use). Bounds concurrent sessions so one host cannot pin unlimited
/// tasks; connection #cap+1 is greeted with 421 and dropped.
///
/// # Errors
/// Never returns an error today; the `Result` is the stable signature
/// for when graceful shutdown lands.
pub async fn serve(listener: TcpListener, runtime: Arc<Runtime>) -> Result<(), SmtpError> {
    let limiter = Arc::new(tokio::sync::Semaphore::new(runtime.max_connections));
    let per_ip = crate::connlimit::PerIpLimiter::new(runtime.max_connections_per_ip);
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let span = tracing::info_span!("smtp_session", %peer);
                // Per-IP cap first (a single host cannot consume the
                // whole global pool), then the global cap. Both refuse
                // with a transient 421 so a legitimate sender retries.
                let Some(ip_guard) = per_ip.admit(peer.ip()) else {
                    let hostname = runtime.params.hostname.clone();
                    tokio::spawn(
                        async move {
                            tracing::warn!(%peer, "per-IP connection limit reached; refusing with 421");
                            let mut stream = stream;
                            let _best_effort = stream
                                .write_all(Reply::service_closing(&hostname).to_string().as_bytes())
                                .await;
                        }
                        .instrument(span),
                    );
                    continue;
                };
                match Arc::clone(&limiter).try_acquire_owned() {
                    Ok(permit) => {
                        let runtime = Arc::clone(&runtime);
                        tokio::spawn(
                            async move {
                                let _permit = permit;
                                let _ip_guard = ip_guard;
                                if let Err(error) = accept_connection(stream, peer, runtime).await {
                                    tracing::debug!(%error, "session ended with I/O error");
                                }
                            }
                            .instrument(span),
                        );
                    }
                    Err(_no_permit) => {
                        let hostname = runtime.params.hostname.clone();
                        tokio::spawn(
                            async move {
                                let _ip_guard = ip_guard;
                                tracing::warn!("connection limit reached; refusing with 421");
                                let mut stream = stream;
                                let _best_effort = stream
                                    .write_all(
                                        Reply::service_closing(&hostname).to_string().as_bytes(),
                                    )
                                    .await;
                            }
                            .instrument(span),
                        );
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "accept failed; retrying");
                tokio::time::sleep(ACCEPT_RETRY_DELAY).await;
            }
        }
    }
}

/// Wraps the accepted TCP stream (performing the implicit-TLS
/// handshake when this listener requires it), then drives the session.
async fn accept_connection(
    tcp: TcpStream,
    peer: SocketAddr,
    runtime: Arc<Runtime>,
) -> std::io::Result<()> {
    let stream = if runtime.params.tls_active {
        match tokio::time::timeout(TLS_HANDSHAKE_TIMEOUT, runtime.tls_acceptor.accept(tcp)).await {
            Ok(Ok(tls)) => SmtpStream::Tls(Box::new(tls)),
            Ok(Err(error)) => {
                tracing::info!(%error, "implicit TLS handshake failed");
                return Ok(());
            }
            Err(_elapsed) => {
                tracing::info!("implicit TLS handshake timed out");
                return Ok(());
            }
        }
    } else {
        SmtpStream::Plain(tcp)
    };
    handle_connection(stream, peer, runtime).await
}

/// Drives one connection: greeting, then command/reply/DATA and the
/// STARTTLS/AUTH sub-dialogs until close.
async fn handle_connection(
    stream: SmtpStream,
    peer: SocketAddr,
    runtime: Arc<Runtime>,
) -> std::io::Result<()> {
    let mut conn: Conn = BufReader::new(stream);
    let mut session = Session::new(runtime.params.clone());

    tracing::info!(tls = conn.get_ref().is_tls(), "session opened");
    write_reply(&mut conn, &session.greeting()).await?;

    let mut auth_failures: u32 = 0;
    loop {
        let outcome = match tokio::time::timeout(
            COMMAND_TIMEOUT,
            read_raw_line(&mut conn, MAX_COMMAND_LINE, FLOOD_LIMIT),
        )
        .await
        {
            Ok(result) => result?,
            Err(_elapsed) => {
                tracing::info!("idle timeout; closing");
                write_reply(&mut conn, &Reply::service_closing(&runtime.params.hostname)).await?;
                break;
            }
        };

        let bytes = match outcome {
            RawLine::Line(bytes) => bytes,
            RawLine::TooLong { .. } => {
                write_reply(&mut conn, &Reply::line_too_long()).await?;
                continue;
            }
            RawLine::BadEol => {
                write_reply(&mut conn, &Reply::bare_line_ending()).await?;
                continue;
            }
            RawLine::Flooded => {
                tracing::warn!("peer flooded without newline; closing");
                write_reply(&mut conn, &Reply::service_closing(&runtime.params.hostname)).await?;
                break;
            }
            RawLine::Eof => {
                tracing::info!("peer disconnected");
                break;
            }
        };

        // Commands are ASCII (§2.4); SMTPUTF8 is not yet advertised.
        if !bytes.is_ascii() {
            write_reply(&mut conn, &Reply::command_unrecognized()).await?;
            continue;
        }
        let line = String::from_utf8_lossy(&bytes).into_owned();

        match session.on_line(&line) {
            Directive::Respond(reply, action) => {
                write_reply(&mut conn, &reply).await?;
                match action {
                    Action::Continue => {}
                    Action::Close => {
                        tracing::info!("session closed by QUIT");
                        break;
                    }
                    Action::EnterData => {
                        if !handle_data_phase(&mut conn, &mut session, peer, &runtime).await? {
                            break;
                        }
                    }
                }
            }
            Directive::StartTls => {
                if !do_starttls(&mut conn, &mut session, &runtime).await? {
                    break;
                }
            }
            Directive::Authenticate { mechanism, initial } => {
                let authenticated =
                    do_auth(&mut conn, &mut session, &runtime, mechanism, initial).await?;
                if !authenticated {
                    auth_failures += 1;
                    if auth_failures >= MAX_AUTH_FAILURES {
                        tracing::warn!("too many failed AUTH attempts; closing");
                        write_reply(
                            &mut conn,
                            &Reply::too_many_auth_failures(&runtime.params.hostname),
                        )
                        .await?;
                        break;
                    }
                }
            }
            Directive::CheckRecipient { email } => {
                // Resolve the local recipient against the store; unknown →
                // 550 5.1.1 at RCPT (never a silent post-DATA drop). Absent a
                // store (invariant: the flag is only set with one), accept.
                let exists = match &runtime.local_delivery {
                    Some(ld) => ld.recipient_exists(&email).await,
                    None => true,
                };
                let reply = session.resolve_pending(exists);
                write_reply(&mut conn, &reply).await?;
            }
        }
    }
    Ok(())
}

/// Performs the STARTTLS upgrade (RFC 3207). Returns whether the
/// session may continue.
async fn do_starttls(
    conn: &mut Conn,
    session: &mut Session,
    runtime: &Arc<Runtime>,
) -> std::io::Result<bool> {
    write_reply(conn, &Reply::tls_ready()).await?;

    // RFC 3207 §5: any buffered plaintext after the 220 and before the
    // handshake is a command-injection attempt across the TLS boundary
    // — drop the connection, process nothing.
    if !conn.buffer().is_empty() {
        tracing::warn!("plaintext buffered after STARTTLS; possible injection — closing");
        return Ok(false);
    }

    // Take the underlying plaintext stream out and wrap it in TLS.
    // The `Closed` placeholder is swapped in only long enough to move
    // the real stream out; it is never read or written.
    let inner = std::mem::replace(conn, BufReader::new(SmtpStream::Closed));
    let SmtpStream::Plain(tcp) = inner.into_inner() else {
        tracing::error!("STARTTLS on an already-encrypted connection — invariant broken");
        return Ok(false);
    };
    let tls =
        match tokio::time::timeout(TLS_HANDSHAKE_TIMEOUT, runtime.tls_acceptor.accept(tcp)).await {
            Ok(Ok(tls)) => tls,
            Ok(Err(error)) => {
                tracing::info!(%error, "STARTTLS handshake failed; closing");
                return Ok(false);
            }
            Err(_elapsed) => {
                tracing::info!("STARTTLS handshake timed out; closing");
                return Ok(false);
            }
        };
    *conn = BufReader::new(SmtpStream::Tls(Box::new(tls)));
    session.reset_after_starttls();
    tracing::info!("STARTTLS established");
    Ok(true)
}

/// Runs the SASL exchange for `mechanism` and records the identity on
/// success. Returns whether authentication succeeded; all failures
/// reply and leave the session unauthenticated.
async fn do_auth(
    conn: &mut Conn,
    session: &mut Session,
    runtime: &Arc<Runtime>,
    mechanism: Mechanism,
    initial: Option<String>,
) -> std::io::Result<bool> {
    if mechanism == Mechanism::XOAuth2 {
        return do_auth_xoauth2(conn, session, runtime, initial).await;
    }
    let credentials = match collect_credentials(conn, mechanism, initial).await? {
        Ok(credentials) => credentials,
        Err(reply) => {
            write_reply(conn, &reply).await?;
            return Ok(false);
        }
    };
    // AUTH is only offered on submission roles, which always carry an
    // identity; treat an absent one as a failed auth rather than panic.
    let Some(identity) = runtime.identity.as_ref() else {
        tracing::info!("authentication failed (no credential authority)");
        write_reply(conn, &Reply::auth_failed()).await?;
        return Ok(false);
    };
    // Legacy-protocol auth: constant-time verify of the primary or an app
    // password; a 2FA account's primary is refused (fail closed — app
    // password or OIDC instead), with per-username backoff on top of the
    // per-connection cap. See docs/design/identity.md.
    match identity
        .authenticate_legacy(&credentials.username, &credentials.password)
        .await
    {
        Ok(Some(_principal)) => {
            // The login is personal data (CLAUDE.md law 1): keep it out of
            // default-visible logs; available at debug for audit.
            tracing::debug!(user = %credentials.username, "authentication succeeded");
            tracing::info!("authentication succeeded");
            session.set_authenticated(AuthIdentity::new(credentials.username.clone()));
            write_reply(conn, &Reply::auth_ok()).await?;
            Ok(true)
        }
        Ok(None) => {
            // No log of which of user/password was wrong (§7.3).
            tracing::info!("authentication failed");
            write_reply(conn, &Reply::auth_failed()).await?;
            Ok(false)
        }
        Err(_) => {
            // A store fault is a temporary condition, not a credential
            // rejection — reply 454 so the client may retry.
            tracing::warn!("authentication backend error");
            write_reply(conn, &Reply::auth_temporary_failure()).await?;
            Ok(false)
        }
    }
}

/// Runs the `AUTH XOAUTH2` exchange: one base64 blob carrying an asserted
/// login name and an OAuth bearer token, verified through the
/// introspection seam (ADR 0025) — this is how an OAuth-capable client
/// submits mail without an app password. On a credential failure the
/// mechanism's own error dialog runs first: a `334` carrying a base64
/// error status, which the client acknowledges with one (empty) line
/// before the final `535` (the de-facto XOAUTH2 contract — see
/// `docs/interop.md`).
async fn do_auth_xoauth2(
    conn: &mut Conn,
    session: &mut Session,
    runtime: &Arc<Runtime>,
    initial: Option<String>,
) -> std::io::Result<bool> {
    let payload = match initial {
        Some(ir) => ir,
        None => {
            write_reply(conn, &Reply::auth_challenge("")).await?;
            match read_sasl_line(conn).await? {
                Some(line) => line,
                None => {
                    write_reply(conn, &Reply::auth_malformed()).await?;
                    return Ok(false);
                }
            }
        }
    };
    if payload == "*" {
        write_reply(conn, &Reply::auth_malformed()).await?; // client cancelled
        return Ok(false);
    }
    let response = match auth::decode_xoauth2(decode_ir_marker(&payload)) {
        Ok(response) => response,
        Err(_) => {
            write_reply(conn, &Reply::auth_malformed()).await?;
            return Ok(false);
        }
    };
    let Some(identity) = runtime.identity.as_ref() else {
        tracing::info!("authentication failed (no credential authority)");
        write_reply(conn, &Reply::auth_failed()).await?;
        return Ok(false);
    };
    match identity
        .authenticate_xoauth2(&response.username, &response.token)
        .await
    {
        Ok(Some(_principal)) => {
            // The login is personal data (CLAUDE.md law 1): keep it out of
            // default-visible logs; available at debug for audit.
            tracing::debug!(user = %response.username, "authentication succeeded");
            tracing::info!("authentication succeeded");
            session.set_authenticated(AuthIdentity::new(response.username.clone()));
            write_reply(conn, &Reply::auth_ok()).await?;
            Ok(true)
        }
        Ok(None) => {
            // Unknown, expired, revoked, and wrong-user tokens all land
            // here indistinguishably (no oracle). The client's
            // acknowledgement line's content is irrelevant — the exchange
            // has already failed.
            tracing::info!("authentication failed");
            write_reply(
                conn,
                &Reply::auth_challenge(&alo_identity::xoauth2::error_status_b64()),
            )
            .await?;
            let _ = read_sasl_line(conn).await?;
            write_reply(conn, &Reply::auth_failed()).await?;
            Ok(false)
        }
        Err(_) => {
            tracing::warn!("authentication backend error");
            write_reply(conn, &Reply::auth_temporary_failure()).await?;
            Ok(false)
        }
    }
}

/// Collects credentials from the client for `mechanism`, running the
/// 334-challenge sub-dialog when needed. The inner `Result` is the
/// SASL-level outcome (Ok credentials, or a reply to send).
async fn collect_credentials(
    conn: &mut Conn,
    mechanism: Mechanism,
    initial: Option<String>,
) -> std::io::Result<Result<auth::Credentials, Reply>> {
    // RFC 4954 §4: an initial response of "=" denotes a zero-length
    // response (distinct from "no initial response"). Normalize it so
    // it is not fed to the base64 decoder as literal "=".
    let initial = initial.map(|ir| if ir == "=" { String::new() } else { ir });
    match mechanism {
        Mechanism::Plain => {
            let payload = match initial {
                Some(ir) => ir,
                None => {
                    write_reply(conn, &Reply::auth_challenge("")).await?;
                    match read_sasl_line(conn).await? {
                        Some(line) => line,
                        None => return Ok(Err(Reply::auth_malformed())),
                    }
                }
            };
            if payload == "*" {
                return Ok(Err(Reply::auth_malformed())); // client cancelled
            }
            // RFC 4954 §4: a lone "=" is a zero-length initial response.
            let payload = decode_ir_marker(&payload);
            Ok(auth::decode_plain(payload).map_err(|_| Reply::auth_malformed()))
        }
        Mechanism::Login => {
            // Username: from the initial response if present, else 334.
            let username_b64 = match initial {
                Some(ir) => ir,
                None => {
                    write_reply(conn, &Reply::auth_challenge(auth::LOGIN_USERNAME_CHALLENGE))
                        .await?;
                    match read_sasl_line(conn).await? {
                        Some(line) => line,
                        None => return Ok(Err(Reply::auth_malformed())),
                    }
                }
            };
            if username_b64 == "*" {
                return Ok(Err(Reply::auth_malformed()));
            }
            let username = match auth::decode_login_field(decode_ir_marker(&username_b64)) {
                Ok(u) => u,
                Err(_) => return Ok(Err(Reply::auth_malformed())),
            };
            write_reply(conn, &Reply::auth_challenge(auth::LOGIN_PASSWORD_CHALLENGE)).await?;
            let password_b64 = match read_sasl_line(conn).await? {
                Some(line) => line,
                None => return Ok(Err(Reply::auth_malformed())),
            };
            if password_b64 == "*" {
                return Ok(Err(Reply::auth_malformed()));
            }
            let password = match auth::decode_login_field(decode_ir_marker(&password_b64)) {
                Ok(p) => p,
                Err(_) => return Ok(Err(Reply::auth_malformed())),
            };
            if username.is_empty() {
                return Ok(Err(Reply::auth_malformed()));
            }
            Ok(Ok(auth::Credentials { username, password }))
        }
        // Not a username/password mechanism: `do_auth` routes XOAUTH2 to
        // its own exchange before calling here. Kept as a refusal, not a
        // panic, so a future call-site mistake fails safe on the wire.
        Mechanism::XOAuth2 => Ok(Err(Reply::auth_mechanism_unsupported())),
    }
}

/// Maps the RFC 4954 §4 zero-length-response marker `=` to an empty
/// string; every other value passes through unchanged.
fn decode_ir_marker(field: &str) -> &str {
    if field == "=" { "" } else { field }
}

/// Reads one CRLF-terminated SASL response line (bounded, ASCII).
/// `None` on any read problem (EOF/too-long/bad ending).
async fn read_sasl_line(conn: &mut Conn) -> std::io::Result<Option<String>> {
    let outcome = match tokio::time::timeout(
        COMMAND_TIMEOUT,
        read_raw_line(conn, MAX_COMMAND_LINE, FLOOD_LIMIT),
    )
    .await
    {
        Ok(result) => result?,
        Err(_elapsed) => return Ok(None),
    };
    match outcome {
        RawLine::Line(bytes) if bytes.is_ascii() => {
            Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
        }
        _ => Ok(None),
    }
}

/// Collects one message after 354, applies submission fixups when the
/// role calls for them, stamps `Received:`, and spools it durably.
/// Returns whether the session may continue.
async fn handle_data_phase(
    conn: &mut Conn,
    session: &mut Session,
    peer: SocketAddr,
    runtime: &Arc<Runtime>,
) -> std::io::Result<bool> {
    let max_size = runtime.params.max_message_size;
    let collected =
        match tokio::time::timeout(DATA_TIMEOUT, data::read_message(conn, max_size)).await {
            Ok(result) => result,
            Err(_elapsed) => {
                tracing::info!("DATA timeout; closing");
                session.end_data();
                write_reply(conn, &Reply::service_closing(&runtime.params.hostname)).await?;
                return Ok(false);
            }
        };

    match collected {
        Ok(body) => {
            let Some((mail_from, rcpt_to)) = session.envelope_fields() else {
                tracing::error!("DATA completed without a transaction — invariant broken");
                session.end_data();
                write_reply(conn, &Reply::local_error()).await?;
                return Ok(true);
            };
            let envelope = Envelope {
                helo: session.helo_client().to_owned(),
                peer: peer.to_string(),
                mail_from,
                rcpt_to,
                received_at: jiff::Timestamp::now().to_string(),
            };
            let id = spool_id(&runtime.spool);
            let now = jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC);

            // RFC 6409 §8: on submission, add Date/Message-ID if absent
            // before stamping Received (so the fixups sit under it).
            let mut body = if runtime.apply_fixups {
                submission::apply_fixups(&body, &runtime.params.hostname, &id, &now)
            } else {
                body
            };

            // Trust stack (M4): headers to insert between our Received:
            // stamp and the original message.
            let mut trust_headers = String::new();
            // Inbound (MX): SPF + DKIM + DMARC over the message as
            // received; stamp Received-SPF + Authentication-Results,
            // and honor a DMARC reject policy.
            if runtime.params.role == Role::Mx && runtime.auth.is_active() {
                let result = runtime
                    .auth
                    .inbound(
                        peer.ip(),
                        session.helo_client(),
                        envelope.mail_from.as_deref(),
                        &envelope.rcpt_to,
                        &body,
                    )
                    .await;
                // Record the DMARC evaluation for aggregate reporting
                // (RFC 7489 §7.2) — rejects included, defers excluded
                // (the gauntlet already nulls the event on a defer). A
                // failed insert never affects the SMTP outcome.
                if let (Some(local), Some(event)) = (&runtime.local_delivery, &result.dmarc_event) {
                    let record = alo_store::DmarcEventRecord {
                        from_domain: event.from_domain.clone(),
                        source_ip: peer.ip().to_string(),
                        disposition: event.disposition.to_owned(),
                        dkim_aligned: event.dkim_aligned,
                        spf_aligned: event.spf_aligned,
                    };
                    if let Err(error) = local.store().record_dmarc_event(&record).await {
                        tracing::warn!(%error, "dmarc report event not recorded");
                    }
                }
                // Reject/defer verdicts end the transaction before spool.
                let refusal = match result.outcome {
                    InboundOutcome::Accept => None,
                    InboundOutcome::RejectDmarc => Some(Reply::dmarc_reject()),
                    InboundOutcome::RejectSpam => Some(Reply::spam_reject()),
                    InboundOutcome::RejectVirus => Some(Reply::virus_reject(
                        result.virus.as_deref().unwrap_or("malware"),
                    )),
                    InboundOutcome::DeferSpam => Some(Reply::spam_tempfail()),
                };
                if let Some(reply) = refusal {
                    session.end_data();
                    write_reply(conn, &reply).await?;
                    return Ok(true);
                }
                // RFC 8601 §5: if a forged Authentication-Results /
                // Received-SPF was stripped, spool the rewritten body.
                if let Some(clean) = result.stripped_body {
                    body = clean;
                }
                trust_headers.push_str(&result.headers);
            }
            // Outbound (submission): DKIM-sign the fixed-up message.
            if runtime.apply_fixups
                && let Some(signature) = runtime.auth.sign_outbound(&body).await
            {
                trust_headers.push_str(&signature);
            }

            let header = received::stamp(
                session.helo_client(),
                &peer.ip().to_string(),
                &runtime.params.hostname,
                session.protocol_name(),
                &id,
                &now,
            );
            let mut message = Vec::with_capacity(header.len() + trust_headers.len() + body.len());
            message.extend_from_slice(header.as_bytes());
            message.extend_from_slice(trust_headers.as_bytes());
            message.extend_from_slice(&body);

            // Local delivery (MX + store configured): deliver into the store
            // with Sieve at the boundary, instead of the spool. Every
            // recipient here resolved to a local account at RCPT.
            if let Some(local) = runtime.local_delivery.clone() {
                session.end_data();
                let outcome = local
                    .deliver(&message, envelope.mail_from.as_deref(), &envelope.rcpt_to)
                    .await;
                let reply = match outcome {
                    crate::local_delivery::DeliveryOutcome::Delivered => {
                        tracing::info!(%id, size = body.len(), rcpts = envelope.rcpt_to.len(), "delivered to store");
                        Reply::ok_queued(&id)
                    }
                    crate::local_delivery::DeliveryOutcome::Transient => Reply::delivery_tempfail(),
                };
                write_reply(conn, &reply).await?;
                return Ok(true);
            }

            let spool = Arc::clone(&runtime.spool);
            let id_for_task = id.clone();
            let spool_task =
                tokio::task::spawn_blocking(move || spool.store(&id_for_task, &envelope, &message));
            session.end_data();
            match spool_task.await {
                Ok(Ok(())) => {
                    tracing::info!(%id, size = body.len(), "message accepted");
                    write_reply(conn, &Reply::ok_queued(&id)).await?;
                }
                Ok(Err(error)) => {
                    tracing::error!(%error, "spool write failed");
                    write_reply(conn, &Reply::local_error()).await?;
                }
                Err(join_error) => {
                    tracing::error!(%join_error, "spool task panicked");
                    write_reply(conn, &Reply::local_error()).await?;
                }
            }
            Ok(true)
        }
        Err(DataError::TooLarge) => {
            session.end_data();
            write_reply(conn, &Reply::message_too_large()).await?;
            Ok(true)
        }
        Err(DataError::LineTooLong) => {
            session.end_data();
            write_reply(conn, &Reply::line_too_long()).await?;
            Ok(true)
        }
        Err(DataError::BareLineEnding) => {
            tracing::warn!("bare line ending inside DATA; closing");
            session.end_data();
            write_reply(conn, &Reply::bare_line_ending()).await?;
            Ok(false)
        }
        Err(DataError::Flooded) => {
            tracing::warn!("peer flooded the DATA channel; closing");
            session.end_data();
            write_reply(conn, &Reply::service_closing(&runtime.params.hostname)).await?;
            Ok(false)
        }
        Err(DataError::UnexpectedEof) => {
            tracing::info!("peer disconnected mid-DATA; message discarded");
            session.end_data();
            Ok(false)
        }
        Err(DataError::Io(error)) => {
            session.end_data();
            Err(error)
        }
    }
}

fn spool_id(spool: &Arc<Spool>) -> String {
    spool.next_id()
}

async fn write_reply(conn: &mut Conn, reply: &Reply) -> std::io::Result<()> {
    tokio::time::timeout(WRITE_TIMEOUT, async {
        conn.write_all(reply.to_string().as_bytes()).await?;
        conn.flush().await
    })
    .await
    .map_err(|_elapsed| {
        std::io::Error::new(std::io::ErrorKind::TimedOut, "reply write timed out")
    })?
}
