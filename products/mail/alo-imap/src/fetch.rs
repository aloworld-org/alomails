//! FETCH data items: header/body splitting (byte-exact), ENVELOPE
//! (RFC 9051 §7.5.2 / RFC 5322 addresses), a bounded recursive MIME walk
//! for BODYSTRUCTURE and numbered `BODY[part]` addressing, and body-section
//! extraction with `<offset.count>` partials. Byte sections are exact
//! slices of the stored message — never re-rendered (clients may hash
//! them). BODYSTRUCTURE fidelity is bounded and honest (see the design
//! note): malformed MIME past the bound degrades to a single text part.

/// Depth/part ceilings for the MIME walk — bound work on hostile input.
const MAX_DEPTH: usize = 16;
const MAX_PARTS: usize = 256;

/// Splits raw message bytes into header and body at the first empty line
/// (CRLFCRLF, tolerating LFLF). Returns `(header, body, body_start)` where
/// `header` excludes the blank-line terminator and `body_start` is the
/// offset of the body in `raw`.
pub fn split_header_body(raw: &[u8]) -> (&[u8], &[u8], usize) {
    // Look for CRLFCRLF first, then LFLF (lenient inbound).
    if let Some(i) = find(raw, b"\r\n\r\n") {
        return (&raw[..i], &raw[i + 4..], i + 4);
    }
    if let Some(i) = find(raw, b"\n\n") {
        return (&raw[..i], &raw[i + 2..], i + 2);
    }
    (raw, &[], raw.len())
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// A parsed header field: original-case name and unfolded value.
pub struct Field {
    /// Header name.
    pub name: String,
    /// Unfolded value (folding whitespace collapsed).
    pub value: String,
}

/// Parses a header block into ordered fields (unfolded values).
pub fn parse_fields(header: &[u8]) -> Vec<Field> {
    let text = String::from_utf8_lossy(header);
    let mut fields: Vec<Field> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation of the previous field.
            if let Some(last) = fields.last_mut() {
                last.value.push(' ');
                last.value.push_str(line.trim());
            }
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            fields.push(Field {
                name: name.trim().to_owned(),
                value: value.trim().to_owned(),
            });
        }
    }
    fields
}

fn field<'a>(fields: &'a [Field], name: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case(name))
        .map(|f| f.value.as_str())
}

// ---- ENVELOPE -------------------------------------------------------------

/// Renders the IMAP ENVELOPE for a message's header block.
pub fn envelope(header: &[u8]) -> String {
    let fields = parse_fields(header);
    let date = nstring(field(&fields, "Date"));
    let subject = nstring(field(&fields, "Subject"));
    let from = addr_list(field(&fields, "From"));
    let sender = addr_list_or(field(&fields, "Sender"), field(&fields, "From"));
    let reply_to = addr_list_or(field(&fields, "Reply-To"), field(&fields, "From"));
    let to = addr_list(field(&fields, "To"));
    let cc = addr_list(field(&fields, "Cc"));
    let bcc = addr_list(field(&fields, "Bcc"));
    let in_reply_to = nstring(field(&fields, "In-Reply-To"));
    let message_id = nstring(field(&fields, "Message-ID"));
    format!(
        "({date} {subject} {from} {sender} {reply_to} {to} {cc} {bcc} {in_reply_to} {message_id})"
    )
}

/// An IMAP quoted string, or NIL for `None`/empty.
fn nstring(v: Option<&str>) -> String {
    match v {
        Some(s) if !s.is_empty() => quote(s),
        _ => "NIL".to_owned(),
    }
}

/// Double-quotes a string with IMAP escaping. Control characters (which a
/// quoted string may not carry, RFC 9051 §4.3) are dropped defensively so a
/// CR/LF can never splice extra response lines.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c.is_control() {
            continue;
        }
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn addr_list_or(primary: Option<&str>, fallback: Option<&str>) -> String {
    match primary {
        Some(s) if !s.is_empty() => addr_list(Some(s)),
        _ => addr_list(fallback),
    }
}

