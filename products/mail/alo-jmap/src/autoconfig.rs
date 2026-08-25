//! Mail-client autoconfiguration: let a mail app configure itself from
//! nothing but the user's email address, instead of the user hand-typing
//! server names and ports.
//!
//! Two unauthenticated, read-only endpoints serve the same public facts —
//! our IMAPS host/port and SMTPS host/port — in the two formats real
//! clients ask for:
//!
//! * **Mozilla autoconfig** (Thunderbird, and Apple Mail as a fallback):
//!   `GET …/mail/config-v1.1.xml` → a `clientConfig` document. Clients look
//!   for it at `https://autoconfig.<email-domain>/mail/config-v1.1.xml` and
//!   at `https://<email-domain>/.well-known/autoconfig/mail/config-v1.1.xml`.
//! * **Microsoft POX Autodiscover** (Outlook): `POST/GET
//!   /autodiscover/autodiscover.xml` → an `Autodiscover` document. Outlook
//!   looks for it at `https://autodiscover.<email-domain>/autodiscover/…`.
//!
//! These reveal only public connection settings (host, port, TLS) — never a
//! secret — so they are deliberately unauthenticated, as the specs require
//! (the client has no credentials yet). The DNS records that point a mail
//! *domain* at this server for discovery are an operator step, documented in
//! the deployment README; the XML itself is generated here.

use axum::extract::{Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::state::AppState;

/// IMAP-over-TLS port we advertise (implicit TLS).
const IMAPS_PORT: u16 = 993;
/// SMTP submission-over-TLS port we advertise (implicit TLS).
const SMTPS_PORT: u16 = 465;

/// `?emailaddress=` (Thunderbird spells it lowercase, one word).
#[derive(Deserialize, Default)]
pub struct MozillaQuery {
    #[serde(default)]
    emailaddress: Option<String>,
}

/// The server FQDN clients connect to for IMAP/SMTP — the host of the JMAP
/// base URL (e.g. `mail.example.com`), stripped of scheme, port, and path.
fn server_host(base_url: &str) -> String {
    base_url
        .split("://")
        .nth(1)
        .unwrap_or(base_url)
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_lowercase()
}

/// The email domain to name in the config. Prefer the domain of the address
/// the client is asking about (so a multi-domain deployment answers for the
/// domain queried), but only if it is a syntactically sane hostname —
/// otherwise fall back to the server's configured mail domain. Validating the
/// charset keeps a caller-supplied value from breaking out of the XML.
fn email_domain(queried: Option<&str>, fallback: &str) -> String {
    if let Some(addr) = queried
        && let Some((_, domain)) = addr.rsplit_once('@')
    {
        let domain = domain.trim().trim_end_matches('.').to_lowercase();
        if is_plain_hostname(&domain) {
            return domain;
        }
    }
    fallback.to_lowercase()
}

/// True for a non-empty `label.label` hostname of `[a-z0-9-.]` only — the
/// charset that needs no XML escaping and cannot inject markup.
fn is_plain_hostname(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 253
        && s.contains('.')
        && !s.starts_with('.')
        && !s.starts_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
}

/// Escapes the five XML predefined entities so a caller-supplied value is
/// inert as element text.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

fn xml_response(body: String) -> Response {
    ([(CONTENT_TYPE, "application/xml; charset=utf-8")], body).into_response()
}

/// The Mozilla `clientConfig` document (Thunderbird / Apple Mail).
///
/// `%EMAILADDRESS%` is a placeholder Thunderbird substitutes with the real
/// address; using it means we never echo caller input into the username
/// field. Auth is `password-cleartext` — i.e. AUTH PLAIN/LOGIN carried inside
/// the TLS channel, which is exactly how our IMAP/SMTP accept credentials.
pub async fn mozilla(State(state): State<AppState>, Query(q): Query<MozillaQuery>) -> Response {
    let host = xml_escape(&server_host(&state.base_url));
    let fallback = crate::security::mail_domain(&state.base_url);
    let domain = xml_escape(&email_domain(q.emailaddress.as_deref(), &fallback));
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<clientConfig version="1.1">
  <emailProvider id="{domain}">
    <domain>{domain}</domain>
    <displayName>alo</displayName>
    <displayShortName>alo</displayShortName>
    <incomingServer type="imap">
      <hostname>{host}</hostname>
      <port>{IMAPS_PORT}</port>
      <socketType>SSL</socketType>
      <authentication>password-cleartext</authentication>
      <username>%EMAILADDRESS%</username>
    </incomingServer>
    <outgoingServer type="smtp">
      <hostname>{host}</hostname>
      <port>{SMTPS_PORT}</port>
      <socketType>SSL</socketType>
      <authentication>password-cleartext</authentication>
      <username>%EMAILADDRESS%</username>
    </outgoingServer>
  </emailProvider>
</clientConfig>
"#
    );
    xml_response(body)
}

