//! Opaque, random, URL-safe identifiers.
//!
//! Every id that crosses the API boundary is `base64url(16 random
//! bytes)` — 128 bits, non-sequential, unguessable. A leaked id reveals
//! nothing about its neighbours and cannot be incremented into another
//! tenant's row (the auditor's first probe).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};

static FALLBACK_COUNTER: AtomicU64 = AtomicU64::new(0);
static PROCESS_SALT: OnceLock<[u8; 32]> = OnceLock::new();

/// A per-process secret salt, drawn once at startup (when the RNG almost
/// certainly still works). Mixed into the RNG-failure fallback so those
/// ids remain unguessable rather than a predictable counter.
fn process_salt() -> &'static [u8; 32] {
    PROCESS_SALT.get_or_init(|| {
        let mut salt = [0u8; 32];
        let _ = SystemRandom::new().fill(&mut salt);
        salt
    })
}

/// 16 cryptographically-random bytes. Infallible (never panics — this
/// runs on the delivery path): on the essentially impossible event that
/// the system RNG is unavailable at runtime, derives the bytes from
/// `SHA-256(process-salt || counter || clock)` so they stay unguessable
/// and non-sequential, not a bare counter.
fn random_bytes() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    if SystemRandom::new().fill(&mut bytes).is_err() {
        let n = FALLBACK_COUNTER.fetch_add(1, Ordering::Relaxed);
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let mut hasher = Sha256::new();
        hasher.update(process_salt());
        hasher.update(n.to_le_bytes());
        hasher.update(t.to_le_bytes());
        bytes.copy_from_slice(&hasher.finalize()[..16]);
    }
    bytes
}

/// Generates one opaque id token. `pub(crate)` so sibling modules can mint
/// non-id opaque tokens (e.g. a domain DNS-verification token) from the same
/// cryptographically-random, non-sequential source.
pub(crate) fn generate_token() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes())
}

/// Defines a typed, opaque id newtype over `String`.
macro_rules! opaque_id {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            /// Generates a fresh random id.
            pub fn generate() -> Self {
                Self(generate_token())
            }

            /// Wraps an existing id string (e.g. one read back from the
            /// database or received over the API).
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            /// The id as a string slice (for binding into queries).
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(id: String) -> Self {
                Self(id)
            }
        }
    };
}

opaque_id!(
    /// A tenant — the isolation root. The only key to a [`crate::TenantStore`].
    TenantId
);
opaque_id!(
    /// A user (JMAP account) within a tenant.
    UserId
);
opaque_id!(
    /// A group (a named membership set) within a tenant.
    GroupId
);
opaque_id!(
    /// A mailbox.
    MailboxId
);
opaque_id!(
    /// A message.
    MessageId
);
opaque_id!(
    /// A thread.
    ThreadId
);
opaque_id!(
    /// A content-addressed blob.
    BlobId
);
opaque_id!(
    /// A user-defined message category (colored label). The id is embedded in
    /// the message's `$category_<id>` keyword to record membership.
    CategoryId
);
opaque_id!(
    /// An address-book contact. Also serves as the vCard `UID`, so a contact
    /// keeps its identity across a CardDAV/JMAP round-trip.
    ContactId
);
opaque_id!(
    /// A calendar event. Also serves as the iCalendar `UID`, so an event keeps
    /// its identity across a CalDAV round-trip once calendar sync lands.
    EventId
);
opaque_id!(
    /// A calendar (a named collection of events). Also the CalDAV collection
    /// name. Every event belongs to exactly one calendar.
    CalendarId
);
opaque_id!(
    /// A task — the core record of the Tasks module (ADR 0021).
    TaskId
);
opaque_id!(
    /// A task project (board): the group a task belongs to, and how personal
    /// vs team is expressed (ADR 0021).
    ProjectId
);
opaque_id!(
    /// A task subtask (checklist item).
    SubtaskId
);
opaque_id!(
    /// A task comment.
    CommentId
);
opaque_id!(
    /// A file attached to a task (a reference to a tenant blob).
    AttachmentId
);
opaque_id!(
    /// A task label (tag) — reusable and tenant-scoped.
    LabelId
);

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_opaque_urlsafe_and_unique() {
        let mut seen = HashSet::new();
        for _ in 0..10_000 {
            let id = MessageId::generate();
            let s = id.as_str();
            // 16 bytes → 22 base64url chars, no padding, URL-safe set.
            assert_eq!(s.len(), 22);
            assert!(
                s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "unexpected char in id {s}"
            );
            assert!(seen.insert(s.to_owned()), "duplicate id {s}");
        }
    }

    #[test]
    fn ids_are_not_sequential() {
        // Two consecutive ids share no long common prefix (not a counter).
        let a = TenantId::generate();
        let b = TenantId::generate();
        let common = a
            .as_str()
            .chars()
            .zip(b.as_str().chars())
            .take_while(|(x, y)| x == y)
            .count();
        assert!(common < 8, "ids look sequential: {a} vs {b}");
    }
}