/// Renders an address-list header value as an IMAP parenthesized address
/// list, or NIL. Each address is `(name adl mailbox host)`.
fn addr_list(v: Option<&str>) -> String {
    let Some(v) = v.filter(|s| !s.is_empty()) else {
        return "NIL".to_owned();
    };
    let addrs = parse_addresses(v);
    if addrs.is_empty() {
        return "NIL".to_owned();
    }
    let mut out = String::from("(");
    for a in addrs {
        out.push_str(&a);
    }
    out.push(')');
    out
}

/// Parses an RFC 5322 address list into IMAP `(name NIL mailbox host)`
/// address structures. Handles `Display Name <local@domain>`, bare
/// `local@domain`, and comma separation; quoted display names supported.
fn parse_addresses(value: &str) -> Vec<String> {
    let mut result = Vec::new();
    for part in split_addresses(value) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, addr) = if let (Some(lt), Some(gt)) = (part.rfind('<'), part.rfind('>')) {
            if lt < gt {
                (part[..lt].trim(), part[lt + 1..gt].trim())
            } else {
                ("", part)
            }
        } else {
            ("", part)
        };
        let name = name.trim_matches('"').trim();
        let (mailbox, host) = match addr.rsplit_once('@') {
            Some((m, h)) => (m, h),
            None => (addr, ""),
        };
        result.push(format!(
            "({} NIL {} {})",
            nstring((!name.is_empty()).then_some(name)),
            nstring((!mailbox.is_empty()).then_some(mailbox)),
            nstring((!host.is_empty()).then_some(host)),
        ));
    }
    result
}

/// Splits an address-list on commas that are not inside quotes or angle
/// brackets.
fn split_addresses(value: &str) -> Vec<String> {
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
            ',' if !in_quote && !in_angle => {
                parts.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        parts.push(cur);
    }
    parts
}

// ---- MIME structure -------------------------------------------------------

/// A node in the MIME tree, with byte ranges into the full message.
struct MimePart {
    ctype: String,
    subtype: String,
    params: Vec<(String, String)>,
    encoding: String,
    /// `(start, end)` of this part's own header block in the full message.
    header: (usize, usize),
    /// `(start, end)` of this part's content bytes in the full message.
    body: (usize, usize),
    children: Vec<MimePart>,
}

/// Parses the MIME tree of `raw` (the whole message occupies
/// `[start,end)`), bounded by depth/part budgets.
fn parse_mime(raw: &[u8], start: usize, end: usize, depth: usize, budget: &mut usize) -> MimePart {
    let slice = &raw[start..end];
    let (hdr, _body, body_off) = split_header_body(slice);
    let body_start = start + body_off;
    let fields = parse_fields(hdr);
    let ctype_raw = field(&fields, "Content-Type").unwrap_or("text/plain");
    let (mime, params) = parse_content_type(ctype_raw);
    let (ctype, subtype) = mime
        .split_once('/')
        .map(|(a, b)| (a.to_owned(), b.to_owned()))
        .unwrap_or_else(|| ("text".to_owned(), "plain".to_owned()));
    let encoding = field(&fields, "Content-Transfer-Encoding")
        .unwrap_or("7bit")
        .to_owned();

    let mut part = MimePart {
        ctype: ctype.clone(),
        subtype,
        params,
        encoding,
        header: (start, body_start),
        body: (body_start, end),
        children: Vec::new(),
    };

    if ctype.eq_ignore_ascii_case("multipart")
        && depth < MAX_DEPTH
        && let Some(boundary) = part
            .params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("boundary"))
    {
        let boundary = boundary.1.clone();
        for (cs, ce) in split_multipart(raw, body_start, end, &boundary) {
            if *budget == 0 {
                break;
            }
            *budget -= 1;
            part.children
                .push(parse_mime(raw, cs, ce, depth + 1, budget));
        }
    }
    part
}

