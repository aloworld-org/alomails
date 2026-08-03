//! The message model the evaluator tests against: parsed headers (unfolded,
//! order-preserving, duplicates kept), the SMTP envelope, and the raw size.
//! Header/address parsing is self-contained so the engine depends on
//! nothing below it.

/// A message presented to the Sieve evaluator.
#[derive(Debug, Clone)]
pub struct Message {
    headers: Vec<(String, String)>,
    /// Raw message size in octets (for the `size` test).
    pub size: u64,
    /// Envelope MAIL FROM (return-path); `None` = `<>` (null sender).
    pub envelope_from: Option<String>,
    /// Envelope RCPT TO this delivery is for (for `envelope "to"` and
    /// subaddress `:detail`).
    pub envelope_to: String,
}

impl Message {
    /// Parses `raw` (RFC 5322) with the given envelope. `size` is the raw
    /// length.
    pub fn parse(raw: &[u8], envelope_from: Option<String>, envelope_to: String) -> Self {
        Self {
            headers: parse_headers(raw),
            size: raw.len() as u64,
            envelope_from,
            envelope_to,
        }
    }

    /// Builds a message from already-parsed pieces (used by callers that
    /// parsed the message once for the store).
    pub fn from_parts(
        headers: Vec<(String, String)>,
        size: u64,
        envelope_from: Option<String>,
        envelope_to: String,
    ) -> Self {
        Self {
            headers,
            size,
            envelope_from,
            envelope_to,
        }
    }

    /// All values of a header (case-insensitive name), in order.
    pub fn header_values(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// Whether a header exists.
    pub fn has_header(&self, name: &str) -> bool {
        self.headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(name))
    }

    /// The addresses in a header's value(s), as `(local, domain)` pairs plus
    /// the whole addr-spec.
    pub fn header_addresses(&self, name: &str) -> Vec<Address> {
        self.header_values(name)
            .iter()
            .flat_map(|v| parse_address_list(v))
            .collect()
    }
}

/// A parsed email address (`local@domain`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    /// The full `local@domain`.
    pub all: String,
    /// The local part (before `@`).
    pub local: String,
    /// The domain (after `@`).
    pub domain: String,
}

impl Address {
    /// Parses a bare addr-spec into an [`Address`]; `None` if it has no `@`.
    pub fn parse(spec: &str) -> Option<Address> {
        let spec = spec.trim();
        let (local, domain) = spec.rsplit_once('@')?;
        if local.is_empty() || domain.is_empty() {
            return None;
        }
        Some(Address {
            all: spec.to_owned(),
            local: local.to_owned(),
            domain: domain.to_owned(),
        })
    }

    /// The `:user` (local part with any `+detail` removed) and `:detail`
    /// (the part after the first `+`, or `None`), for subaddress.
    pub fn user_detail(&self) -> (String, Option<String>) {
        match self.local.split_once('+') {
            Some((user, detail)) => (user.to_owned(), Some(detail.to_owned())),
            None => (self.local.clone(), None),
        }
    }
}

/// Splits raw bytes into unfolded header `(name, value)` pairs, preserving
/// order and duplicates. Stops at the first empty line.
fn parse_headers(raw: &[u8]) -> Vec<(String, String)> {
    // Take the header block up to the first blank line.
    let end = find_header_end(raw);
    let text = String::from_utf8_lossy(&raw[..end]);
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = out.last_mut() {
                last.1.push(' ');
                last.1.push_str(line.trim());
            }
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            out.push((name.trim().to_owned(), value.trim().to_owned()));
        }
    }
    out
}

fn find_header_end(raw: &[u8]) -> usize {
    if let Some(i) = find(raw, b"\r\n\r\n") {
        return i;
    }
    if let Some(i) = find(raw, b"\n\n") {
        return i;
    }
    raw.len()
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Parses an address-list header value into [`Address`]es: handles
/// `Display Name <local@domain>`, bare `local@domain`, and comma lists.
pub fn parse_address_list(value: &str) -> Vec<Address> {
    let mut out = Vec::new();
    for part in split_commas(value) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let spec = if let (Some(lt), Some(gt)) = (part.rfind('<'), part.rfind('>')) {
            if lt < gt { &part[lt + 1..gt] } else { part }
        } else {
            part
        };
        if let Some(a) = Address::parse(spec) {
            out.push(a);
        }
    }
    out
}

fn split_commas(value: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let (mut in_quote, mut in_angle) = (false, false);
    for c in value.chars() {
        match c {
            '"' => {
                in_quote = !in_quote;
                cur.push(c);
            }
            '<' if !in_quote => {
                in_angle = true;
                cur.push(c);
            }
            '>' if !in_quote => {
                in_angle = false;
                cur.push(c);
            }
            ',' if !in_quote && !in_angle => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        parts.push(cur);
    }
    parts
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const RAW: &[u8] = b"From: Alice <alice@example.com>\r\n\
To: bob+urgent@example.org, carol@example.net\r\n\
Subject: Hi\r\n \tthere\r\n\r\nbody\r\n";

    #[test]
    fn headers_unfold_and_keep_order() {
        let m = Message::parse(
            RAW,
            Some("alice@example.com".into()),
            "bob@example.org".into(),
        );
        assert_eq!(m.header_values("subject"), vec!["Hi there"]);
        assert_eq!(m.header_values("from"), vec!["Alice <alice@example.com>"]);
        assert!(m.has_header("To"));
        assert!(!m.has_header("Cc"));
    }

    #[test]
    fn address_extraction_and_subaddress() {
        let m = Message::parse(RAW, None, "bob+urgent@example.org".into());
        let to = m.header_addresses("to");
        assert_eq!(to.len(), 2);
        assert_eq!(to[0].local, "bob+urgent");
        assert_eq!(to[0].domain, "example.org");
        let (user, detail) = to[0].user_detail();
        assert_eq!(user, "bob");
        assert_eq!(detail.as_deref(), Some("urgent"));
        assert_eq!(to[1].all, "carol@example.net");
    }
}
