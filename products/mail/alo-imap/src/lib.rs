//! # alo-imap — IMAP4rev2/rev1 + POP3 compatibility shims
//!
//! A thin protocol translator over [`alo_store::AccountStore`]: JMAP is
//! alo's native protocol (ADR 0001), and these shims let the installed
//! base of IMAP/POP3 clients reach a alo mailbox unchanged. Every
//! mailbox, message, flag, and UID served is account-scoped data access,
//! so tenant/account isolation is inherited from the store, not
//! re-implemented here. See `docs/design/imap-pop3-shims.md`.

use std::sync::Arc;

use alo_identity::Identity;
use alo_store::Store;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

pub mod config;
pub mod error;
pub mod fetch;
pub mod flags;
pub mod mailbox;
pub mod parser;
pub mod pop3;
pub mod search;
pub mod session;
pub mod stream;
pub mod tls;

pub use config::Config;
pub use error::{ImapError, Result};
pub use session::Session;
pub use stream::ImapStream;

/// Runs the configured IMAP/POP3 listeners until they error. Listeners are
/// off unless their address is set in `cfg`.
///
/// # Errors
/// [`ImapError`] if a TLS acceptor cannot be built or a listener cannot
/// bind.
pub async fn serve(cfg: Config, store: Arc<Store>, identity: Identity) -> Result<()> {
    let cfg = Arc::new(cfg);
    let needs_tls = cfg.imaps_addr.is_some() || cfg.pop3s_addr.is_some() || cfg.imap_addr.is_some();
    let acceptor = if needs_tls {
        Some(tls::build_acceptor(
            cfg.tls_cert.as_deref(),
            cfg.tls_key.as_deref(),
            &cfg.hostname,
            cfg.allow_self_signed,
        )?)
    } else {
        None
    };

    let mut tasks = Vec::new();
    if let Some(addr) = cfg.imaps_addr {
        let l = TcpListener::bind(addr).await?;
        tracing::info!(%addr, "IMAP implicit-TLS listener up");
        tasks.push(tokio::spawn(accept_imap(
            l,
            cfg.clone(),
            store.clone(),
            identity.clone(),
            acceptor.clone(),
            true,
        )));
    }
    if let Some(addr) = cfg.imap_addr {
        let l = TcpListener::bind(addr).await?;
        tracing::info!(%addr, "IMAP STARTTLS listener up");
        tasks.push(tokio::spawn(accept_imap(
            l,
            cfg.clone(),
            store.clone(),
            identity.clone(),
            acceptor.clone(),
            false,
        )));
    }
    if let Some(addr) = cfg.pop3s_addr {
        let l = TcpListener::bind(addr).await?;
        tracing::info!(%addr, "POP3 implicit-TLS listener up");
        let acceptor = acceptor.clone();
        let (cfg, store, identity) = (cfg.clone(), store.clone(), identity.clone());
        tasks.push(tokio::spawn(async move {
            pop3::accept(l, cfg, store, identity, acceptor).await
        }));
    }
    for t in tasks {
        let _ = t.await;
    }
    Ok(())
}

/// Accept loop for an IMAP listener. `implicit_tls` wraps every connection
/// in TLS immediately (993); otherwise it starts cleartext and offers
/// STARTTLS (143).
async fn accept_imap(
    listener: TcpListener,
    cfg: Arc<Config>,
    store: Arc<Store>,
    identity: Identity,
    acceptor: Option<TlsAcceptor>,
    implicit_tls: bool,
) {
    loop {
        let (tcp, peer) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "IMAP accept failed");
                continue;
            }
        };
        let cfg = cfg.clone();
        let store = store.clone();
        let identity = identity.clone();
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            let stream = if implicit_tls {
                match &acceptor {
                    Some(a) => match a.accept(tcp).await {
                        Ok(t) => ImapStream::Tls(Box::new(t)),
                        Err(e) => {
                            tracing::debug!(%peer, error = %e, "TLS handshake failed");
                            return;
                        }
                    },
                    None => return,
                }
            } else {
                ImapStream::Plain(tcp)
            };
            // STARTTLS is only offered on the cleartext listener.
            let starttls = if implicit_tls { None } else { acceptor };
            let session = Session::new(stream, cfg, store, identity, starttls);
            if let Err(e) = session.run().await {
                tracing::debug!(%peer, error = %e, "IMAP session ended");
            }
        });
    }
}