/// Splits a multipart body into child part byte ranges by boundary.
fn split_multipart(raw: &[u8], start: usize, end: usize, boundary: &str) -> Vec<(usize, usize)> {
    let delim = format!("--{boundary}");
    let region = &raw[start..end];
    let mut bounds: Vec<usize> = Vec::new();
    let mut i = 0;
    while let Some(rel) = find(&region[i..], delim.as_bytes()) {
        let at = i + rel;
        // A boundary must be at line start (offset 0 or preceded by \n).
        if at == 0 || region.get(at.wrapping_sub(1)) == Some(&b'\n') {
            bounds.push(start + at);
        }
        i = at + delim.len();
        if i >= region.len() {
            break;
        }
    }
    let mut parts = Vec::new();
    for w in bounds.windows(2) {
        // Child content starts after the boundary line's CRLF.
        let after = w[0] + delim.len();
        let content_start = skip_line_end(raw, after, end);
        // Child ends just before the next boundary's leading CRLF.
        let mut child_end = w[1];
        while child_end > content_start
            && (raw[child_end - 1] == b'\n' || raw[child_end - 1] == b'\r')
        {
            child_end -= 1;
        }
        if content_start <= child_end {
            parts.push((content_start, child_end));
        }
    }
    parts
}

fn skip_line_end(raw: &[u8], mut i: usize, end: usize) -> usize {
    // Skip an optional "--" (close delim) then the CRLF/LF.
    while i < end && raw[i] != b'\n' {
        i += 1;
    }
    if i < end {
        i += 1; // past the \n
    }
    i
}

/// Parses a Content-Type value into `(mime, params)`.
fn parse_content_type(value: &str) -> (String, Vec<(String, String)>) {
    let mut parts = value.split(';');
    let mime = parts.next().unwrap_or("text/plain").trim().to_owned();
    let mut params = Vec::new();
    for p in parts {
        if let Some((k, v)) = p.split_once('=') {
            let v = v.trim().trim_matches('"');
            params.push((k.trim().to_owned(), v.to_owned()));
        }
    }
    (mime, params)
}

/// Renders BODYSTRUCTURE (or the shorter BODY, `extended=false`) for a
/// message.
pub fn body_structure(raw: &[u8], extended: bool) -> String {
    let mut budget = MAX_PARTS;
    let part = parse_mime(raw, 0, raw.len(), 0, &mut budget);
    render_part(raw, &part, extended)
}

fn render_part(raw: &[u8], part: &MimePart, extended: bool) -> String {
    if !part.children.is_empty() {
        let mut inner = String::new();
        for c in &part.children {
            inner.push_str(&render_part(raw, c, extended));
        }
        let sub = quote(&part.subtype.to_uppercase());
        if extended {
            let params = render_params(&part.params);
            format!("({inner} {sub} {params} NIL NIL NIL)")
        } else {
            format!("({inner} {sub})")
        }
    } else {
        render_leaf(raw, part, extended)
    }
}

fn render_leaf(raw: &[u8], part: &MimePart, extended: bool) -> String {
    let content = &raw[part.body.0..part.body.1.min(raw.len())];
    let octets = content.len();
    let ty = quote(&part.ctype.to_uppercase());
    let sub = quote(&part.subtype.to_uppercase());
    let params = render_params(&part.params);
    let enc = quote(&part.encoding.to_uppercase());
    let mut base = format!("({ty} {sub} {params} NIL NIL {enc} {octets}");
    if part.ctype.eq_ignore_ascii_case("text") {
        let lines = content.iter().filter(|&&b| b == b'\n').count();
        base.push_str(&format!(" {lines}"));
    }
    if extended {
        base.push_str(" NIL NIL NIL NIL");
    }
    base.push(')');
    base
}

