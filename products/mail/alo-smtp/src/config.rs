//! Runtime configuration for the SMTP service, read from environment.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::egress::EgressMap;
use crate::error::SmtpError;

/// Environment variable naming the socket address to listen on.
pub const ENV_ADDR: &str = "ALO_SMTP_ADDR";
/// Environment variable naming the hostname used in banners/replies.
pub const ENV_HOSTNAME: &str = "ALO_SMTP_HOSTNAME";
/// Environment variable naming the spool directory.
pub const ENV_SPOOL_DIR: &str = "ALO_SMTP_SPOOL_DIR";
/// Environment variable naming the **durable** blob directory for local
/// delivery (message bytes on disk). Defaults to `./blobs`.
pub const ENV_BLOB_DIR: &str = "ALO_SMTP_BLOB_DIR";
/// Environment variable for the maximum message size in octets.
pub const ENV_MAX_MESSAGE_SIZE: &str = "ALO_SMTP_MAX_MESSAGE_SIZE";
/// Environment variable for the per-transaction recipient limit.
pub const ENV_MAX_RCPT: &str = "ALO_SMTP_MAX_RCPT";
/// Environment variable for the concurrent-connection cap.
pub const ENV_MAX_CONNECTIONS: &str = "ALO_SMTP_MAX_CONNECTIONS";
/// Environment variable for the per-source-IP concurrent-connection cap
/// (native inbound abuse control; `0` disables). IPv6 is bucketed by /64.
pub const ENV_MAX_CONNECTIONS_PER_IP: &str = "ALO_SMTP_MAX_CONNECTIONS_PER_IP";
/// Environment flag enabling outbound delivery (off by default — see
/// the relay-safety note on [`OutboundConfig`]).
pub const ENV_OUTBOUND_ENABLED: &str = "ALO_SMTP_OUTBOUND_ENABLED";
/// Environment variable routing all outbound mail to one host.
pub const ENV_SMARTHOST: &str = "ALO_SMTP_SMARTHOST";
/// Environment variable for the retry base delay in seconds.
pub const ENV_RETRY_BASE_SECS: &str = "ALO_SMTP_RETRY_BASE_SECS";
/// Environment variable for the retry cap in seconds.
pub const ENV_RETRY_CAP_SECS: &str = "ALO_SMTP_RETRY_CAP_SECS";
/// Environment variable for the maximum delivery attempts.
pub const ENV_MAX_ATTEMPTS: &str = "ALO_SMTP_MAX_ATTEMPTS";
/// Environment variable for the queue polling interval in seconds.
pub const ENV_QUEUE_INTERVAL_SECS: &str = "ALO_SMTP_QUEUE_INTERVAL_SECS";
/// Environment variable for the outbound per-destination-domain send
/// rate (messages/minute; `0` disables). Protects sending-IP reputation
/// from a compromised account.
pub const ENV_OUTBOUND_RATE_PER_MIN: &str = "ALO_SMTP_OUTBOUND_RATE_PER_MIN";
/// Environment variable for the outbound rate burst depth (max
/// instantaneous messages to one domain; defaults to the per-minute rate).
pub const ENV_OUTBOUND_RATE_BURST: &str = "ALO_SMTP_OUTBOUND_RATE_BURST";
/// Environment variable mapping a sending domain to the local address its mail
/// leaves by: `news.example.com=203.0.113.7`, comma-separated for several
/// (ADR 0044 §1 — a campaign identity's own IP). Unset means the kernel
/// chooses, which is what a deployment with one address does. Matched on the
/// **envelope-from** domain, because that is the identity SPF is evaluated for.
pub const ENV_EGRESS_IPS: &str = "ALO_SMTP_EGRESS_IPS";
/// Environment variable for the submission (STARTTLS) listener address.
pub const ENV_SUBMISSION_ADDR: &str = "ALO_SMTP_SUBMISSION_ADDR";
/// Environment variable for the implicit-TLS submission listener address.
pub const ENV_IMPLICIT_TLS_ADDR: &str = "ALO_SMTP_IMPLICIT_TLS_ADDR";
/// Environment variable for the TRUSTED INTERNAL submission listener address.
///
/// This listener runs the full submission pipeline (RFC 6409 fixups + DKIM +
/// spool) but with NO AUTH — it trusts its caller (the co-located `alo-jmap`,
/// which has already authenticated the user and binds `MAIL FROM` to that
/// user). It MUST be network-isolated: bound inside the container and never
/// published to the host/internet. `None` disables it. See
/// `docs/design/email-submission.md`.
pub const ENV_INTERNAL_SUBMISSION_ADDR: &str = "ALO_SMTP_INTERNAL_SUBMISSION_ADDR";
/// Environment variable for the TLS certificate PEM path.
pub const ENV_TLS_CERT: &str = "ALO_SMTP_TLS_CERT";
/// Environment variable for the TLS private-key PEM path.
pub const ENV_TLS_KEY: &str = "ALO_SMTP_TLS_KEY";
/// Environment variable listing the domains this server hosts
/// (comma-separated). The MX anti-open-relay guard: only these
/// domains' recipients are accepted on port 25.
pub const ENV_LOCAL_DOMAINS: &str = "ALO_SMTP_LOCAL_DOMAINS";
/// Environment flag permitting self-signed certificate generation when
/// no PEM is configured (development only).
pub const ENV_ALLOW_SELF_SIGNED: &str = "ALO_SMTP_ALLOW_SELF_SIGNED";
/// Environment variable for the DKIM signing domain (`d=`).
pub const ENV_DKIM_DOMAIN: &str = "ALO_SMTP_DKIM_DOMAIN";
/// Environment variable for the DKIM selector (`s=`).
pub const ENV_DKIM_SELECTOR: &str = "ALO_SMTP_DKIM_SELECTOR";
/// Environment variable for the DKIM private-key PEM path.
pub const ENV_DKIM_KEY: &str = "ALO_SMTP_DKIM_KEY";
/// Environment variable selecting the DKIM algorithm (`ed25519` or the
/// default `rsa`).
pub const ENV_DKIM_ALGORITHM: &str = "ALO_SMTP_DKIM_ALGORITHM";
/// Environment variable for the second DKIM selector (RFC 8463
/// dual-signing): with [`ENV_DKIM_KEY2`], outbound mail carries one
/// signature per key — the RSA one first, for verifiers that cannot
/// read Ed25519 yet. The pair's algorithms are read from the key
/// files at startup and must differ; publish the second selector's
/// DNS record BEFORE setting these.
pub const ENV_DKIM_SELECTOR2: &str = "ALO_SMTP_DKIM_SELECTOR2";
/// Environment variable for the second DKIM private-key PEM path
/// (see [`ENV_DKIM_SELECTOR2`]).
pub const ENV_DKIM_KEY2: &str = "ALO_SMTP_DKIM_KEY2";
/// Environment flag for ARC sealing of Sieve-redirect forwards
/// (RFC 8617). **Default on**; set to `false`/`off` to disable (the
/// rollback switch — forwards then break downstream DMARC again).
pub const ENV_ARC_SEALING: &str = "ALO_SMTP_ARC_SEALING";
/// Environment flag for DMARC aggregate-report delivery (RFC 7489
/// §7.2). **Default on** (runs only when local delivery + outbound are
/// configured); set to `false`/`off` to disable — the rollback switch.
pub const ENV_DMARC_REPORTS: &str = "ALO_SMTP_DMARC_REPORTS";
/// Environment variable overriding the report window age in seconds:
/// events older than this are reported on the next tick. Unset uses
/// the standard daily cadence (everything before the current UTC day).
/// An override is an operational tool (testing, catch-up drills).
pub const ENV_DMARC_REPORT_MIN_AGE_SECS: &str = "ALO_SMTP_DMARC_REPORT_MIN_AGE_SECS";
/// Environment variable for the reporter tick interval in seconds
/// (default 3600 — hourly sweeps, daily windows).
pub const ENV_DMARC_REPORT_TICK_SECS: &str = "ALO_SMTP_DMARC_REPORT_TICK_SECS";
/// Environment flag for DANE (RFC 7672) on outbound delivery.
/// **Default on**; set to `false`/`off` to disable — outbound TLS then
/// falls back to opportunistic everywhere (the rollback switch).
pub const ENV_DANE: &str = "ALO_SMTP_DANE";
/// Environment variable naming the Rspamd controller URL
/// (`http://host:port`); unset disables spam scanning.
pub const ENV_RSPAMD_URL: &str = "ALO_SMTP_RSPAMD_URL";
/// Environment variable for the Rspamd call timeout in seconds.
pub const ENV_RSPAMD_TIMEOUT_SECS: &str = "ALO_SMTP_RSPAMD_TIMEOUT_SECS";
/// Environment variable naming the clamd address (`host:port`); unset
/// disables malware scanning. When set, a scanner outage fails closed
/// (451) exactly like Rspamd.
pub const ENV_CLAMAV_ADDR: &str = "ALO_SMTP_CLAMAV_ADDR";
/// Environment variable for the clamd scan timeout in seconds
/// (default 20 — large multi-attachment messages take a moment).
pub const ENV_CLAMAV_TIMEOUT_SECS: &str = "ALO_SMTP_CLAMAV_TIMEOUT_SECS";
/// Environment variable naming the campaign return path — the address
/// campaign mail's bounces come back to (e.g. `bounces@news.alomails.com`).
/// When set, the MX accepts it for delivery and routes it to the bounce
/// intake (RFC 3464 parsing → hard bounces suppress, ADR 0044 §4). Its
/// domain must be in [`ENV_LOCAL_DOMAINS`] and local delivery must be
/// configured, or the setting could not do anything and startup refuses it.
/// Unset disables the intake.
pub const ENV_CAMPAIGN_RETURN_PATH: &str = "ALO_SMTP_CAMPAIGN_RETURN_PATH";
/// Environment variable for the MTA-STS policy listener address; unset
/// disables serving the policy.
pub const ENV_MTA_STS_ADDR: &str = "ALO_SMTP_MTA_STS_ADDR";
/// Environment variable for the MTA-STS mode (`enforce`/`testing`/`none`).
pub const ENV_MTA_STS_MODE: &str = "ALO_SMTP_MTA_STS_MODE";
/// Environment variable for the MTA-STS MX patterns (comma-separated;
/// defaults to the server hostname).
pub const ENV_MTA_STS_MX: &str = "ALO_SMTP_MTA_STS_MX";
/// Environment variable for the MTA-STS `max_age` in seconds.
pub const ENV_MTA_STS_MAX_AGE: &str = "ALO_SMTP_MTA_STS_MAX_AGE";
/// Environment variable for an explicit MTA-STS policy id (derived from
/// the policy content when unset).
pub const ENV_MTA_STS_ID: &str = "ALO_SMTP_MTA_STS_ID";

