//! Session Contexts ([MS-OXCMAPIHTTP] §3.1.1) — the server-side state a client
//! establishes with `Connect` and names on every later request by cookie.
//!
//! Two cookies exist per context: the **session context cookie**, which names
//! it, and the **request sequence cookie**, which the client returns so the
//! server can order requests within it. Both are opaque to the client.
//!
//! **Opaque means unguessable, not merely unlabelled.** The cookie is the
//! bearer of a mailbox session, so it is drawn from the OS random source and
//! never derived from a mailbox id, a user name, or a counter — a predictable
//! context cookie is a session-hijacking primitive, and every context here is
//! bound to the tenant and user that created it so a stolen one still cannot
//! reach across a tenant boundary.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use alo_store::{TenantId, UserId};
use ring::rand::{SecureRandom, SystemRandom};
use time::{Duration, OffsetDateTime};

/// The cookie naming a Session Context.
pub const SESSION_COOKIE: &str = "MapiContext";
/// The cookie carrying the client's request sequence within a context.
pub const SEQUENCE_COOKIE: &str = "MapiSequence";

/// How long an idle context survives. The client is told this in
/// `X-ExpirationInfo` and refreshes by using the context.
pub const CONTEXT_LIFETIME: Duration = Duration::minutes(15);

/// The maximum number of live contexts. A bound rather than an unbounded map:
/// `Connect` is reachable by anyone who can authenticate, and an unbounded
/// session table is a memory-exhaustion lever.
pub const MAX_CONTEXTS: usize = 10_000;

/// One client's established session.
#[derive(Debug, Clone)]
pub struct SessionContext {
    /// The tenant this context belongs to. Every later request is answered
    /// only within it.
    pub tenant: TenantId,
    /// The user who authenticated when the context was created.
    pub user: UserId,
    /// The DN the client connected as, echoed in diagnostics.
    ///
    /// A claim the client made at `Connect`, never an authority: what the
    /// caller may open is decided from [`Self::login`], which is who they
    /// actually proved to be.
    pub user_dn: String,
    /// The address the caller authenticated as. **This is the identity.**
    pub login: String,
    /// When this context stops being valid unless used again.
    pub expires_at: OffsetDateTime,
    /// The server objects this session holds — logons, and later the folders
    /// and messages opened through them.
    ///
    /// Shared behind an `Arc` on purpose: [`SessionStore::touch`] hands back a
    /// clone of the context on every request, and a table that cloned with it
    /// would give each request its own empty one — every handle a client held
    /// would evaporate between calls.
    pub objects: Arc<Mutex<crate::dispatch::ObjectTable>>,
}

/// The live Session Contexts, keyed by their cookie value.
#[derive(Debug, Default)]
pub struct SessionStore {
    contexts: Mutex<HashMap<String, SessionContext>>,
}

/// A freshly minted, unguessable cookie value, or `None` if the system CSPRNG
/// is unavailable.
///
/// 32 bytes of `ring`'s system randomness rendered as hex. Not a UUID and not a
/// counter: this value is the only thing standing between a stranger and a
/// mailbox session, so its strength is the whole point.
///
/// A CSPRNG failure returns `None` and the caller refuses the connection. The
/// alternative — falling back to something weaker so the request succeeds — is
/// how a session token quietly stops being a secret.
fn opaque_token() -> Option<String> {
    let mut bytes = [0u8; 32];
    SystemRandom::new().fill(&mut bytes).ok()?;
    Some(bytes.iter().fold(String::with_capacity(64), |mut acc, b| {
        use std::fmt::Write as _;
        // Writing into a String cannot fail; the result is discarded rather
        // than unwrapped so this stays panic-free.
        let _ = write!(acc, "{b:02x}");
        acc
    }))
}