fn render_params(params: &[(String, String)]) -> String {
    if params.is_empty() {
        return "NIL".to_owned();
    }
    let mut out = String::from("(");
    for (i, (k, v)) in params.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&quote(&k.to_lowercase()));
        out.push(' ');
        out.push_str(&quote(v));
    }
    out.push(')');
    out
}

// ---- body sections --------------------------------------------------------

/// A parsed body-section spec from `BODY[...]`.
#[derive(Debug, Clone)]
pub enum Section {
    /// `BODY[]` — the whole message.
    Full,
    /// `BODY[HEADER]`.
    Header,
    /// `BODY[HEADER.FIELDS (a b)]`.
    HeaderFields(Vec<String>),
    /// `BODY[HEADER.FIELDS.NOT (a b)]`.
    HeaderFieldsNot(Vec<String>),
    /// `BODY[TEXT]`.
    Text,
    /// `BODY[n(.n)*]` — a numbered part's content.
    Part(Vec<usize>),
    /// `BODY[n(.n)*.MIME]` — a numbered part's MIME header.
    PartMime(Vec<usize>),
    /// `BODY[n(.n)*.HEADER]`.
    PartHeader(Vec<usize>),
    /// `BODY[n(.n)*.TEXT]`.
    PartText(Vec<usize>),
}

/// Extracts the bytes for a body section from the raw message. Returns
/// `None` if a numbered part does not exist.
pub fn section_bytes(raw: &[u8], section: &Section) -> Option<Vec<u8>> {
    match section {
        Section::Full => Some(raw.to_vec()),
        Section::Header => {
            // HEADER includes the blank line that ends the header block.
            let (_, _, start) = split_header_body(raw);
            Some(raw[..start.min(raw.len())].to_vec())
        }
        Section::Text => {
            let (_, b, _) = split_header_body(raw);
            Some(b.to_vec())
        }
        Section::HeaderFields(names) => Some(header_fields_bytes(raw, names, false)),
        Section::HeaderFieldsNot(names) => Some(header_fields_bytes(raw, names, true)),
        Section::Part(path) => part_range(raw, path).map(|p| raw[p.body.0..p.body.1].to_vec()),
        Section::PartMime(path) => {
            part_range(raw, path).map(|p| raw[p.header.0..p.header.1].to_vec())
        }
        Section::PartHeader(path) => part_range(raw, path).map(|p| {
            let (_, _, start) = split_header_body(&raw[p.header.0..p.body.1]);
            raw[p.header.0..p.header.0 + start].to_vec()
        }),
        Section::PartText(path) => part_range(raw, path).map(|p| raw[p.body.0..p.body.1].to_vec()),
    }
}

/// Byte ranges of a numbered MIME part.
struct PartRange {
    header: (usize, usize),
    body: (usize, usize),
}

fn part_range(raw: &[u8], path: &[usize]) -> Option<PartRange> {
    let mut budget = MAX_PARTS;
    let root = parse_mime(raw, 0, raw.len(), 0, &mut budget);
    let mut node = &root;
    for &n in path {
        if n == 0 {
            return None;
        }
        node = node.children.get(n - 1)?;
    }
    Some(PartRange {
        header: node.header,
        body: node.body,
    })
}

