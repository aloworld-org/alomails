//! Matching a conversation to a deal — the pure half of the CRM's thread
//! suggestions (alo CRM, ADR 0035, wave B2).
//!
//! Nothing here touches the database and nothing here links anything. Given the
//! addresses a deal knows about and the correspondents of one message, it
//! answers a single question: *would a person say this conversation is about
//! that deal, and why?* The store ([`crate::crm_deal_threads`]) folds that
//! answer over a page of the requesting user's own recent mail; a **link** only
//! ever happens on an explicit, confirmed write.
//!
//! Two rules keep the heuristic honest, and both are the reason this is a file
//! of its own rather than a closure inside a query:
//!
//! - **A suggestion is a proposal, exactly like an AI one** (ADR 0023's posture
//!   applied to a heuristic): it carries the reason it matched and becomes a
//!   link only when a user says so. Automatic linking on a domain match is the
//!   obvious feature and it is wrong twice — a customer with three deals would
//!   have every conversation attached to all three, and a tenant whose customer
//!   mails from a shared free-mail domain would find private mail attached to a
//!   record the whole company reads.
//! - **Free-mail domains never match by domain.** Half of European SME
//!   customers mail from Gmail; domain-matching there would propose every
//!   personal message the user has. For those, only the full address matches.

/// Why a conversation was proposed for a deal. Ordered: an address match is
/// always the better answer, and the suggestion list is sorted by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchReason {
    /// A correspondent's domain is the domain of one of the deal's addresses,
    /// and it is not a free-mail domain.
    Domain,
    /// A correspondent **is** one of the deal's addresses.
    Address,
}

impl MatchReason {
    /// The word the HTTP surface publishes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Address => "address",
            Self::Domain => "domain",
        }
    }
}

/// Domains whose users are individuals rather than one company, so sharing a
/// domain says nothing about sharing a customer.
///
/// Kept as an explicit, sorted list rather than a clever rule: being wrong here
/// means proposing somebody's private mail, and a list a human can read and
/// correct is worth more than a heuristic that is right most of the time. It
/// leans European because our customers are. Sorted — [`is_free_mail_domain`]
/// binary-searches it, and the test below fails if the order ever slips.
const FREE_MAIL_DOMAINS: &[&str] = &[
    "aol.com",
    "bluewin.ch",
    "btinternet.com",
    "comcast.net",
    "eircom.net",
    "email.it",
    "fastmail.com",
    "free.fr",
    "freenet.de",
    "gmail.com",
    "gmx.at",
    "gmx.ch",
    "gmx.com",
    "gmx.de",
    "gmx.net",
    "googlemail.com",
    "hey.com",
    "home.nl",
    "hotmail.co.uk",
    "hotmail.com",
    "hotmail.de",
    "hotmail.fr",
    "hotmail.it",
    "hushmail.com",
    "icloud.com",
    "interia.pl",
    "kpnmail.nl",
    "laposte.net",
    "libero.it",
    "live.be",
    "live.co.uk",
    "live.com",
    "live.nl",
    "mac.com",
    "mail.com",
    "mail.ru",
    "mailbox.org",
    "me.com",
    "msn.com",
    "o2.pl",
    "online.no",
    "op.pl",
    "orange.fr",
    "outlook.be",
    "outlook.com",
    "outlook.de",
    "outlook.fr",
    "pm.me",
    "posteo.de",
    "proton.me",
    "protonmail.com",
    "sapo.pt",
    "seznam.cz",
    "sfr.fr",
    "sky.com",
    "skynet.be",
    "t-online.de",
    "telenet.be",
    "telfort.nl",
    "terra.es",
    "tin.it",
    "tiscali.it",
    "tuta.com",
    "tutanota.com",
    "verizon.net",
    "virgilio.it",
    "virginmedia.com",
    "wanadoo.fr",
    "web.de",
    "wp.pl",
    "xs4all.nl",
    "ziggo.nl",
    "zoho.com",
    "zonnet.nl",
];

/// Free-mail families that run a domain per country (`yahoo.fr`, `yandex.ru`,
/// `aol.de`, …). Listing every ccTLD would be a list nobody can keep correct, so
/// the family prefix carries them — and it is the whole label plus a dot, so
/// `yahoo-consulting.de` is a company, not a free-mail provider.
const FREE_MAIL_FAMILIES: &[&str] = &["aol.", "gmx.", "hotmail.", "live.", "yahoo.", "yandex."];

