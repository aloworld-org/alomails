//! The link in the mail that ends it (alo Campaigns, ADR 0044 §3, wave C2s.1) —
//! one unguessable token per recipient per send, stored only as a digest.
//!
//! ADR 0044 §3: *every campaign carries `List-Unsubscribe` with one-click
//! support (RFC 8058), and the link works without a login.* A link that works
//! without a login is a link whose URL **is** the whole credential, and that is
//! what every decision in this module answers to.
//!
//! ## The two failures this module exists to prevent
//!
//! The queue item names them, and each has a test rather than a paragraph.
//!
//! 1. **Unsubscribing other people by iterating identifiers.** The token is 256
//!    random bits ([`mint_campaign_unsubscribe_token`](TenantStore::mint_campaign_unsubscribe_token)),
//!    not an encoding of the recipient and the send. The alternative nearly
//!    every bulk sender ships — `?u=<customer id>&c=<campaign id>`, signed or
//!    not — hands whoever holds the mail an id to increment, and an unsubscribe
//!    link is forwarded, quoted in replies and fetched by every scanner between
//!    us and the recipient. Here there is nothing to increment: a wrong guess
//!    is a row that does not exist.
//! 2. **Confirming somebody's address is live by watching what the endpoint
//!    does.** There is exactly one way into a row —
//!    [`resolve_campaign_unsubscribe_token`](crate::Store::resolve_campaign_unsubscribe_token),
//!    by digest — and no method anywhere that answers *does this address have a
//!    token*. An unknown token, a malformed token and an empty token are all
//!    the same `Ok(None)`: a spammer who posts a million guesses learns which
//!    of their guesses were right, which is nothing, rather than which of their
//!    addresses we hold. The unit tests hold the module's own SQL to that, so a
//!    convenience lookup added later fails a test rather than shipping an
//!    oracle.
//!
//! ## What is stored, and what is not
//!
//! **The token is never stored.** Only `sha256(token)`, the same shape
//! [`crate::share`] keeps for a download link: a database dump, a backup on a
//! laptop and a `SELECT *` over somebody's shoulder are all read access to this
//! table, and none of them may hand over a working unsubscribe for somebody
//! else. The raw token exists once, in the return of the mint call, and is
//! never readable again — `resolve` takes it and cannot produce it.
//!
//! **The record id is the handle, and the token is not.**
//! [`CampaignUnsubscribeTokenId`] is what a suppression names in its
//! `source_ref` ([`crate::campaign_suppression`]), so *which send did they
//! leave over* stays answerable without the live credential being copied into a
//! second table.
//!
//! ## What is deliberately absent
//!
//! - **No expiry.** The difference from [`crate::share`], which lends a file
//!   for a fortnight: this is a person's ability to make us stop, and it must
//!   work when they find the mail two years later while searching for something
//!   else. A column that eventually turns an unsubscribe into a `404` is a
//!   column that eventually earns a complaint — and a complaint is the
//!   expensive one (ADR 0044 §4).
//! - **No revoke, and no update path.** Minting again for the same person and
//!   the same send adds a second live token rather than replacing the first: we
//!   hold only a digest, so "replace" means killing a link already sitting in
//!   somebody's inbox. Both work, and the suppression they write is idempotent.
//! - **No listing.** Nothing returns the tokens of a send, because a list of
//!   digests beside the addresses they belong to is the oracle above with extra
//!   steps.
//!
//! **Nothing here sends anything, and nothing here suppresses anything.** This
//! module answers "whose link is this"; acting on the answer is queue item
//! C2s.2 (the landing page) and C2s.3 (the suppression it fires), and the
//! separation is deliberate — a resolve that suppressed as a side effect would
//! unsubscribe everybody whose mail passed a link-prefetching scanner, which is
//! why RFC 8058 requires a POST in the first place.

use time::OffsetDateTime;

use crate::blob::hash_hex;
use crate::campaign_audience::normalise_address;
use crate::error::{Result, StoreError};
use crate::id::{CampaignUnsubscribeTokenId, TenantId, generate_token};
use crate::store::{Store, TenantStore};

/// The longest `send_ref` — matched to
/// [`SUPPRESSION_SOURCE_REF_MAX`](crate::campaign_suppression::SUPPRESSION_SOURCE_REF_MAX),
/// because the one flows into the other when somebody uses their link.
pub const UNSUBSCRIBE_SEND_REF_MAX: usize = 200;

/// Which send, and for whom. A struct rather than two `&str` arguments, because
/// two strings of the same type in a row is the call somebody eventually swaps
/// — and swapped here means minting a link that unsubscribes the wrong person.
#[derive(Debug, Clone)]
pub struct NewUnsubscribeToken<'a> {
    /// The send this link is minted for. Opaque today: the per-recipient send
    /// record is queue item C5m.1, and a reference to a table that does not
    /// exist yet is a guess rather than a foreign key.
    pub send_ref: &'a str,
    /// The recipient, in whatever casing the sender holds them; normalised here
    /// so the suppression this link eventually writes joins the audience rather
    /// than sitting beside it.
    pub address: &'a str,
}