impl SessionStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Establishes a context and returns its cookie value.
    ///
    /// Returns `None` when the store is full, which the caller reports as a
    /// transport failure rather than pretending the session exists.
    pub fn establish(
        &self,
        tenant: TenantId,
        user: UserId,
        user_dn: String,
        login: String,
        now: OffsetDateTime,
    ) -> Option<String> {
        let mut contexts = self.contexts.lock().ok()?;
        // Expired contexts are cleared on the way in, so the bound below counts
        // live sessions rather than everything ever created.
        contexts.retain(|_, context| context.expires_at > now);
        if contexts.len() >= MAX_CONTEXTS {
            return None;
        }
        let token = opaque_token()?;
        contexts.insert(
            token.clone(),
            SessionContext {
                tenant,
                user,
                user_dn,
                login,
                expires_at: now + CONTEXT_LIFETIME,
                objects: Arc::new(Mutex::new(crate::dispatch::ObjectTable::new())),
            },
        );
        Some(token)
    }

    /// Looks a context up and extends it, as any use of a context refreshes it
    /// ([MS-OXCMAPIHTTP] §3.1.5.5). An expired context is not found — it is
    /// removed, so a stale cookie can never be revived.
    pub fn touch(&self, token: &str, now: OffsetDateTime) -> Option<SessionContext> {
        let mut contexts = self.contexts.lock().ok()?;
        let context = contexts.get_mut(token)?;
        if context.expires_at <= now {
            contexts.remove(token);
            return None;
        }
        context.expires_at = now + CONTEXT_LIFETIME;
        Some(context.clone())
    }

    /// Ends a context. Idempotent: disconnecting an unknown context is not an
    /// error, because a client retrying a `Disconnect` it already completed is
    /// behaving correctly.
    pub fn end(&self, token: &str) {
        if let Ok(mut contexts) = self.contexts.lock() {
            contexts.remove(token);
        }
    }

    /// How many contexts are live (used by tests and, later, a metric).
    #[must_use]
    pub fn len(&self) -> usize {
        self.contexts.lock().map(|c| c.len()).unwrap_or(0)
    }

    /// Whether no context is live.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The `Set-Cookie` header value for a context cookie.
///
/// `HttpOnly` because no script has business reading it; `Secure` because this
/// protocol carries credentials and must not travel in clear; `SameSite=None`
/// because Outlook is not a browser and no same-site notion applies, and `Path`
/// scoped to the endpoint family rather than the whole origin.
#[must_use]
pub fn set_cookie(name: &str, value: &str) -> String {
    format!("{name}={value}; Path=/mapi; HttpOnly; Secure; SameSite=None")
}