const DEFAULT_ADDR: &str = "0.0.0.0:2525";
const DEFAULT_HOSTNAME: &str = "alo.test";
const DEFAULT_SPOOL_DIR: &str = "./spool";
const DEFAULT_BLOB_DIR: &str = "./blobs";
/// 25 MiB default, in line with common provider limits.
const DEFAULT_MAX_MESSAGE_SIZE: usize = 25 * 1024 * 1024;
/// RFC 5321 §4.5.3.1.8: servers MUST accept at least 100 recipients.
const DEFAULT_MAX_RCPT: usize = 100;
const MIN_MAX_RCPT: usize = 100;
/// RFC 5321 §4.5.3.1.7: servers MUST accept messages of at least
/// 64K octets — a lower configured cap ships a non-compliant server.
const MIN_MESSAGE_SIZE: usize = 64 * 1024;
const DEFAULT_MAX_CONNECTIONS: usize = 256;
/// Default per-IP concurrent-connection cap: generous for legitimate
/// shared MTAs, still far below the global cap so one host cannot
/// monopolise it.
const DEFAULT_MAX_CONNECTIONS_PER_IP: usize = 20;
const DEFAULT_RETRY_BASE_SECS: u64 = 60;
const DEFAULT_RETRY_CAP_SECS: u64 = 3600;
const DEFAULT_MAX_ATTEMPTS: u32 = 8;
const DEFAULT_QUEUE_INTERVAL_SECS: u64 = 30;
/// Default Rspamd call timeout (a local scanner should answer quickly;
/// on timeout the message is fail-closed deferred).
const DEFAULT_RSPAMD_TIMEOUT_SECS: u64 = 10;
/// Default clamd scan timeout — signature matching over a large
/// multi-attachment message takes longer than an Rspamd consult.
const DEFAULT_CLAMAV_TIMEOUT_SECS: u64 = 20;
/// Default MTA-STS `max_age`: one week (RFC 8461 recommends a long TTL
/// once a policy is stable).
const DEFAULT_MTA_STS_MAX_AGE: u32 = 604_800;

// Compile-time guarantees that the defaults never drift below the
// RFC 5321 floors (§4.5.3.1.8 recipients, §4.5.3.1.7 message size).
const _: () = assert!(DEFAULT_MAX_RCPT >= MIN_MAX_RCPT);
const _: () = assert!(DEFAULT_MAX_MESSAGE_SIZE >= MIN_MESSAGE_SIZE);
const _: () = assert!(DEFAULT_MAX_CONNECTIONS >= 1);

