//! Which source address a message leaves by, chosen from its envelope sender.
//!
//! ADR 0044 §1: bulk mail leaves by a **separate IP** from transactional mail,
//! so a marketing reputation can never reach the address that carries invoices
//! and password resets. That separation is not a setting on a screen — it is the
//! source address of the TCP connection, because that is the only thing a
//! receiver's SPF check can see.
//!
//! The lookup key is the **envelope-from domain** and not the `From` header,
//! deliberately: SPF (RFC 7208 §2.4) is evaluated for the `MAIL FROM` identity,
//! so the domain whose record must authorise us is the one in the envelope. A
//! campaign identity under strict alignment (`aspf=s`) has both the same anyway;
//! keying on the envelope is what makes the choice correct rather than
//! coincidental.
//!
//! Unconfigured is the default and means the kernel chooses — today's behaviour,
//! unchanged, for every deployment that has one address.

use std::collections::HashMap;
use std::net::IpAddr;

/// Sending domain → the local address its mail must leave by.
#[derive(Debug, Clone, Default)]
pub struct EgressMap {
    by_domain: HashMap<String, IpAddr>,
}

impl EgressMap {
    /// Parses `domain=ip[,domain=ip…]`. An empty or whitespace-only spec is an
    /// empty map (no binding, kernel default).
    ///
    /// # Errors
    /// A human-readable reason naming the offending pair. The caller **must**
    /// treat this as fatal: a sender that shrugged off an unparseable egress map
    /// would fall back to the transactional address, which is the exact failure
    /// this module exists to prevent, and it would do so silently.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let mut by_domain = HashMap::new();
        for pair in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (domain, ip) = pair
                .split_once('=')
                .ok_or_else(|| format!("expected domain=ip, found {pair:?}"))?;
            let domain = domain.trim().trim_matches('.').to_ascii_lowercase();
            if domain.is_empty() || !domain.contains('.') {
                return Err(format!("{domain:?} is not a sending domain"));
            }
            let ip: IpAddr = ip
                .trim()
                .parse()
                .map_err(|_| format!("{:?} is not an IP address", ip.trim()))?;
            if by_domain.insert(domain.clone(), ip).is_some() {
                // Two answers to "where does this domain leave from" is a
                // configuration whose meaning depends on parse order.
                return Err(format!("{domain:?} is listed twice"));
            }
        }
        Ok(Self { by_domain })
    }

    /// The source address for an envelope sender, or `None` when this domain has
    /// no dedicated address (or the sender is the null path `<>`, which is a
    /// bounce — it carries no identity to keep separate).
    pub fn source_for(&self, mail_from: Option<&str>) -> Option<IpAddr> {
        let domain = sender_domain(mail_from?)?;
        self.by_domain.get(&domain).copied()
    }

    /// Whether any domain has a dedicated source address.
    pub fn is_empty(&self) -> bool {
        self.by_domain.is_empty()
    }

    /// The configured domains, for the startup log an operator reads to check
    /// the separation is actually on. Never the addresses of individual
    /// messages — only what was configured.
    pub fn domains(&self) -> Vec<&str> {
        let mut domains: Vec<&str> = self.by_domain.keys().map(String::as_str).collect();
        domains.sort_unstable();
        domains
    }
}

/// The domain of an envelope sender in display form, lowercased. `None` when
/// there is no `@` or either side of it is empty.
fn sender_domain(mail_from: &str) -> Option<String> {
    let addr = mail_from
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>');
    let (local, domain) = addr.rsplit_once('@')?;
    if local.is_empty() {
        return None;
    }
    let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    if domain.is_empty() {
        return None;
    }
    Some(domain)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn an_unconfigured_deployment_binds_nothing() {
        for spec in ["", "   ", ",", " , "] {
            let map = EgressMap::parse(spec).expect("empty spec parses");
            assert!(map.is_empty());
            assert_eq!(map.source_for(Some("anyone@example.test")), None);
        }
    }

    #[test]
    fn a_sending_domain_leaves_by_its_own_address() {
        let map = EgressMap::parse("news.alomails.com=159.195.89.28").unwrap();
        assert_eq!(
            map.source_for(Some("bounces@news.alomails.com")),
            Some(ip("159.195.89.28"))
        );
        // The parent domain is a different identity and keeps the default
        // address — a suffix match here would hand the transactional domain the
        // campaign IP, which is the separation running backwards.
        assert_eq!(map.source_for(Some("noreply@alomails.com")), None);
        assert_eq!(map.source_for(Some("x@deeper.news.alomails.com")), None);
    }

    #[test]
    fn the_domain_is_matched_however_the_sender_spelled_it() {
        let map = EgressMap::parse(" News.AloMails.Com = 159.195.89.28 ").unwrap();
        for sender in [
            "a@news.alomails.com",
            "a@NEWS.ALOMAILS.COM",
            "<a@News.Alomails.Com>",
            " a@news.alomails.com. ",
        ] {
            assert_eq!(
                map.source_for(Some(sender)),
                Some(ip("159.195.89.28")),
                "sender {sender:?} must resolve to the campaign address"
            );
        }
    }

    #[test]
    fn a_bounce_and_a_malformed_sender_take_the_default_route() {
        let map = EgressMap::parse("news.alomails.com=159.195.89.28").unwrap();
        // The null path `<>`: a bounce has no identity to keep separate.
        assert_eq!(map.source_for(None), None);
        for sender in ["", "postmaster", "@news.alomails.com", "a@", "a@ "] {
            assert_eq!(
                map.source_for(Some(sender)),
                None,
                "sender {sender:?} must not select an egress address"
            );
        }
    }

    #[test]
    fn several_identities_can_have_their_own_addresses() {
        let map = EgressMap::parse("news.alomails.com=159.195.89.28, news.other.test=2001:db8::5")
            .unwrap();
        assert_eq!(map.domains(), vec!["news.alomails.com", "news.other.test"]);
        assert_eq!(
            map.source_for(Some("a@news.other.test")),
            Some(ip("2001:db8::5"))
        );
    }

    #[test]
    fn a_spec_that_could_be_read_two_ways_is_refused_rather_than_guessed() {
        // Each of these would otherwise leave mail on the transactional
        // address while looking configured.
        for spec in [
            "news.alomails.com",                                         // no address
            "news.alomails.com=",                                        // empty address
            "news.alomails.com=159.195.89",                              // not an address
            "news.alomails.com=localhost",                               // not an address
            "=159.195.89.28",                                            // no domain
            "news=159.195.89.28",                                        // not a domain
            "news.alomails.com=159.195.89.28,news.alomails.com=1.2.3.4", // two answers
        ] {
            assert!(
                EgressMap::parse(spec).is_err(),
                "spec {spec:?} must be refused rather than half-applied"
            );
        }
    }
}