/// Whether a domain is one individuals hold accounts at, in which case only a
/// **full address** may match it.
///
/// The input is expected lowercase (every address this module handles goes
/// through [`normalize_address`] first); it is lowercased again anyway, because
/// a caller getting that wrong must not turn into a privacy leak.
#[must_use]
pub fn is_free_mail_domain(domain: &str) -> bool {
    let domain = domain.trim().to_ascii_lowercase();
    if FREE_MAIL_DOMAINS.binary_search(&domain.as_str()).is_ok() {
        return true;
    }
    FREE_MAIL_FAMILIES
        .iter()
        .any(|family| domain.starts_with(family))
}

/// The domain part of an address, or `None` when there is not exactly one `@`
/// with something on both sides of it.
#[must_use]
pub fn domain_of(address: &str) -> Option<&str> {
    let (local, domain) = address.split_once('@')?;
    if local.is_empty() || domain.is_empty() || domain.contains('@') || !domain.contains('.') {
        return None;
    }
    Some(domain)
}

/// Normalises one address to the form this module compares: trimmed of display
/// name and angle brackets, lowercased, and `None` unless it is shaped like an
/// address at all.
///
/// Case-folding the local part is technically lossy (RFC 5321 §2.4 leaves it to
/// the receiving host), but every mail UI on earth treats `Ada@acme.test` and
/// `ada@acme.test` as one person, and a *suggestion* that disagrees with the
/// user's own address book is a suggestion they will not trust.
#[must_use]
pub fn normalize_address(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let inner = match (raw.rfind('<'), raw.rfind('>')) {
        (Some(lt), Some(gt)) if lt < gt => raw[lt + 1..gt].trim(),
        _ => raw,
    };
    let inner = inner.trim_matches(|c: char| c.is_whitespace() || c == '"' || c == ',');
    if inner.contains(char::is_whitespace) {
        return None;
    }
    domain_of(inner)?;
    Some(inner.to_ascii_lowercase())
}

/// The addresses in an address-list header value (`From`, `To`), each
/// normalised, in order, without duplicates.
///
/// A stray comma inside a quoted display name yields a fragment with no `@`,
/// which is dropped; the real address in the same entry still survives.
#[must_use]
pub fn addresses_in(header_value: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in header_value.split(',') {
        if let Some(address) = normalize_address(part)
            && !out.contains(&address)
        {
            out.push(address);
        }
    }
    out
}

/// The deal's own addresses, normalised once so a page of messages is not
/// re-parsing them per row. Blank and malformed entries simply fall out: a deal
/// with no usable address gets no suggestions, which is the honest answer.
#[must_use]
pub fn targets(raw: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for candidate in raw {
        if let Some(address) = normalize_address(candidate)
            && !out.contains(&address)
        {
            out.push(address);
        }
    }
    out
}