/// Validated service configuration.
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    /// Address the listener binds to.
    pub bind_addr: SocketAddr,
    /// Hostname announced in the 220 greeting and EHLO reply
    /// (RFC 5321 §4.1.1.1: the fully-qualified domain of the server).
    pub hostname: String,
    /// Root of the durable message spool.
    pub spool_dir: PathBuf,
    /// Root of the durable on-disk blob store for local delivery (message
    /// bytes). Only used when `database_url` is set.
    pub blob_dir: PathBuf,
    /// Maximum accepted message size in octets, enforced during read.
    pub max_message_size: usize,
    /// Maximum recipients per transaction (≥ 100 per §4.5.3.1.8).
    pub max_rcpt: usize,
    /// Concurrent-connection cap; excess connections are greeted with
    /// 421 and closed so one host cannot pin unlimited tasks.
    pub max_connections: usize,
    /// Per-source-IP concurrent-connection cap (`0` disables). Bounds a
    /// single host's share of `max_connections`; IPv6 counts by /64.
    pub max_connections_per_ip: usize,
    /// Outbound delivery settings; `None` means receive-only.
    pub outbound: Option<OutboundConfig>,
    /// Submission (STARTTLS, port 587) listener; `None` disables it.
    pub submission_addr: Option<SocketAddr>,
    /// Implicit-TLS submission (port 465) listener; `None` disables it.
    pub implicit_tls_addr: Option<SocketAddr>,
    /// Trusted internal submission listener (no auth, docker-network only);
    /// `None` disables it. Must never be published to the internet.
    pub internal_submission_addr: Option<SocketAddr>,
    /// TLS certificate + key PEM paths. `None` generates a self-signed
    /// certificate at startup (development only).
    pub tls: Option<TlsPaths>,
    /// Hosted domains (lowercased) for the MX anti-open-relay guard.
    /// Empty accepts all recipients (development); a non-empty list is
    /// required before outbound delivery may be enabled.
    pub local_domains: Vec<String>,
    /// Whether a self-signed certificate may be generated when no PEM
    /// is configured. Must be set explicitly so a production server
    /// never silently presents an untrusted cert (opportunistic-TLS
    /// MITM exposure).
    pub allow_self_signed: bool,
    /// DKIM signing for submitted mail; `None` disables signing.
    pub dkim: Option<DkimSigning>,
    /// Rspamd spam-scoring endpoint; `None` disables scanning (mail
    /// flows unscanned). When set, a scanner outage fails closed.
    pub rspamd: Option<RspamdSettings>,
    /// ClamAV malware scanning; `None` disables (mail flows unscanned).
    /// When set, a scanner outage fails closed.
    pub clamav: Option<ClamavSettings>,
    /// MTA-STS policy endpoint; `None` disables serving the policy.
    pub mta_sts: Option<MtaStsSettings>,
    /// PostgreSQL URL for the message store. When set (and the MX has a
    /// non-empty `local_domains` list), inbound mail for a hosted domain is
    /// delivered into the store (with Sieve at the boundary) instead of the
    /// spool. `None` keeps the receive-only spool behaviour.
    pub database_url: Option<String>,
    /// The campaign return path ([`ENV_CAMPAIGN_RETURN_PATH`]), lowercased;
    /// `None` disables the bounce intake.
    pub campaign_return_path: Option<String>,
    /// ARC sealing (RFC 8617) of Sieve-redirect forwards. On by
    /// default; [`ENV_ARC_SEALING`]`=off` is the operational off-switch.
    pub arc_sealing: bool,
    /// DMARC aggregate-report delivery (RFC 7489 §7.2). On by default;
    /// [`ENV_DMARC_REPORTS`]`=off` is the operational off-switch.
    pub dmarc_reports: bool,
    /// Report-window override in seconds ([`ENV_DMARC_REPORT_MIN_AGE_SECS`]);
    /// `None` keeps the standard daily cadence.
    pub dmarc_report_min_age: Option<std::time::Duration>,
    /// Reporter sweep interval ([`ENV_DMARC_REPORT_TICK_SECS`], default
    /// hourly).
    pub dmarc_report_tick: std::time::Duration,
}

/// Rspamd integration settings (M4b).
#[derive(Debug, Clone)]
pub struct RspamdSettings {
    /// Controller URL (`http://host:port`), kept for logging.
    pub url: String,
    /// The validated client (built once at config load).
    pub client: std::sync::Arc<crate::rspamd::RspamdClient>,
}

/// ClamAV integration settings: the validated client, built at config
/// load so a typo fails at startup.
#[derive(Debug, Clone)]
pub struct ClamavSettings {
    /// clamd `host:port`, kept for logging.
    pub addr: String,
    /// The validated client.
    pub client: std::sync::Arc<crate::clamav::ClamavClient>,
}

/// MTA-STS serving settings (M4b): where to serve the policy and the
/// validated policy itself.
#[derive(Debug, Clone)]
pub struct MtaStsSettings {
    /// Listener address for the (plaintext, proxy-fronted) policy HTTP
    /// endpoint.
    pub addr: SocketAddr,
    /// The validated, pre-rendered policy.
    pub policy: alo_auth_mail::mta_sts::MtaStsPolicy,
}

/// DKIM signing configuration (M4). The key path is always explicit —
/// never defaulted into the repo tree — and permission-checked at load.
#[derive(Debug, Clone)]
pub struct DkimSigning {
    /// Signing domain (`d=`).
    pub domain: String,
    /// Selector (`s=`), addressing the key for rotation.
    pub selector: String,
    /// Path to the PKCS#8 PEM private key.
    pub key_path: PathBuf,
    /// `true` for Ed25519 (RFC 8463), `false` for RSA.
    pub ed25519: bool,
    /// A second signing key for the same domain (RFC 8463 dual-signing,
    /// M4.5): every submitted message then carries one signature per key,
    /// so receivers that cannot read Ed25519 still verify the RSA one.
    /// `None` signs once — byte-identical to before the pair existed.
    pub second: Option<DkimSecondKey>,
}

/// The second DKIM signing key of a dual-signing deployment.
///
/// Its algorithm is **read from the key file at startup, never declared**:
/// an `a=` tag the key cannot produce yields a signature that looks fine
/// here and fails at every receiver, surfacing as lost delivery weeks
/// later rather than as an error now. Startup refuses a pair whose keys
/// sign the same algorithm — the second signature exists to cover the
/// other family.
#[derive(Debug, Clone)]
pub struct DkimSecondKey {
    /// Selector (`s=`) the second key's DNS record is published under.
    pub selector: String,
    /// Path to the PKCS#8 PEM private key.
    pub key_path: PathBuf,
    /// `true` when the key file holds an Ed25519 key — detected, not
    /// configured.
    pub ed25519: bool,
    /// The TXT record value the selector must publish (public material
    /// only), derived from the key file so the operator can diff it
    /// against DNS. Startup logs it beside the record name.
    pub dns_record: String,
}