/// A freshly minted link — **the only time the raw token exists.**
///
/// It goes into the `List-Unsubscribe` header and the footer of exactly one
/// message and is not recoverable afterwards: the row keeps a digest. A caller
/// that drops this without putting the token in the mail has minted a link
/// nobody can use, which is a bug in the caller and not a state this store can
/// repair.
#[derive(Debug, Clone)]
pub struct IssuedUnsubscribeToken {
    /// The record's handle — safe to log, safe to store, useless as a link.
    pub record: CampaignUnsubscribeTokenId,
    /// The secret itself. 256 bits, URL-safe, and never read back.
    pub token: String,
    /// The recipient as stored.
    pub address: String,
    pub issued_at: OffsetDateTime,
}

/// Whose link this is: everything the landing page needs to act, and nothing it
/// needs to display.
///
/// Carries the tenant because the token is the only thing that names one — the
/// public route has no account, exactly as [`crate::share`]'s does not, so the
/// caller goes through [`Store::for_tenant`] with what it finds here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsubscribeTokenTarget {
    /// The workspace whose mail this was.
    pub tenant: TenantId,
    /// The record — what the resulting suppression names as its `source_ref`.
    pub record: CampaignUnsubscribeTokenId,
    /// Which send.
    pub send_ref: String,
    /// The person, normalised. Needed to suppress them; **not** something the
    /// landing page may print back, because a page that echoes the address
    /// turns a forwarded mail into a disclosure.
    pub address: String,
    pub issued_at: OffsetDateTime,
}

/// The write. Plain insert: no `ON CONFLICT`, because a digest collision at 256
/// bits is not a case to handle gracefully — it is a broken RNG, and failing
/// the mint is the correct response to one.
fn mint_sql() -> &'static str {
    "INSERT INTO campaign_unsubscribe_tokens \
         (token_hash, tenant_id, id, send_ref, address) \
     VALUES ($1, $2, $3, $4, $5) \
     RETURNING issued_at"
}

/// The read, and the **only** way into a row.
///
/// Keyed on the digest alone and deliberately not on `tenant_id`: the caller is
/// the public route, which has no account and no tenant until this answers. The
/// 256-bit token is the scope — the same shape [`crate::share`]'s resolve has
/// carried since 0026 — and the tenant comes back so every subsequent read and
/// write goes through a tenant-scoped door.
fn resolve_sql() -> &'static str {
    "SELECT tenant_id, id, send_ref, address, issued_at \
       FROM campaign_unsubscribe_tokens \
      WHERE token_hash = $1"
}

/// Every statement this module can issue.
///
/// The same list [`crate::campaign_audience`], [`crate::campaign_consent`] and
/// [`crate::campaign_suppression`] keep. Here it carries a second promise
/// beyond "no campaign query reads the per-user address book": that there is no
/// way to reach a token except by its digest.
#[cfg(test)]
fn all_sql() -> Vec<&'static str> {
    vec![mint_sql(), resolve_sql()]
}

/// A row as [`resolve_sql`] returns it.
type TargetRow = (String, String, String, String, OffsetDateTime);

/// Mints one token. 256 bits, from the same cryptographically-random,
/// non-sequential source as every opaque id — two draws rather than one,
/// because an unsubscribe link is a bearer credential in a URL that will be
/// read by strangers, and 128 bits is the id budget rather than the secret one.
fn generate_unsubscribe_token() -> String {
    format!("{}{}", generate_token(), generate_token())
}

/// The validated shape of a [`NewUnsubscribeToken`], ready to bind.
struct Validated {
    send_ref: String,
    address: String,
}

/// Checks one mint, once, in one place — separated from the write so the rules
/// are testable without a database, and so there is exactly one of them.
fn validate(request: &NewUnsubscribeToken<'_>) -> Result<Validated> {
    let address = normalise_address(request.address).ok_or_else(|| {
        StoreError::Validation(
            "an unsubscribe link is minted for an address, and this is not one".to_owned(),
        )
    })?;

    let send_ref = request.send_ref.trim();
    if send_ref.is_empty() {
        return Err(StoreError::Validation(
            "an unsubscribe link says which send it came from".to_owned(),
        ));
    }
    if send_ref.chars().count() > UNSUBSCRIBE_SEND_REF_MAX {
        return Err(StoreError::Validation(format!(
            "an unsubscribe send reference fits in {UNSUBSCRIBE_SEND_REF_MAX} characters"
        )));
    }

    Ok(Validated {
        send_ref: send_ref.to_owned(),
        address,
    })
}