/// Serializes selected (or excluded) header fields, terminated by a blank
/// line, as IMAP returns for `HEADER.FIELDS`.
fn header_fields_bytes(raw: &[u8], names: &[String], exclude: bool) -> Vec<u8> {
    let (header, _, _) = split_header_body(raw);
    let wanted: Vec<Vec<u8>> = names
        .iter()
        .map(|n| n.to_ascii_lowercase().into_bytes())
        .collect();
    let mut out = Vec::new();
    let mut keep = false;
    let mut start = 0usize;
    let mut i = 0usize;
    // Walk the raw header line by line, keeping **exact byte slices** of the
    // kept lines (clients may hash HEADER.FIELDS) — never a lossy round-trip.
    while i <= header.len() {
        let at_end = i == header.len();
        if at_end || header[i] == b'\n' {
            let line_end = if at_end { header.len() } else { i + 1 };
            if line_end > start {
                let line = &header[start..line_end];
                let is_cont = matches!(line.first(), Some(b' ') | Some(b'\t'));
                if !is_cont {
                    let name_end = line.iter().position(|&b| b == b':').unwrap_or(line.len());
                    let name: Vec<u8> = line[..name_end]
                        .iter()
                        .filter(|b| !b.is_ascii_whitespace())
                        .map(u8::to_ascii_lowercase)
                        .collect();
                    let listed = wanted.iter().any(|w| w.as_slice() == name.as_slice());
                    keep = if exclude { !listed } else { listed };
                }
                if keep {
                    out.extend_from_slice(line);
                }
            }
            start = line_end;
            if at_end {
                break;
            }
        }
        i += 1;
    }
    // Terminate with the blank line that ends a header section.
    if !out.ends_with(b"\r\n") {
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"\r\n");
    out
}

/// Applies an optional `<offset.count>` partial to section bytes.
pub fn apply_partial(bytes: Vec<u8>, partial: Option<(usize, usize)>) -> Vec<u8> {
    match partial {
        None => bytes,
        Some((off, count)) => {
            if off >= bytes.len() {
                Vec::new()
            } else {
                let end = off.saturating_add(count).min(bytes.len());
                bytes[off..end].to_vec()
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const MSG: &[u8] = b"From: Alice <alice@example.com>\r\n\
To: Bob <bob@example.org>, carol@example.net\r\n\
Subject: Hi\r\n\
Message-ID: <m1@example.com>\r\n\
\r\n\
Hello body\r\n";

    #[test]
    fn header_body_split_is_byte_exact() {
        let (h, b, start) = split_header_body(MSG);
        assert!(h.starts_with(b"From:"));
        assert_eq!(b, b"Hello body\r\n");
        assert_eq!(&MSG[start..], b"Hello body\r\n");
    }

    #[test]
    fn envelope_parses_addresses() {
        let (h, _, _) = split_header_body(MSG);
        let env = envelope(h);
        assert!(env.contains("\"Hi\""));
        assert!(env.contains("\"alice\" \"example.com\""));
        assert!(env.contains("\"bob\" \"example.org\""));
        assert!(env.contains("\"carol\" \"example.net\""));
        assert!(env.contains("\"<m1@example.com>\""));
    }

    #[test]
    fn single_part_bodystructure() {
        let bs = body_structure(MSG, false);
        assert!(bs.starts_with("(\"TEXT\" \"PLAIN\""));
    }

    #[test]
    fn multipart_bodystructure_decomposes() {
        let mp = b"Content-Type: multipart/alternative; boundary=BB\r\n\r\n\
--BB\r\nContent-Type: text/plain\r\n\r\nplain\r\n\
--BB\r\nContent-Type: text/html\r\n\r\n<p>html</p>\r\n\
--BB--\r\n";
        let bs = body_structure(mp, false);
        assert!(bs.contains("\"PLAIN\""));
        assert!(bs.contains("\"HTML\""));
        assert!(bs.trim_end().ends_with("\"ALTERNATIVE\")"));
        // BODY[1] is the first part's content.
        let p1 = section_bytes(mp, &Section::Part(vec![1])).unwrap();
        assert_eq!(p1, b"plain");
        let p2 = section_bytes(mp, &Section::Part(vec![2])).unwrap();
        assert_eq!(p2, b"<p>html</p>");
    }

    #[test]
    fn header_fields_selection() {
        let b = header_fields_bytes(MSG, &["subject".to_owned()], false);
        let s = String::from_utf8(b).unwrap();
        assert!(s.starts_with("Subject: Hi\r\n"));
        assert!(!s.contains("From:"));
        assert!(s.ends_with("\r\n\r\n"));
    }
}