/// Paths to a TLS certificate chain and its private key (PEM).
#[derive(Debug, Clone)]
pub struct TlsPaths {
    /// Certificate chain PEM.
    pub cert: PathBuf,
    /// Private key PEM.
    pub key: PathBuf,
}

/// Outbound delivery configuration.
///
/// Relay safety: outbound is off unless [`ENV_OUTBOUND_ENABLED`] is
/// explicitly true, because M1 accepts any recipient and enabling
/// delivery without the AUTH gate (M3) would make an exposed instance
/// an open relay. A smarthost is the supported self-hosted route.
#[derive(Debug, Clone)]
pub struct OutboundConfig {
    /// Smarthost to relay all mail through; `None` means MX delivery.
    pub smarthost: Option<SocketAddr>,
    /// First-retry base delay.
    pub retry_base: std::time::Duration,
    /// Retry delay cap.
    pub retry_cap: std::time::Duration,
    /// Attempts before a transient failure bounces.
    pub max_attempts: u32,
    /// Queue polling interval.
    pub queue_interval: std::time::Duration,
    /// DANE (RFC 7672): validate TLSA and enforce verified TLS where a
    /// secure record set exists. [`ENV_DANE`]`=off` disables.
    pub dane: bool,
    /// Outbound send rate per destination domain (messages/minute; `0`
    /// disables). [`ENV_OUTBOUND_RATE_PER_MIN`].
    pub rate_per_min: u32,
    /// Outbound rate burst depth. [`ENV_OUTBOUND_RATE_BURST`].
    pub rate_burst: u32,
    /// Per-sending-domain source addresses. [`ENV_EGRESS_IPS`].
    pub egress: EgressMap,
}

impl SmtpConfig {
    /// Builds the configuration from environment variables, falling
    /// back to development defaults.
    ///
    /// # Errors
    /// Returns [`SmtpError::Config`] when a provided value cannot be
    /// used, with a message naming the variable and the expected form.
    pub fn from_env() -> Result<Self, SmtpError> {
        let addr_raw = std::env::var(ENV_ADDR).unwrap_or_else(|_| DEFAULT_ADDR.to_owned());
        let bind_addr: SocketAddr = addr_raw.parse().map_err(|_| SmtpError::Config {
            message: format!(
                "{ENV_ADDR}={addr_raw} is not a socket address; expected e.g. 0.0.0.0:2525"
            ),
        })?;

        let hostname = std::env::var(ENV_HOSTNAME).unwrap_or_else(|_| DEFAULT_HOSTNAME.to_owned());
        if hostname.is_empty() || hostname.contains(char::is_whitespace) {
            return Err(SmtpError::Config {
                message: format!(
                    "{ENV_HOSTNAME}={hostname:?} must be a non-empty hostname without whitespace"
                ),
            });
        }

        let spool_dir = PathBuf::from(
            std::env::var(ENV_SPOOL_DIR).unwrap_or_else(|_| DEFAULT_SPOOL_DIR.to_owned()),
        );
        let blob_dir = PathBuf::from(
            std::env::var(ENV_BLOB_DIR).unwrap_or_else(|_| DEFAULT_BLOB_DIR.to_owned()),
        );

        let max_message_size = env_usize(ENV_MAX_MESSAGE_SIZE, DEFAULT_MAX_MESSAGE_SIZE)?;
        if max_message_size < MIN_MESSAGE_SIZE {
            return Err(SmtpError::Config {
                message: format!(
                    "{ENV_MAX_MESSAGE_SIZE} must be at least {MIN_MESSAGE_SIZE} octets"
                ),
            });
        }

        let max_rcpt = env_usize(ENV_MAX_RCPT, DEFAULT_MAX_RCPT)?;
        if max_rcpt < MIN_MAX_RCPT {
            // RFC 5321 §4.5.3.1.8 sets the floor; configuring below it
            // would ship a non-compliant server.
            return Err(SmtpError::Config {
                message: format!("{ENV_MAX_RCPT} must be at least {MIN_MAX_RCPT} (RFC 5321)"),
            });
        }

        let max_connections = env_usize(ENV_MAX_CONNECTIONS, DEFAULT_MAX_CONNECTIONS)?;
        if max_connections == 0 {
            return Err(SmtpError::Config {
                message: format!("{ENV_MAX_CONNECTIONS} must be at least 1"),
            });
        }
        let max_connections_per_ip =
            env_usize(ENV_MAX_CONNECTIONS_PER_IP, DEFAULT_MAX_CONNECTIONS_PER_IP)?;

        let outbound = Self::outbound_from_env()?;

        let submission_addr = env_addr(ENV_SUBMISSION_ADDR)?;
        let implicit_tls_addr = env_addr(ENV_IMPLICIT_TLS_ADDR)?;
        let internal_submission_addr = env_addr(ENV_INTERNAL_SUBMISSION_ADDR)?;

        let tls = match (std::env::var(ENV_TLS_CERT), std::env::var(ENV_TLS_KEY)) {
            (Ok(cert), Ok(key)) if !cert.is_empty() && !key.is_empty() => Some(TlsPaths {
                cert: PathBuf::from(cert),
                key: PathBuf::from(key),
            }),
            (Ok(one), Err(_)) | (Err(_), Ok(one)) if !one.is_empty() => {
                return Err(SmtpError::Config {
                    message: format!("{ENV_TLS_CERT} and {ENV_TLS_KEY} must be set together"),
                });
            }
            _ => None,
        };

        // Implicit-TLS (465) cannot run without a usable certificate;
        // a self-signed one is generated when none is configured, so
        // this never blocks dev — but warn nobody set a real cert.
        if implicit_tls_addr.is_some() && tls.is_none() {
            tracing::warn!(
                "implicit-TLS listener configured without a certificate; a self-signed one will be generated (development only)"
            );
        }

        let local_domains: Vec<String> = std::env::var(ENV_LOCAL_DOMAINS)
            .unwrap_or_default()
            .split(',')
            .map(|d| d.trim().to_ascii_lowercase())
            .filter(|d| !d.is_empty())
            .collect();

        // Anti-open-relay (security audit): outbound delivery must not
        // be enabled while the MX accepts recipients for any domain, or
        // an exposed instance relays to arbitrary externals. Enforced
        // in code, not left to the outbound-off default.
        if outbound.is_some() && local_domains.is_empty() {
            return Err(SmtpError::Config {
                message: format!(
                    "{ENV_OUTBOUND_ENABLED}=true requires {ENV_LOCAL_DOMAINS} to be set \
                     (the MX would otherwise be an open relay)"
                ),
            });
        }

        let allow_self_signed = env_bool(ENV_ALLOW_SELF_SIGNED)?;
        // A production server (real cert absent) must not silently
        // present a self-signed cert: require the explicit opt-in.
        if tls.is_none() && !allow_self_signed {
            return Err(SmtpError::Config {
                message: format!(
                    "no TLS certificate configured: set {ENV_TLS_CERT}/{ENV_TLS_KEY}, \
                     or {ENV_ALLOW_SELF_SIGNED}=true for development"
                ),
            });
        }

        let dkim = Self::dkim_from_env()?;
        let rspamd = Self::rspamd_from_env()?;
        let clamav = Self::clamav_from_env()?;
        let mta_sts = Self::mta_sts_from_env(&hostname)?;
        let database_url = std::env::var("DATABASE_URL").ok().filter(|s| !s.is_empty());
        // Local delivery into the store needs a hosted-domains list, so a
        // recipient can be classified local before it is resolved.
        if database_url.is_some() && local_domains.is_empty() {
            return Err(SmtpError::Config {
                message: format!(
                    "DATABASE_URL is set for local delivery but {ENV_LOCAL_DOMAINS} is empty \
                     (no way to tell which recipients are local)"
                ),
            });
        }

        let campaign_return_path = campaign_return_path_from_env(&local_domains, &database_url)?;

        // ARC sealing defaults ON (an unsealed forward fails downstream
        // DMARC); the env var is the explicit off-switch.
        let arc_sealing = std::env::var(ENV_ARC_SEALING).is_err() || env_bool(ENV_ARC_SEALING)?;
        // DMARC aggregate reporting likewise defaults ON.
        let dmarc_reports =
            std::env::var(ENV_DMARC_REPORTS).is_err() || env_bool(ENV_DMARC_REPORTS)?;
        let dmarc_report_min_age = match std::env::var(ENV_DMARC_REPORT_MIN_AGE_SECS) {
            Err(_) => None,
            Ok(raw) => Some(std::time::Duration::from_secs(raw.parse().map_err(
                |_| SmtpError::Config {
                    message: format!("{ENV_DMARC_REPORT_MIN_AGE_SECS}={raw} is not a number"),
                },
            )?)),
        };
        let dmarc_report_tick =
            std::time::Duration::from_secs(env_u64(ENV_DMARC_REPORT_TICK_SECS, 3600)?.max(60));

        Ok(Self {
            bind_addr,
            hostname,
            spool_dir,
            blob_dir,
            max_message_size,
            max_rcpt,
            max_connections,
            max_connections_per_ip,
            outbound,
            submission_addr,
            implicit_tls_addr,
            internal_submission_addr,
            tls,
            local_domains,
            allow_self_signed,
            dkim,
            rspamd,
            clamav,
            mta_sts,
            database_url,
            campaign_return_path,
            arc_sealing,
            dmarc_reports,
            dmarc_report_min_age,
            dmarc_report_tick,
        })
    }