/// Reads one cookie's value out of a `Cookie` header.
///
/// Matching is on the whole name between delimiters, so a cookie called
/// `NotMapiContext` cannot be mistaken for `MapiContext` — a suffix match here
/// would let a client choose which cookie we read.
#[must_use]
pub fn cookie_value(header: &str, name: &str) -> Option<String> {
    header.split(';').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key.trim() == name).then(|| value.trim().to_owned())
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn ids() -> (TenantId, UserId) {
        (TenantId::generate(), UserId::generate())
    }

    #[test]
    fn a_context_is_found_until_it_expires_and_never_after() {
        let store = SessionStore::new();
        let (tenant, user) = ids();
        let now = OffsetDateTime::UNIX_EPOCH;
        let token = store
            .establish(
                tenant.clone(),
                user,
                "/o=alo/cn=x".to_owned(),
                "x@alo.test".to_owned(),
                now,
            )
            .expect("established");

        assert!(store.touch(&token, now).is_some());
        // Still inside the window.
        assert!(store.touch(&token, now + Duration::minutes(10)).is_some());
        // The touch above pushed expiry out, so this is inside the new window.
        assert!(store.touch(&token, now + Duration::minutes(20)).is_some());
        // Long enough after the last use, it is gone — and stays gone.
        assert!(store.touch(&token, now + Duration::hours(2)).is_none());
        assert!(store.touch(&token, now + Duration::hours(2)).is_none());
        assert!(store.is_empty(), "an expired context was left behind");
    }

    /// The cookie is the bearer of a mailbox session. Two contexts must never
    /// share a value, and the value must not be derived from anything about the
    /// user — a predictable cookie is a session-hijacking primitive.
    #[test]
    fn context_cookies_are_unguessable_and_never_repeat() {
        let store = SessionStore::new();
        let (tenant, user) = ids();
        let now = OffsetDateTime::UNIX_EPOCH;

        let mut seen = std::collections::HashSet::new();
        for _ in 0..256 {
            let token = store
                .establish(
                    tenant.clone(),
                    user.clone(),
                    "/o=alo/cn=x".to_owned(),
                    "x@alo.test".to_owned(),
                    now,
                )
                .expect("established");
            assert_eq!(token.len(), 64, "expected 32 bytes of hex");
            assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
            // Nothing about the identity leaks into the token.
            assert!(!token.contains(tenant.as_str()));
            assert!(!token.contains(user.as_str()));
            assert!(seen.insert(token), "a context cookie repeated");
        }
    }

    /// A context carries the tenant that made it, so a later request answered
    /// from a context can never be answered for somebody else's tenant.
    #[test]
    fn a_context_remembers_the_tenant_that_created_it() {
        let store = SessionStore::new();
        let (tenant_a, user_a) = ids();
        let (tenant_b, user_b) = ids();
        let now = OffsetDateTime::UNIX_EPOCH;

        let a = store
            .establish(
                tenant_a.clone(),
                user_a.clone(),
                "a".to_owned(),
                "a@alo.test".to_owned(),
                now,
            )
            .unwrap();
        let b = store
            .establish(
                tenant_b.clone(),
                user_b,
                "b".to_owned(),
                "b@alo.test".to_owned(),
                now,
            )
            .unwrap();

        let found_a = store.touch(&a, now).expect("a");
        let found_b = store.touch(&b, now).expect("b");
        assert_eq!(found_a.tenant, tenant_a);
        assert_eq!(found_b.tenant, tenant_b);
        assert_ne!(found_a.tenant, found_b.tenant);
        // And one tenant's cookie does not open the other's context.
        assert_ne!(a, b);
    }

    #[test]
    fn ending_a_context_is_idempotent() {
        let store = SessionStore::new();
        let (tenant, user) = ids();
        let now = OffsetDateTime::UNIX_EPOCH;
        let token = store
            .establish(tenant, user, "x".to_owned(), "x@alo.test".to_owned(), now)
            .expect("established");

        store.end(&token);
        assert!(store.touch(&token, now).is_none());
        // A client retrying a Disconnect it already completed is behaving
        // correctly, so a second end is silent rather than an error.
        store.end(&token);
        store.end("never-existed");
    }

    /// A cookie name is matched whole. A suffix match would let a client send
    /// `NotMapiContext=…` and have us read it as the real one.
    #[test]
    fn a_cookie_is_matched_by_its_whole_name() {
        let header = "NotMapiContext=attacker; MapiContext=real; MapiContextExtra=no";
        assert_eq!(
            cookie_value(header, SESSION_COOKIE),
            Some("real".to_owned())
        );
        assert_eq!(cookie_value("Other=1", SESSION_COOKIE), None);
        assert_eq!(cookie_value("", SESSION_COOKIE), None);
        // A bare name with no value is not a value.
        assert_eq!(cookie_value("MapiContext", SESSION_COOKIE), None);
    }

    #[test]
    fn the_cookie_is_marked_secure_and_http_only() {
        let cookie = set_cookie(SESSION_COOKIE, "abc123");
        assert!(cookie.starts_with("MapiContext=abc123;"));
        assert!(cookie.contains("HttpOnly"), "{cookie}");
        assert!(cookie.contains("Secure"), "{cookie}");
        assert!(cookie.contains("Path=/mapi"), "{cookie}");
    }
}