impl TenantStore {
    /// Mints one recipient's one-click unsubscribe link for one send.
    ///
    /// The returned [`IssuedUnsubscribeToken::token`] is the **only** copy of
    /// the secret: put it in the `List-Unsubscribe` header and the mail's
    /// footer, and do not store it. The row keeps `sha256(token)`, so nothing —
    /// including this store — can produce the link again.
    ///
    /// Minting twice for the same person and the same send is not an error and
    /// does not replace anything: both links stay live. See the module docs for
    /// why a dead unsubscribe link is the more expensive failure.
    ///
    /// On [`TenantStore`] rather than [`AccountStore`](crate::AccountStore) for
    /// the reason [`crate::campaign_suppression`] gives: a send is not a
    /// colleague's action, and the route that redeems what this mints has no
    /// account at all.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the address is not one, or the send
    /// reference is blank or too long; [`StoreError::Db`] on failure.
    pub async fn mint_campaign_unsubscribe_token(
        &self,
        request: &NewUnsubscribeToken<'_>,
    ) -> Result<IssuedUnsubscribeToken> {
        let valid = validate(request)?;
        let token = generate_unsubscribe_token();
        let record = CampaignUnsubscribeTokenId::generate();
        let (issued_at,): (OffsetDateTime,) = sqlx::query_as(mint_sql())
            .bind(hash_hex(token.as_bytes()))
            .bind(self.tenant().as_str())
            .bind(record.as_str())
            .bind(&valid.send_ref)
            .bind(&valid.address)
            .fetch_one(self.pool())
            .await
            .map_err(StoreError::Db)?;
        Ok(IssuedUnsubscribeToken {
            record,
            token,
            address: valid.address,
            issued_at,
        })
    }
}