    /// Reads the Rspamd endpoint; validates the URL now so a typo fails
    /// at startup, naming the variable.
    fn rspamd_from_env() -> Result<Option<RspamdSettings>, SmtpError> {
        let url = match std::env::var(ENV_RSPAMD_URL) {
            Ok(url) if !url.is_empty() => url,
            _ => return Ok(None),
        };
        let timeout = Duration::from_secs(
            env_u64(ENV_RSPAMD_TIMEOUT_SECS, DEFAULT_RSPAMD_TIMEOUT_SECS)?.max(1),
        );
        let client = crate::rspamd::RspamdClient::from_url(&url, timeout).map_err(|message| {
            SmtpError::Config {
                message: format!("{ENV_RSPAMD_URL}: {message}"),
            }
        })?;
        Ok(Some(RspamdSettings {
            url,
            client: std::sync::Arc::new(client),
        }))
    }

    /// Reads the clamd endpoint; validates the address shape now so a
    /// typo fails at startup, naming the variable.
    fn clamav_from_env() -> Result<Option<ClamavSettings>, SmtpError> {
        let addr = match std::env::var(ENV_CLAMAV_ADDR) {
            Ok(addr) if !addr.is_empty() => addr,
            _ => return Ok(None),
        };
        let timeout = Duration::from_secs(
            env_u64(ENV_CLAMAV_TIMEOUT_SECS, DEFAULT_CLAMAV_TIMEOUT_SECS)?.max(1),
        );
        let client = crate::clamav::ClamavClient::from_addr(&addr, timeout).map_err(|message| {
            SmtpError::Config {
                message: format!("{ENV_CLAMAV_ADDR}: {message}"),
            }
        })?;
        Ok(Some(ClamavSettings {
            addr,
            client: std::sync::Arc::new(client),
        }))
    }

    /// Reads and validates the MTA-STS policy; only served when an
    /// address is configured.
    fn mta_sts_from_env(hostname: &str) -> Result<Option<MtaStsSettings>, SmtpError> {
        use alo_auth_mail::mta_sts::{MtaStsPolicy, StsMode};
        let Some(addr) = env_addr(ENV_MTA_STS_ADDR)? else {
            return Ok(None);
        };
        let mode = match std::env::var(ENV_MTA_STS_MODE) {
            Ok(m) if !m.is_empty() => StsMode::parse(&m).ok_or_else(|| SmtpError::Config {
                message: format!("{ENV_MTA_STS_MODE}={m} must be enforce/testing/none"),
            })?,
            _ => StsMode::Enforce,
        };
        let mx: Vec<String> = std::env::var(ENV_MTA_STS_MX)
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        // Default to this server's hostname when no MX patterns are given.
        let mx = if mx.is_empty() {
            vec![hostname.to_owned()]
        } else {
            mx
        };
        let max_age = u32::try_from(env_u64(
            ENV_MTA_STS_MAX_AGE,
            u64::from(DEFAULT_MTA_STS_MAX_AGE),
        )?)
        .map_err(|_| SmtpError::Config {
            message: format!("{ENV_MTA_STS_MAX_AGE} is out of range"),
        })?;
        let id = std::env::var(ENV_MTA_STS_ID).ok().filter(|s| !s.is_empty());
        let policy =
            MtaStsPolicy::new(mode, mx, max_age, id).map_err(|error| SmtpError::Config {
                message: format!("MTA-STS policy invalid: {error}"),
            })?;
        Ok(Some(MtaStsSettings { addr, policy }))
    }