/// The Microsoft POX Autodiscover document (Outlook).
///
/// Accepts both `GET` and `POST`; Outlook POSTs an XML request whose
/// `<EMailAddress>` we echo (escaped) as the login name, and falls back to
/// the address the user typed when we omit it. We only ever emit public
/// connection settings.
///
/// IMAP and SMTP are all this offers. The `mapiHttp` block that used to appear
/// here is gone with the adapter it advertised
/// ([ADR 0056](../../../../docs/decisions/0056-our-own-client-on-443-is-the-product.md)):
/// alo's own client over 443 is the product, and Outlook connects — if it
/// connects at all — the way every other third-party client does.
pub async fn outlook(
    State(state): State<AppState>,
    _headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let host = xml_escape(&server_host(&state.base_url));
    let login = extract_email(&body)
        .filter(|e| is_email(e))
        .map(|e| format!("<LoginName>{}</LoginName>", xml_escape(&e)))
        .unwrap_or_default();
    let body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<Autodiscover xmlns="http://schemas.microsoft.com/exchange/autodiscover/responseschema/2006">
  <Response xmlns="http://schemas.microsoft.com/exchange/autodiscover/outlook/responseschema/2006a">
    <Account>
      <AccountType>email</AccountType>
      <Action>settings</Action>
      <Protocol>
        <Type>IMAP</Type>
        <Server>{host}</Server>
        <Port>{IMAPS_PORT}</Port>
        <SSL>on</SSL>
        <SPA>off</SPA>
        <AuthRequired>on</AuthRequired>
        {login}
      </Protocol>
      <Protocol>
        <Type>SMTP</Type>
        <Server>{host}</Server>
        <Port>{SMTPS_PORT}</Port>
        <SSL>on</SSL>
        <SPA>off</SPA>
        <AuthRequired>on</AuthRequired>
      </Protocol>
    </Account>
  </Response>
</Autodiscover>
"#
    );
    xml_response(body)
}

/// Pulls the address out of an Autodiscover `<EMailAddress>…</EMailAddress>`
/// request body without a full XML parse (the element is flat text).
fn extract_email(body: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(body).ok()?;
    let open = text.find("<EMailAddress>")? + "<EMailAddress>".len();
    let rest = &text[open..];
    let close = rest.find("</EMailAddress>")?;
    let value = rest[..close].trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// A minimal `local@domain` sanity check — enough to refuse markup or a stray
/// value in the login field; the mail server is the real authority on whether
/// the address exists.
fn is_email(s: &str) -> bool {
    match s.rsplit_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && is_plain_hostname(domain)
                && !s.contains(['<', '>', '&', '"', '\'', ' '])
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_host_strips_scheme_port_path() {
        assert_eq!(
            server_host("https://mail.example.com:8080/x"),
            "mail.example.com"
        );
        assert_eq!(server_host("https://MAIL.example.com"), "mail.example.com");
        assert_eq!(server_host("mail.example.com"), "mail.example.com");
    }

    #[test]
    fn email_domain_prefers_valid_query_else_fallback() {
        assert_eq!(
            email_domain(Some("a@Sub.Example.com"), "fallback.eu"),
            "sub.example.com"
        );
        // No @, junk, or markup → fallback.
        assert_eq!(
            email_domain(Some("not-an-address"), "fallback.eu"),
            "fallback.eu"
        );
        assert_eq!(
            email_domain(Some("a@bad domain"), "fallback.eu"),
            "fallback.eu"
        );
        assert_eq!(
            email_domain(Some("a@<script>"), "fallback.eu"),
            "fallback.eu"
        );
        assert_eq!(email_domain(None, "fallback.eu"), "fallback.eu");
    }

    #[test]
    fn hostname_validation_rejects_injection() {
        assert!(is_plain_hostname("example.com"));
        assert!(is_plain_hostname("mail.sub.example.co.uk"));
        assert!(!is_plain_hostname("nodot"));
        assert!(!is_plain_hostname("bad<>.com"));
        assert!(!is_plain_hostname(""));
        assert!(!is_plain_hostname(".leading.dot"));
    }

    #[test]
    fn xml_escape_covers_all_five_entities() {
        assert_eq!(
            xml_escape(r#"<a href="x">&'</a>"#),
            "&lt;a href=&quot;x&quot;&gt;&amp;&apos;&lt;/a&gt;"
        );
    }

    #[test]
    fn extract_email_reads_the_element() {
        let body = b"<Autodiscover><Request><EMailAddress> user@d.eu </EMailAddress></Request></Autodiscover>";
        assert_eq!(extract_email(body).as_deref(), Some("user@d.eu"));
        assert_eq!(extract_email(b"<Request/>"), None);
    }

    #[test]
    fn is_email_rejects_markup_and_spaces() {
        assert!(is_email("user@example.com"));
        assert!(!is_email("user@nodot"));
        assert!(!is_email("a b@example.com"));
        assert!(!is_email("\"><x>@example.com"));
        assert!(!is_email("noat"));
    }
}