impl Store {
    /// Whose unsubscribe link this is, or `None`.
    ///
    /// Cross-tenant, and on [`Store`] rather than a tenant-scoped door because
    /// the caller is the public landing page (queue item C2s.2): there is no
    /// login, and the token is what names the tenant.
    ///
    /// **`None` says nothing about anybody.** An unknown token, a malformed
    /// one, an empty one and the digest of a real one all return `None`, so the
    /// answer carries no information about which addresses this deployment
    /// holds — see the module docs' second failure. Nothing is suppressed here;
    /// resolving a link is not using it.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure. A token that is not one is not an error —
    /// an error and a miss are distinguishable, and that distinction is the
    /// oracle.
    pub async fn resolve_campaign_unsubscribe_token(
        &self,
        token: &str,
    ) -> Result<Option<UnsubscribeTokenTarget>> {
        let row: Option<TargetRow> = sqlx::query_as(resolve_sql())
            .bind(hash_hex(token.as_bytes()))
            .fetch_optional(self.pool())
            .await
            .map_err(StoreError::Db)?;
        Ok(row.map(
            |(tenant, record, send_ref, address, issued_at)| UnsubscribeTokenTarget {
                tenant: TenantId::new(tenant),
                record: CampaignUnsubscribeTokenId::new(record),
                send_ref,
                address,
                issued_at,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifiers a SQL string contains — see the twin of this helper in
    /// `campaign_audience.rs` for why identifiers rather than substrings.
    fn identifiers(sql: &str) -> Vec<&str> {
        sql.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|token| !token.is_empty())
            .collect()
    }

    fn new() -> NewUnsubscribeToken<'static> {
        NewUnsubscribeToken {
            send_ref: "send-2026-08",
            address: "Ann@Lead.TEST",
        }
    }

    #[test]
    fn no_query_in_this_module_can_read_the_per_user_address_book() {
        for sql in all_sql() {
            assert!(
                !identifiers(sql).contains(&"contacts"),
                "an unsubscribe query names the per-user address book: {sql}"
            );
        }
    }

    #[test]
    fn the_only_way_to_reach_a_token_is_to_hold_it() {
        // The item's second named failure: "confirming an address is live by
        // watching what the endpoint does." A lookup by address or by send is
        // exactly that oracle, so the absence is checked against the SQL rather
        // than trusted to review — a convenience query added next year fails
        // here instead of shipping.
        let sql = resolve_sql();
        assert!(
            sql.contains("WHERE token_hash = $1"),
            "the resolve is keyed on something other than the token: {sql}"
        );
        for reachable_by in ["address = ", "send_ref = ", "tenant_id = ", "id = "] {
            assert!(
                !sql.contains(reachable_by),
                "a token can be found by {reachable_by:?}: {sql}"
            );
        }
    }

    #[test]
    fn nothing_here_hands_back_a_working_link() {
        // Only the digest is stored, so no statement may return it either: a
        // read that yields `token_hash` is a read that yields the credential's
        // one stored form to whatever logs the row.
        assert!(
            !resolve_sql().contains("SELECT tenant_id, id, send_ref, address, issued_at, token"),
            "the resolve returns the stored digest"
        );
        let selected = resolve_sql()
            .split_once("FROM")
            .map(|(head, _)| head.to_owned())
            .unwrap_or_default();
        assert!(
            !identifiers(&selected).contains(&"token_hash"),
            "the digest is in the select list: {selected}"
        );
    }

    #[test]
    fn a_token_can_never_be_taken_away_or_rewritten() {
        // No revoke and no update: we hold a digest, so "replace" means killing
        // a link already sitting in somebody's inbox, and a dead unsubscribe
        // link is what makes a person press the spam button instead.
        for sql in all_sql() {
            let statements = identifiers(sql);
            for forbidden in ["DELETE", "delete", "UPDATE", "update"] {
                assert!(
                    !statements.contains(&forbidden),
                    "an unsubscribe link can be revoked: {sql}"
                );
            }
        }
    }

    #[test]
    fn the_mint_is_scoped_to_one_tenant() {
        // The resolve deliberately is not — the token is the scope there, and
        // the public route has no tenant until it answers — but nothing may
        // write into another workspace's tokens.
        assert!(mint_sql().contains("tenant_id"));
        assert!(
            mint_sql().contains("$2"),
            "the tenant is bound, not inlined"
        );
    }

    #[test]
    fn a_token_is_a_secret_rather_than_an_encoding_of_who_it_is_for() {
        // The item: "unguessable, identifying the send and the recipient,
        // revealing neither to whoever holds it." An unsubscribe link is
        // forwarded, quoted in replies and read by scanners, so what it carries
        // is what strangers learn.
        let a = generate_unsubscribe_token();
        let b = generate_unsubscribe_token();
        assert_ne!(a, b, "two links for the same recipient must differ");
        for part in ["ann", "lead.test", "ann@lead.test", "send-2026-08"] {
            assert!(
                !a.to_lowercase().contains(part),
                "the token spells out {part:?}: {a}"
            );
        }
        // 256 bits, as two base64url draws of 16 bytes: the id budget is 128,
        // and this is a bearer credential in a URL rather than an id.
        assert_eq!(a.len(), 44);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "a token that needs escaping in a URL is a link that breaks: {a}"
        );
    }

    #[test]
    fn the_stored_digest_is_not_the_link() {
        // What a database reader gets. `hash_hex` is SHA-256, so holding this
        // row is not holding the token — which is the whole reason the column
        // exists in that form.
        let token = generate_unsubscribe_token();
        let stored = hash_hex(token.as_bytes());
        assert_eq!(stored.len(), 64);
        assert!(
            stored
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_ne!(stored, token);
        assert_eq!(
            stored,
            hash_hex(token.as_bytes()),
            "the same link must find the same row"
        );
    }

    #[test]
    fn a_link_carries_the_address_folded_to_one_identity() {
        let valid = validate(&new()).unwrap_or_else(|e| panic!("refused a good one: {e:?}"));
        assert_eq!(valid.address, "ann@lead.test");
        assert_eq!(valid.send_ref, "send-2026-08");
    }

    #[test]
    fn an_address_nobody_could_be_mailed_at_cannot_be_given_a_link() {
        // A token minted for junk is a link whose suppression would not join
        // the audience — somebody who pressed unsubscribe and is still mailed.
        for junk in ["", "   ", "n/a", "ask reception", "ann@localhost"] {
            let candidate = NewUnsubscribeToken {
                address: junk,
                ..new()
            };
            assert!(
                matches!(validate(&candidate), Err(StoreError::Validation(_))),
                "minted a link for {junk:?}"
            );
        }
    }

    #[test]
    fn a_link_says_which_send_it_came_from() {
        // Blank is refused rather than stored: an unsubscribe that cannot name
        // its send is one the tenant cannot learn anything from, and C5m.1 will
        // hang the send record off exactly this reference.
        for blank in ["", "   ", "\t\n"] {
            let candidate = NewUnsubscribeToken {
                send_ref: blank,
                ..new()
            };
            assert!(matches!(
                validate(&candidate),
                Err(StoreError::Validation(_))
            ));
        }
        let padded = NewUnsubscribeToken {
            send_ref: "  send-2026-08  ",
            ..new()
        };
        assert_eq!(
            validate(&padded)
                .unwrap_or_else(|e| panic!("{e:?}"))
                .send_ref,
            "send-2026-08"
        );
        let long = "s".repeat(UNSUBSCRIBE_SEND_REF_MAX + 1);
        let overlong = NewUnsubscribeToken {
            send_ref: &long,
            ..new()
        };
        assert!(matches!(
            validate(&overlong),
            Err(StoreError::Validation(_))
        ));
    }
}