    /// Reads DKIM signing config; all three of domain/selector/key must
    /// be set together, or none (signing disabled). A second key pair
    /// (dual-signing, M4.5) is validated against its key file at startup.
    fn dkim_from_env() -> Result<Option<DkimSigning>, SmtpError> {
        let get = |name: &str| std::env::var(name).ok().filter(|s| !s.is_empty());
        let mut signing = assemble_dkim_signing(
            get(ENV_DKIM_DOMAIN),
            get(ENV_DKIM_SELECTOR),
            get(ENV_DKIM_KEY),
            get(ENV_DKIM_ALGORITHM),
            get(ENV_DKIM_SELECTOR2),
            get(ENV_DKIM_KEY2),
        )?;
        if let Some(signing) = &mut signing
            && let Some(second) = &mut signing.second
        {
            (second.ed25519, second.dns_record) =
                validate_second_dkim_key(&signing.key_path, signing.ed25519, &second.key_path)
                    .map_err(|message| SmtpError::Config { message })?;
        }
        Ok(signing)
    }

    fn outbound_from_env() -> Result<Option<OutboundConfig>, SmtpError> {
        if !env_bool(ENV_OUTBOUND_ENABLED)? {
            return Ok(None);
        }
        let smarthost = match std::env::var(ENV_SMARTHOST) {
            Err(_) => None,
            Ok(raw) if raw.is_empty() => None,
            Ok(raw) => Some(raw.parse().map_err(|_| SmtpError::Config {
                message: format!("{ENV_SMARTHOST}={raw} is not a host:port address"),
            })?),
        };
        let retry_base =
            Duration::from_secs(env_u64(ENV_RETRY_BASE_SECS, DEFAULT_RETRY_BASE_SECS)?.max(1));
        let retry_cap = Duration::from_secs(
            env_u64(ENV_RETRY_CAP_SECS, DEFAULT_RETRY_CAP_SECS)?.max(retry_base.as_secs()),
        );
        let max_attempts =
            u32::try_from(env_u64(ENV_MAX_ATTEMPTS, u64::from(DEFAULT_MAX_ATTEMPTS))?)
                .unwrap_or(DEFAULT_MAX_ATTEMPTS)
                .max(1);
        let queue_interval = Duration::from_secs(
            env_u64(ENV_QUEUE_INTERVAL_SECS, DEFAULT_QUEUE_INTERVAL_SECS)?.max(1),
        );
        // DANE defaults ON (a secure TLSA set means the destination
        // asked for verified TLS); the env var is the off-switch.
        let dane = std::env::var(ENV_DANE).is_err() || env_bool(ENV_DANE)?;
        // Outbound rate limiting is off unless configured (a
        // single-tenant server rarely needs it; a shared host does).
        let rate_per_min = env_u64(ENV_OUTBOUND_RATE_PER_MIN, 0)? as u32;
        let rate_burst = env_u64(ENV_OUTBOUND_RATE_BURST, u64::from(rate_per_min))? as u32;
        // A sending identity's own source address (ADR 0044 §1). Unparseable is
        // fatal on purpose: the fallback would be the transactional address,
        // which is the separation silently undone.
        let egress = EgressMap::parse(&std::env::var(ENV_EGRESS_IPS).unwrap_or_default()).map_err(
            |reason| SmtpError::Config {
                message: format!(
                    "{ENV_EGRESS_IPS}: {reason}; expected e.g. news.example.com=203.0.113.7"
                ),
            },
        )?;
        Ok(Some(OutboundConfig {
            smarthost,
            retry_base,
            retry_cap,
            max_attempts,
            queue_interval,
            dane,
            rate_per_min,
            rate_burst,
            egress,
        }))
    }
}

/// Reads and validates the campaign return path. Every rule here exists so a
/// misconfiguration fails at startup with a message naming the variable,
/// rather than as a bounce address that silently answers 550 in production:
/// the address must look like one, its domain must be hosted (or the RCPT
/// anti-relay guard refuses it before delivery is consulted), and local
/// delivery must be on (the intake writes into the store).
fn campaign_return_path_from_env(
    local_domains: &[String],
    database_url: &Option<String>,
) -> Result<Option<String>, SmtpError> {
    match std::env::var(ENV_CAMPAIGN_RETURN_PATH) {
        Ok(raw) if !raw.is_empty() => {
            validate_campaign_return_path(&raw, local_domains, database_url).map(Some)
        }
        _ => Ok(None),
    }
}

/// The validation half of [`campaign_return_path_from_env`], separated from
/// the env read so the rules are testable without process-global state.
fn validate_campaign_return_path(
    raw: &str,
    local_domains: &[String],
    database_url: &Option<String>,
) -> Result<String, SmtpError> {
    let address = raw.trim().to_ascii_lowercase();
    let Some((local, domain)) = address.split_once('@') else {
        return Err(SmtpError::Config {
            message: format!(
                "{ENV_CAMPAIGN_RETURN_PATH}={raw} is not an address; expected e.g. bounces@news.example.com"
            ),
        });
    };
    if local.is_empty() || domain.is_empty() || address.contains(char::is_whitespace) {
        return Err(SmtpError::Config {
            message: format!(
                "{ENV_CAMPAIGN_RETURN_PATH}={raw} is not an address; expected e.g. bounces@news.example.com"
            ),
        });
    }
    if !local_domains.iter().any(|d| d == domain) {
        return Err(SmtpError::Config {
            message: format!(
                "{ENV_CAMPAIGN_RETURN_PATH}: {domain} is not in {ENV_LOCAL_DOMAINS}, \
                 so the MX would refuse the bounce address it is meant to accept"
            ),
        });
    }
    if database_url.is_none() {
        return Err(SmtpError::Config {
            message: format!(
                "{ENV_CAMPAIGN_RETURN_PATH} needs local delivery (DATABASE_URL): \
                 the bounce intake writes into the store"
            ),
        });
    }
    Ok(address)
}