/// The best reason one message's correspondents give for proposing it, with the
/// correspondent that caused it — so the UI can show *why*, which is what makes
/// a proposal reviewable rather than magic.
///
/// `header_values` are raw header strings (`From`, `To`); an exact address match
/// wins outright, a domain match is kept only if no address matches anywhere in
/// the message, and a free-mail domain never matches by domain at all.
#[must_use]
pub fn match_message(targets: &[String], header_values: &[&str]) -> Option<(MatchReason, String)> {
    let mut domain_hit: Option<String> = None;
    for value in header_values {
        for address in addresses_in(value) {
            if targets.contains(&address) {
                return Some((MatchReason::Address, address));
            }
            if domain_hit.is_some() {
                continue;
            }
            let Some(domain) = domain_of(&address) else {
                continue;
            };
            if is_free_mail_domain(domain) {
                continue;
            }
            if targets
                .iter()
                .any(|t| domain_of(t).is_some_and(|d| d == domain))
            {
                domain_hit = Some(address);
            }
        }
    }
    domain_hit.map(|address| (MatchReason::Domain, address))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(addresses: &[&str]) -> Vec<String> {
        targets(addresses)
    }

    #[test]
    fn the_free_mail_list_stays_sorted_and_unique() {
        // `is_free_mail_domain` binary-searches it: an unsorted entry is a
        // silently missed provider, which is a privacy bug, not a style one.
        let mut sorted = FREE_MAIL_DOMAINS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.as_slice(), FREE_MAIL_DOMAINS);
        assert!(
            FREE_MAIL_DOMAINS
                .iter()
                .all(|d| *d == d.to_ascii_lowercase()),
            "entries are compared lowercase"
        );
    }

    #[test]
    fn free_mail_covers_the_named_providers_and_their_country_domains() {
        for free in [
            "gmail.com",
            "GMAIL.COM",
            "outlook.com",
            "hotmail.com",
            "yahoo.com",
            "yahoo.co.uk",
            "yahoo.fr",
            "proton.me",
            "gmx.de",
            "gmx.pt",
            "yandex.ru",
            "web.de",
            "orange.fr",
        ] {
            assert!(is_free_mail_domain(free), "expected free mail: {free}");
        }
        for company in [
            "acme.test",
            "acme.de",
            // A family prefix is the whole label plus a dot: a company that
            // happens to start with those letters is still a company.
            "yahoo-consulting.de",
            "livewire.nl",
            "gmxsolutions.com",
            "protonics.eu",
        ] {
            assert!(!is_free_mail_domain(company), "expected company: {company}");
        }
    }

    #[test]
    fn an_address_is_recognised_inside_a_display_name_or_bare() {
        assert_eq!(
            normalize_address("Ada Lovelace <Ada@Acme.test>").as_deref(),
            Some("ada@acme.test")
        );
        assert_eq!(
            normalize_address("  ADA@acme.test  ").as_deref(),
            Some("ada@acme.test")
        );
        assert_eq!(
            normalize_address("\"Doe, John\" <j@x.eu>").as_deref(),
            Some("j@x.eu")
        );
        for bad in [
            "",
            "   ",
            "not an address",
            "ada@",
            "@acme.test",
            "ada@@acme.test",
            "ada@localhost",
            "Ada Lovelace",
        ] {
            assert_eq!(normalize_address(bad), None, "accepted {bad:?}");
        }
    }

    #[test]
    fn a_header_list_yields_its_addresses_once_each() {
        assert_eq!(
            addresses_in("Ada <ada@acme.test>, bob@acme.test, Ada <ADA@acme.test>"),
            vec!["ada@acme.test".to_owned(), "bob@acme.test".to_owned()]
        );
        assert!(addresses_in("").is_empty());
        assert!(addresses_in("undisclosed-recipients:;").is_empty());
        // The quoted comma splits the entry, but the real address survives.
        assert_eq!(
            addresses_in("\"Doe, John\" <j@x.eu>"),
            vec!["j@x.eu".to_owned()]
        );
    }

    #[test]
    fn an_exact_address_wins_wherever_it_appears() {
        let targets = t(&["ada@acme.test"]);
        assert_eq!(
            match_message(&targets, &["ada@acme.test", ""]),
            Some((MatchReason::Address, "ada@acme.test".to_owned()))
        );
        // Outbound: the deal's contact is on the To line, not the From line.
        assert_eq!(
            match_message(&targets, &["me@ourco.test", "Ada <ada@acme.test>"]),
            Some((MatchReason::Address, "ada@acme.test".to_owned()))
        );
    }

    #[test]
    fn a_company_domain_matches_a_colleague_of_the_contact() {
        let targets = t(&["ada@acme.test"]);
        assert_eq!(
            match_message(&targets, &["Bob <bob@acme.test>"]),
            Some((MatchReason::Domain, "bob@acme.test".to_owned()))
        );
        // An address match anywhere in the message beats a domain match found
        // earlier — the reason shown must be the strongest one there is.
        assert_eq!(
            match_message(&targets, &["bob@acme.test", "ada@acme.test"]),
            Some((MatchReason::Address, "ada@acme.test".to_owned()))
        );
    }

    #[test]
    fn a_free_mail_contact_matches_only_on_the_full_address() {
        // This is the rule that keeps a salesperson's personal mail out of a
        // record the whole company reads.
        let targets = t(&["ada@gmail.com"]);
        assert_eq!(
            match_message(&targets, &["Ada <ada@gmail.com>"]),
            Some((MatchReason::Address, "ada@gmail.com".to_owned()))
        );
        for stranger in [
            "mum@gmail.com",
            "bank@yahoo.fr",
            "someone@outlook.com",
            "friend@gmx.de",
        ] {
            assert_eq!(
                match_message(&targets, &[stranger]),
                None,
                "domain-matched a free-mail address: {stranger}"
            );
        }
    }

    #[test]
    fn nothing_matches_without_targets_or_correspondents() {
        assert_eq!(match_message(&[], &["ada@acme.test"]), None);
        assert_eq!(match_message(&t(&["ada@acme.test"]), &[]), None);
        assert_eq!(match_message(&t(&["ada@acme.test"]), &["", "  "]), None);
        // A deal whose only "address" is unusable proposes nothing at all.
        assert!(t(&["", "  ", "not an address"]).is_empty());
    }

    #[test]
    fn targets_are_normalised_once_and_deduplicated() {
        assert_eq!(
            t(&[
                "Ada <Ada@Acme.test>",
                "ada@acme.test",
                "",
                "billing@acme.test"
            ]),
            vec!["ada@acme.test".to_owned(), "billing@acme.test".to_owned()]
        );
    }

    #[test]
    fn an_address_match_outranks_a_domain_match() {
        assert!(MatchReason::Address > MatchReason::Domain);
        assert_eq!(MatchReason::Address.as_str(), "address");
        assert_eq!(MatchReason::Domain.as_str(), "domain");
    }
}