/// The pairing rules for DKIM signing config, separated from the env read so
/// they are testable without process-global state. Every refusal names the
/// variable: a half-configured pair must fail at startup, not sign half.
fn assemble_dkim_signing(
    domain: Option<String>,
    selector: Option<String>,
    key: Option<String>,
    algorithm: Option<String>,
    selector2: Option<String>,
    key2: Option<String>,
) -> Result<Option<DkimSigning>, SmtpError> {
    let second = match (selector2, key2) {
        (Some(selector), Some(key)) => Some(DkimSecondKey {
            selector,
            key_path: PathBuf::from(key),
            // Filled from the key file by the caller; never declared.
            ed25519: false,
            dns_record: String::new(),
        }),
        (None, None) => None,
        _ => {
            return Err(SmtpError::Config {
                message: format!("{ENV_DKIM_SELECTOR2} and {ENV_DKIM_KEY2} must be set together"),
            });
        }
    };
    match (domain, selector, key) {
        (Some(domain), Some(selector), Some(key)) => {
            if let Some(second) = &second
                && second.selector == selector
            {
                return Err(SmtpError::Config {
                    message: format!(
                        "{ENV_DKIM_SELECTOR2} must differ from {ENV_DKIM_SELECTOR}: two keys \
                         cannot share {selector}._domainkey.{domain}"
                    ),
                });
            }
            let ed25519 = algorithm
                .map(|a| a.eq_ignore_ascii_case("ed25519"))
                .unwrap_or(false);
            Ok(Some(DkimSigning {
                domain,
                selector,
                key_path: PathBuf::from(key),
                ed25519,
                second,
            }))
        }
        (None, None, None) if second.is_some() => Err(SmtpError::Config {
            message: format!(
                "a second DKIM key needs the first: set {ENV_DKIM_DOMAIN}, \
                 {ENV_DKIM_SELECTOR}, and {ENV_DKIM_KEY} as well"
            ),
        }),
        (None, None, None) => Ok(None),
        _ => Err(SmtpError::Config {
            message: format!(
                "{ENV_DKIM_DOMAIN}, {ENV_DKIM_SELECTOR}, and {ENV_DKIM_KEY} must be set together"
            ),
        }),
    }
}

/// The key-file rules for dual-signing (RFC 8463, M4.5), run at startup only
/// when a second key is configured — a single-key deployment never reads its
/// key before signing time, exactly as before.
///
/// Returns whether the **second** key is Ed25519 and the TXT record value its
/// selector must publish (public material only — derived from the key so the
/// operator can diff it against DNS). Refuses, naming the variable: an
/// unreadable or unrecognisable key file, a primary key that contradicts its
/// declared [`ENV_DKIM_ALGORITHM`] (a lying config would put an `a=` tag on
/// signatures the key cannot produce), and a pair signing the same algorithm
/// (the second signature exists to cover the other family). The error strings
/// carry file paths and algorithm names only — never key bytes
/// (`load_pkcs8_pem`'s own contract).
fn validate_second_dkim_key(
    primary_key: &Path,
    primary_declared_ed25519: bool,
    second_key: &Path,
) -> Result<(bool, String), String> {
    use alo_auth_mail::dkim::keystore::{
        KeyAlgorithm, algorithm_of_pkcs8, ed25519_key_from_pkcs8, load_pkcs8_pem, txt_record_for,
    };
    use alo_auth_mail::dkim::rsa_public;
    let read = |name: &str, path: &Path| -> Result<(KeyAlgorithm, Vec<u8>), String> {
        let der = load_pkcs8_pem(path)
            .map_err(|reason| format!("{name}={}: {reason}", path.display()))?;
        let unreadable = || {
            format!(
                "{name}={}: the key is neither an RSA nor an Ed25519 private key",
                path.display()
            )
        };
        let algorithm = algorithm_of_pkcs8(&der).ok_or_else(unreadable)?;
        // The public half, in the encoding its DNS record publishes.
        let public = match algorithm {
            KeyAlgorithm::RsaSha256 => rsa_public::spki_from_pkcs8(&der).ok_or_else(unreadable)?,
            KeyAlgorithm::Ed25519Sha256 => ed25519_key_from_pkcs8(&der).ok_or_else(unreadable)?.1,
        };
        Ok((algorithm, public))
    };
    let (primary, _) = read(ENV_DKIM_KEY, primary_key)?;
    let declared = if primary_declared_ed25519 {
        KeyAlgorithm::Ed25519Sha256
    } else {
        KeyAlgorithm::RsaSha256
    };
    if primary != declared {
        return Err(format!(
            "{ENV_DKIM_ALGORITHM} says {} but the key at {ENV_DKIM_KEY} is {}: \
             signatures would name an algorithm the key cannot produce",
            declared.tag(),
            primary.tag()
        ));
    }
    let (second, second_public) = read(ENV_DKIM_KEY2, second_key)?;
    if second == primary {
        return Err(format!(
            "both DKIM keys are {}: dual-signing (RFC 8463) needs one RSA and one \
             Ed25519 key, so each receiver can verify the family it understands",
            second.tag()
        ));
    }
    let ed25519 = second == KeyAlgorithm::Ed25519Sha256;
    let tag = if ed25519 { "ed25519" } else { "rsa" };
    let record = txt_record_for(tag, &second_public).unwrap_or_default(); // both tags render; unreachable in practice
    Ok((ed25519, record))
}

fn env_bool(name: &str) -> Result<bool, SmtpError> {
    match std::env::var(name) {
        Err(_) => Ok(false),
        Ok(raw) => match raw.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" | "" => Ok(false),
            other => Err(SmtpError::Config {
                message: format!("{name}={other} must be a boolean (true/false)"),
            }),
        },
    }
}

fn env_u64(name: &str, default: u64) -> Result<u64, SmtpError> {
    match std::env::var(name) {
        Err(_) => Ok(default),
        Ok(raw) => raw.parse().map_err(|_| SmtpError::Config {
            message: format!("{name}={raw} is not a number"),
        }),
    }
}

fn env_usize(name: &str, default: usize) -> Result<usize, SmtpError> {
    match std::env::var(name) {
        Err(_) => Ok(default),
        Ok(raw) => raw.parse().map_err(|_| SmtpError::Config {
            message: format!("{name}={raw} is not a number"),
        }),
    }
}

/// Parses an optional socket-address env var.
fn env_addr(name: &str) -> Result<Option<SocketAddr>, SmtpError> {
    match std::env::var(name) {
        Ok(raw) if !raw.is_empty() => raw.parse().map(Some).map_err(|_| SmtpError::Config {
            message: format!("{name}={raw} is not a host:port address"),
        }),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn defaults_are_valid_and_rfc_compliant() {
        // Only assert on parseability — env mutation would race other
        // tests, and the numeric floors are compile-time asserts above.
        let addr: SocketAddr = DEFAULT_ADDR.parse().unwrap();
        assert_eq!(addr.port(), 2525);
    }

    /// The campaign return path's startup rules (M4.4): every misconfiguration
    /// fails loudly at startup, naming the variable — never as a bounce
    /// address that silently answers 550 in production. Tested on the pure
    /// validation half; env mutation would race other tests.
    #[test]
    fn campaign_return_path_rules_refuse_what_production_would_regret() {
        let hosted = vec!["news.example.com".to_owned()];
        let db = Some("postgres://x".to_owned());

        // The good shape: trimmed, lowercased, accepted.
        assert_eq!(
            validate_campaign_return_path(" Bounces@News.Example.COM ", &hosted, &db).unwrap(),
            "bounces@news.example.com"
        );
        // Not an address at all.
        for bad in [
            "bounces",
            "@news.example.com",
            "bounces@",
            "a b@news.example.com",
        ] {
            assert!(
                validate_campaign_return_path(bad, &hosted, &db).is_err(),
                "{bad} should be refused"
            );
        }
        // A domain the MX does not host: the RCPT anti-relay guard would
        // refuse the address before delivery ever saw it.
        assert!(validate_campaign_return_path("bounces@elsewhere.test", &hosted, &db).is_err());
        // No local delivery: the intake would have nowhere to write.
        assert!(validate_campaign_return_path("bounces@news.example.com", &hosted, &None).is_err());
    }

    /// The DKIM pairing rules (M4.5): a half-configured pair fails at startup
    /// naming the variable, never signs half. Tested on the pure assembly
    /// half — env mutation would race other tests.
    #[test]
    fn dkim_pairing_rules_refuse_half_configured_signing() {
        let some = |s: &str| Some(s.to_owned());
        // No signing at all is a valid state.
        assert!(
            assemble_dkim_signing(None, None, None, None, None, None)
                .unwrap()
                .is_none()
        );
        // The primary triple comes together or not at all.
        assert!(assemble_dkim_signing(some("d.test"), None, None, None, None, None).is_err());
        // The second pair comes together or not at all.
        for (selector2, key2) in [(some("s2"), None), (None, some("/k2"))] {
            assert!(
                assemble_dkim_signing(
                    some("d.test"),
                    some("s1"),
                    some("/k1"),
                    None,
                    selector2,
                    key2
                )
                .is_err(),
                "a half-set second pair must be refused"
            );
        }
        // A second key without a first is a refusal, not a promotion.
        assert!(
            assemble_dkim_signing(None, None, None, None, some("s2"), some("/k2")).is_err(),
            "a second key needs the first"
        );
        // Two keys cannot share a selector: one DNS name cannot publish both.
        assert!(
            assemble_dkim_signing(
                some("d.test"),
                some("s1"),
                some("/k1"),
                None,
                some("s1"),
                some("/k2")
            )
            .is_err()
        );
        // The good dual shape parses; the second's algorithm is filled from
        // the key file later, never from these arguments.
        let signing = assemble_dkim_signing(
            some("d.test"),
            some("s1"),
            some("/k1"),
            some("rsa"),
            some("s2"),
            some("/k2"),
        )
        .unwrap()
        .expect("signing configured");
        assert!(!signing.ed25519);
        assert_eq!(signing.second.expect("a second key").selector, "s2");
        // And a single-key deployment still parses exactly as before.
        let single =
            assemble_dkim_signing(some("d.test"), some("s1"), some("/k1"), None, None, None)
                .unwrap()
                .expect("signing configured");
        assert!(single.second.is_none());
    }

    // The committed RSA-2048 test fixture (see
    // `alo_auth_mail::dkim::keystore::fixture_keys`) — RSA is never
    // generated in-process (ADR 0008).
    use alo_auth_mail::dkim::keystore::fixture_keys::RSA_PKCS8_B64;

    /// Writes a PKCS#8 PEM the way an operator leaves one: owner-readable
    /// only, so the keystore's permission check passes on Unix (a no-op on
    /// Windows, where the check does not apply).
    fn write_key_pem(path: &std::path::Path, der: &[u8]) {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(der);
        let pem = format!("-----BEGIN PRIVATE KEY-----\n{b64}\n-----END PRIVATE KEY-----\n");
        std::fs::write(path, pem).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }

    /// The dual-signing key-file rules (M4.5): the algorithms are read from
    /// the keys themselves, a config that contradicts its key is refused, and
    /// a pair that covers only one algorithm family is refused.
    #[test]
    fn dual_dkim_key_rules_read_the_files_not_the_flags() {
        use alo_auth_mail::dkim::keystore::{ed25519_signing_key_from_seed, generate_ed25519_key};
        use base64::Engine;
        let dir = tempfile::tempdir().unwrap();

        let rsa_path = dir.path().join("rsa.pem");
        let rsa_der = base64::engine::general_purpose::STANDARD
            .decode(RSA_PKCS8_B64)
            .unwrap();
        write_key_pem(&rsa_path, &rsa_der);

        let ed = generate_ed25519_key().expect("keygen");
        let ed_der = ed25519_signing_key_from_seed(ed.seed.as_ref())
            .expect("from seed")
            .pkcs8_der;
        let ed_path = dir.path().join("ed25519.pem");
        write_key_pem(&ed_path, &ed_der);

        // The production shape: RSA primary (declared rsa), Ed25519 second —
        // and the record to publish is derived from the second key itself.
        let (ed25519, record) = validate_second_dkim_key(&rsa_path, false, &ed_path)
            .expect("the production pair validates");
        assert!(ed25519);
        let expected_p = base64::engine::general_purpose::STANDARD.encode(ed.public_raw);
        assert_eq!(record, format!("v=DKIM1; k=ed25519; p={expected_p}"));
        // The mirrored shape works too — the RSA key is then the second.
        let (ed25519, record) = validate_second_dkim_key(&ed_path, true, &rsa_path)
            .expect("the mirrored pair validates");
        assert!(!ed25519);
        assert!(record.starts_with("v=DKIM1; k=rsa; p="), "{record}");
        // Two keys of one family cover nobody extra: refused.
        let error = validate_second_dkim_key(&ed_path, true, &ed_path)
            .expect_err("a same-algorithm pair must be refused");
        assert!(error.contains("both"), "{error}");
        // A primary flag the key contradicts would put a lying `a=` tag on
        // every signature: refused, naming the flag.
        let error = validate_second_dkim_key(&rsa_path, true, &ed_path)
            .expect_err("a declared algorithm the key cannot produce must be refused");
        assert!(error.contains(ENV_DKIM_ALGORITHM), "{error}");
        // A missing or unreadable second key file is named by its variable.
        let error = validate_second_dkim_key(&rsa_path, false, &dir.path().join("absent.pem"))
            .expect_err("a missing key file must be refused");
        assert!(error.contains(ENV_DKIM_KEY2), "{error}");
        // A file that is not a key at all is refused — and the refusal never
        // echoes file contents.
        let garbage = dir.path().join("garbage.pem");
        std::fs::write(&garbage, b"-----BEGIN CERTIFICATE-----\nnope\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&garbage, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let error =
            validate_second_dkim_key(&rsa_path, false, &garbage).expect_err("garbage is not a key");
        assert!(!error.contains("nope"), "{error}");
    }
}
