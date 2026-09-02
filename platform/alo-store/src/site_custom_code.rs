//! The sandboxed custom-code block: what a tenant may write by hand, what the
//! browser is allowed to do with it, and what is refused on write.
//!
//! ADR 0036 ruled custom code out of the *first* wave; `docs/features.md`
//! carries it back at tier `[S+]` as **"custom-code blocks (sandboxed)"**, and
//! the two words matter equally. What the ADR forbids without a time limit —
//! third-party embeds and trackers of any kind — stays forbidden here: **no
//! capability in this module opens a network.** A block cannot fetch, cannot
//! load a remote script, font, or pixel, and cannot phone anything home; the
//! "no cookies, no banner" promise the analytics story rests on survives a
//! tenant pasting a snippet from the internet.
//!
//! The model is therefore a *document*, not a fragment:
//!
//! - the three parts ([`html`](CustomCodeSection::html),
//!   [`css`](CustomCodeSection::css), [`js`](CustomCodeSection::js)) are stored
//!   separately, so nothing is smuggled — a `<script>` inside the markup is a
//!   validation error, not a surprise;
//! - the [capabilities](CustomCodeCapabilities) are default-deny and explicit,
//!   and each one maps to exactly one sandbox token and one CSP directive;
//! - [`content_security_policy`](CustomCodeCapabilities::content_security_policy)
//!   and [`sandbox_attribute`](CustomCodeCapabilities::sandbox_attribute) are
//!   the contract the renderer must emit, computed here so the write gate and
//!   the published page cannot drift apart.
//!
//! **Where the boundary actually is.** The isolation is the browser's: an
//! `<iframe sandbox="…">` without `allow-same-origin`, holding a document whose
//! `Content-Security-Policy` starts at `default-src 'none'`. The frame gets an
//! opaque origin, so it cannot read the page around it, its cookies, or its
//! storage, whatever its script tries. The refusals in [`validate`] are not
//! that boundary — they are the *helpful error*: they catch the snippet that
//! would silently do nothing (a remote script CSP will block, a form that can
//! never submit) and the shapes that would break the wrapper document itself
//! (`</` inside an inlined block, a stray control byte). Security that depends
//! on string-matching hostile input is not security; this module says so out
//! loud rather than implying otherwise.

use serde::{Deserialize, Serialize};

use crate::site_model::{SectionSchemaError, check_opt_short, check_short};

/// The section's wire tag, used in every refusal this module raises.
const KIND: &str = "custom_code";

/// Byte cap on the block's markup. Bytes, not characters: the budget a visitor
/// pays is bytes on the wire, and the published-page budget
/// (`docs/design/sites.md`) is expressed the same way.
pub const MAX_CUSTOM_CODE_HTML_BYTES: usize = 16_384;
/// Byte cap on the block's stylesheet.
pub const MAX_CUSTOM_CODE_CSS_BYTES: usize = 8_192;
/// Byte cap on the block's script.
pub const MAX_CUSTOM_CODE_JS_BYTES: usize = 8_192;
/// Byte cap on the three parts together — below the sum of the individual caps
/// on purpose, so one page cannot carry fifty maximal blocks past the site's
/// own page budget.
pub const MAX_CUSTOM_CODE_TOTAL_BYTES: usize = 24_576;

/// Smallest frame height, in CSS pixels.
pub const MIN_CUSTOM_CODE_HEIGHT_PX: u16 = 40;
/// Largest frame height, in CSS pixels.
pub const MAX_CUSTOM_CODE_HEIGHT_PX: u16 = 2_000;

/// What the sandboxed frame is allowed to do. Every field is default-deny: an
/// absent capability is a denied capability, and a capability that is declared
/// without being used is refused on write (least privilege is checked, not
/// merely offered).
///
/// The set is deliberately small, and it is not a stepping stone to a network
/// capability — see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CustomCodeCapabilities {
    /// The block's [`js`](CustomCodeSection::js) runs. Off, the frame is inert
    /// markup and style: the sandbox withholds `allow-scripts` and the policy
    /// names no `script-src`, so even an inline handler that slipped past the
    /// write gate never executes.
    pub scripts: bool,
    /// `data:` images may be decoded inside the frame — an inline SVG sprite or
    /// a tiny PNG carried in the markup itself. Never a URL: there is nothing
    /// to fetch from.
    pub inline_images: bool,
}

impl CustomCodeCapabilities {
    /// The exact value of the frame's `sandbox` attribute. `allow-same-origin`
    /// is not reachable from any capability and never will be: with it the
    /// frame would share the site's origin and the isolation would be
    /// decorative. Neither is `allow-top-navigation`, `allow-popups`, or
    /// `allow-modals` — a block cannot move, cover, or interrupt the page it
    /// sits in.
    ///
    /// An empty string is a *fully* sandboxed frame, which is what a block with
    /// no capabilities gets — the attribute is still written, because omitting
    /// it means "no sandbox at all".
    pub fn sandbox_attribute(self) -> &'static str {
        if self.scripts { "allow-scripts" } else { "" }
    }

    /// The `Content-Security-Policy` the frame's document declares. Directive
    /// order is fixed so the value is a golden the renderer's tests can pin.
    ///
    /// `default-src 'none'` is the floor: no fetch, no XHR, no WebSocket, no
    /// remote font, no nested frame, no worker. `base-uri 'none'` stops a
    /// stored `<base>` from re-pointing relative URLs, and `form-action 'none'`
    /// means a form inside the block has nowhere to post — the contact form is
    /// its own section, with a server that knows what to do with it. Only the
    /// declared capabilities add anything back.
    pub fn content_security_policy(self) -> String {
        let mut policy = String::from(
            "default-src 'none'; base-uri 'none'; form-action 'none'; \
                          style-src 'unsafe-inline'",
        );
        if self.scripts {
            policy.push_str("; script-src 'unsafe-inline'");
        }
        if self.inline_images {
            policy.push_str("; img-src data:");
        }
        policy
    }
}

/// A block of the tenant's own HTML, CSS, and JavaScript, published inside a
/// sandboxed frame.
///
/// The stored value is the source, never a rendered document: the wrapper —
/// doctype, the policy, the `<style>` and `<script>` blocks — is assembled at
/// render time from these parts and [`capabilities`](Self::capabilities), so a
/// tightening of the contract reaches every already-published block the next
/// time it is served.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomCodeSection {
    /// Heading rendered by the *page*, outside the frame, in the site's own
    /// type — so a custom block still reads as part of the site.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// The frame's accessible name (its `title`). Required, because a frame
    /// without one is announced as "frame" and nothing else; a visitor using a
    /// screen reader has to be told what this is.
    pub title: String,
    /// The block's markup — the body of the frame's document. Structural tags
    /// (`<html>`, `<head>`, `<body>`), the wrapper's own blocks (`<script>`,
    /// `<style>`), and anything that loads or embeds (`<iframe>`, `<object>`,
    /// `<link>`, …) are refused on write.
    pub html: String,
    /// The block's stylesheet, inlined into the frame's `<style>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub css: Option<String>,
    /// The block's script, inlined into the frame's `<script>`. Requires the
    /// [`scripts`](CustomCodeCapabilities::scripts) capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub js: Option<String>,
    /// What the frame may do; absent means nothing beyond markup and style.
    #[serde(default, skip_serializing_if = "is_default_capabilities")]
    pub capabilities: CustomCodeCapabilities,
    /// The frame's height in CSS pixels. A sandboxed frame cannot be measured
    /// from the page (that would need the same origin, or a script on both
    /// sides), so the height is authored rather than discovered — an honest
    /// constraint of the isolation, not an oversight.
    pub height_px: u16,
    /// Page-owned presentation around the isolated frame. These choices never
    /// cross the sandbox boundary or alter the tenant's code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<crate::site_model::SectionPresentation>,
}

fn is_default_capabilities(capabilities: &CustomCodeCapabilities) -> bool {
    *capabilities == CustomCodeCapabilities::default()
}

impl CustomCodeSection {
    /// The total wire size of the three parts, in bytes.
    pub fn total_bytes(&self) -> usize {
        self.html.len()
            + self.css.as_ref().map_or(0, String::len)
            + self.js.as_ref().map_or(0, String::len)
    }

    /// Content-rule validation, run by the section schema's write gate.
    ///
    /// # Errors
    /// [`SectionSchemaError::Invalid`] naming the violated rule: a blank or
    /// over-cap part, a part that would break the wrapper document, a
    /// reference to another site, a script without the capability that runs it
    /// (or the capability without the script), or a height outside the frame
    /// bounds.
    pub(crate) fn validate(&self) -> Result<(), SectionSchemaError> {
        check_opt_short(KIND, "heading", self.heading.as_deref())?;
        check_short(KIND, "title", &self.title)?;

        check_part("html", &self.html, MAX_CUSTOM_CODE_HTML_BYTES)?;
        check_html(&self.html)?;
        if let Some(css) = &self.css {
            check_part("css", css, MAX_CUSTOM_CODE_CSS_BYTES)?;
            check_inlined(KIND, "css", css)?;
            if css.to_ascii_lowercase().contains("@import") {
                return Err(invalid(
                    "css may not @import another stylesheet — the block has no network access; \
                     paste the rules in",
                ));
            }
        }
        if let Some(js) = &self.js {
            check_part("js", js, MAX_CUSTOM_CODE_JS_BYTES)?;
            check_inlined(KIND, "js", js)?;
        }

        match (self.js.is_some(), self.capabilities.scripts) {
            (true, false) => {
                return Err(invalid(
                    "a block with a script must declare the scripts capability",
                ));
            }
            (false, true) => {
                return Err(invalid(
                    "the scripts capability may only be declared by a block that has a script",
                ));
            }
            _ => {}
        }

        let total = self.total_bytes();
        if total > MAX_CUSTOM_CODE_TOTAL_BYTES {
            return Err(invalid(&format!(
                "html, css, and js together must be at most {MAX_CUSTOM_CODE_TOTAL_BYTES} bytes \
                 (this block is {total})"
            )));
        }

        if self.height_px < MIN_CUSTOM_CODE_HEIGHT_PX || self.height_px > MAX_CUSTOM_CODE_HEIGHT_PX
        {
            return Err(invalid(&format!(
                "height must be between {MIN_CUSTOM_CODE_HEIGHT_PX} and \
                 {MAX_CUSTOM_CODE_HEIGHT_PX} pixels"
            )));
        }
        Ok(())
    }
}

fn invalid(detail: &str) -> SectionSchemaError {
    SectionSchemaError::Invalid {
        section: KIND,
        detail: detail.to_owned(),
    }
}

/// The rules every part shares: present, bounded, free of control bytes, and
/// naming no other site.
fn check_part(field: &str, value: &str, max_bytes: usize) -> Result<(), SectionSchemaError> {
    if value.trim().is_empty() {
        return Err(invalid(&format!("{field} must not be blank")));
    }
    if value.len() > max_bytes {
        return Err(invalid(&format!(
            "{field} must be at most {max_bytes} bytes (this one is {})",
            value.len()
        )));
    }
    // Tab, newline, and carriage return are the only control characters source
    // code needs. Anything else — a NUL, a form feed, a bidi override — exists
    // to make two parsers read the same bytes differently.
    if let Some(c) = value
        .chars()
        .find(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
    {
        return Err(invalid(&format!(
            "{field} contains a control character (U+{:04X}) that source code does not need",
            c as u32
        )));
    }
    let lower = value.to_ascii_lowercase();
    if lower.contains("://") {
        return Err(invalid(&format!(
            "{field} may not reference another address — the block runs with no network access, \
             so anything it loads from elsewhere would silently fail to appear"
        )));
    }
    for scheme in ["javascript:", "vbscript:"] {
        if lower.contains(scheme) {
            return Err(invalid(&format!(
                "{field} may not use a {scheme} URL; put behaviour in the block's script"
            )));
        }
    }
    Ok(())
}

/// The markup's own rules: it is the *body* of a document this renderer
/// assembles, so it may not declare the document, re-open the wrapper's own
/// blocks, or embed anything.
fn check_html(html: &str) -> Result<(), SectionSchemaError> {
    let lower = html.to_ascii_lowercase();
    const FORBIDDEN: &[(&str, &str)] = &[
        (
            "<script",
            "put JavaScript in the block's script field, not in its markup",
        ),
        (
            "<style",
            "put CSS in the block's style field, not in its markup",
        ),
        (
            "<html",
            "the markup is the body of the block; the document around it is written for you",
        ),
        (
            "<head",
            "the markup is the body of the block; the document around it is written for you",
        ),
        (
            "<body",
            "the markup is the body of the block; the document around it is written for you",
        ),
        (
            "<base",
            "a block may not re-point where its own links resolve",
        ),
        (
            "<meta",
            "a block may not declare document metadata; its policy is written for you",
        ),
        (
            "<link",
            "a block may not link to a resource — it has no network access",
        ),
        (
            "<iframe",
            "a block may not frame anything; it is already inside a frame",
        ),
        (
            "<frame",
            "a block may not frame anything; it is already inside a frame",
        ),
        (
            "<portal",
            "a block may not frame anything; it is already inside a frame",
        ),
        (
            "<object",
            "a block may not embed a plugin or an external document",
        ),
        (
            "<embed",
            "a block may not embed a plugin or an external document",
        ),
        (
            "<applet",
            "a block may not embed a plugin or an external document",
        ),
        (
            "<form",
            "a block's form has nowhere to send what it collects; use a contact form section",
        ),
    ];
    for (tag, why) in FORBIDDEN {
        if lower.contains(tag) {
            return Err(invalid(&format!("html may not contain {tag}>: {why}")));
        }
    }
    Ok(())
}

/// A part inlined verbatim into a `<style>` or `<script>` block may never
/// contain `</`, which would close that block early and let the rest of the
/// value be read as markup. The generated site stylesheet holds the same rule
/// for the same reason (`docs/design/sites.md`).
fn check_inlined(
    section: &'static str,
    field: &str,
    value: &str,
) -> Result<(), SectionSchemaError> {
    if value.contains("</") {
        return Err(SectionSchemaError::Invalid {
            section,
            detail: format!(
                "{field} may not contain the characters `</`, which would end the block it is \
                 written into (in a script, write `<\\/` instead)"
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn block() -> CustomCodeSection {
        CustomCodeSection {
            heading: Some("Roast timer".to_owned()),
            title: "Roast timer".to_owned(),
            html: "<div id=\"t\">0:00</div><button type=\"button\">Start</button>".to_owned(),
            css: Some("#t { font-size: 3rem; }".to_owned()),
            js: Some("document.querySelector('button').onclick = () => {};".to_owned()),
            capabilities: CustomCodeCapabilities {
                scripts: true,
                inline_images: false,
            },
            height_px: 240,
            presentation: None,
        }
    }

    fn refusal(section: &CustomCodeSection) -> String {
        section
            .validate()
            .expect_err("this block must be refused")
            .to_string()
    }

    #[test]
    fn a_complete_block_is_accepted() {
        block().validate().unwrap();
    }

    #[test]
    fn a_block_with_neither_script_nor_capability_is_accepted() {
        let mut section = block();
        section.js = None;
        section.capabilities = CustomCodeCapabilities::default();
        section.validate().unwrap();
    }

    #[test]
    fn the_policy_starts_closed_and_opens_only_what_is_declared() {
        let none = CustomCodeCapabilities::default();
        assert_eq!(
            none.content_security_policy(),
            "default-src 'none'; base-uri 'none'; form-action 'none'; style-src 'unsafe-inline'"
        );
        assert_eq!(none.sandbox_attribute(), "");

        let scripts = CustomCodeCapabilities {
            scripts: true,
            inline_images: false,
        };
        assert_eq!(
            scripts.content_security_policy(),
            "default-src 'none'; base-uri 'none'; form-action 'none'; style-src 'unsafe-inline'; \
             script-src 'unsafe-inline'"
        );
        assert_eq!(scripts.sandbox_attribute(), "allow-scripts");

        let both = CustomCodeCapabilities {
            scripts: true,
            inline_images: true,
        };
        assert_eq!(
            both.content_security_policy(),
            "default-src 'none'; base-uri 'none'; form-action 'none'; style-src 'unsafe-inline'; \
             script-src 'unsafe-inline'; img-src data:"
        );
    }

    /// No capability, in any combination, ever hands the frame its own origin
    /// back or lets it reach the network — the two properties the isolation
    /// rests on. Exhaustive over the capability space.
    #[test]
    fn no_capability_combination_escapes_the_frame_or_reaches_the_network() {
        for scripts in [false, true] {
            for inline_images in [false, true] {
                let capabilities = CustomCodeCapabilities {
                    scripts,
                    inline_images,
                };
                let sandbox = capabilities.sandbox_attribute();
                for token in [
                    "allow-same-origin",
                    "allow-top-navigation",
                    "allow-popups",
                    "allow-modals",
                    "allow-downloads",
                    "allow-presentation",
                ] {
                    assert!(
                        !sandbox.contains(token),
                        "{capabilities:?} handed the frame {token}"
                    );
                }
                let policy = capabilities.content_security_policy();
                assert!(policy.starts_with("default-src 'none';"), "{policy}");
                for reachable in ["http", "*", "'self'", "connect-src", "frame-src"] {
                    assert!(
                        !policy.contains(reachable),
                        "{capabilities:?} let the frame reach {reachable}: {policy}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_script_and_its_capability_travel_together() {
        let mut without_capability = block();
        without_capability.capabilities.scripts = false;
        assert!(
            refusal(&without_capability).contains("must declare the scripts capability"),
            "{}",
            refusal(&without_capability)
        );

        let mut without_script = block();
        without_script.js = None;
        assert!(
            refusal(&without_script).contains("only be declared by a block that has a script"),
            "{}",
            refusal(&without_script)
        );
    }

    #[test]
    fn markup_may_not_declare_a_document_or_embed_anything() {
        for (markup, expected) in [
            ("<script>alert(1)</script>", "<script>"),
            ("<STYLE>body{}</STYLE>", "<style>"),
            ("<html><body>hi</body></html>", "<html>"),
            ("<base href=\"/elsewhere\">", "<base>"),
            ("<meta http-equiv=\"refresh\" content=\"0\">", "<meta>"),
            ("<link rel=\"stylesheet\" href=\"/x.css\">", "<link>"),
            ("<iframe srcdoc=\"&lt;b&gt;\"></iframe>", "<iframe>"),
            ("<object data=\"x.pdf\"></object>", "<object>"),
            ("<form><input name=\"email\"></form>", "<form>"),
        ] {
            let mut section = block();
            section.html = markup.to_owned();
            let detail = refusal(&section);
            assert!(
                detail.contains(expected),
                "{markup:?} was refused as {detail:?}, expected the {expected} rule"
            );
        }
    }

    #[test]
    fn no_part_may_name_another_address_or_a_scriptable_scheme() {
        let mut remote_script = block();
        remote_script.js = Some("fetch('https://tracker.example/p')".to_owned());
        assert!(refusal(&remote_script).contains("no network access"));

        let mut remote_font = block();
        remote_font.css = Some("@font-face { src: url(https://f.example/a.woff2); }".to_owned());
        assert!(refusal(&remote_font).contains("no network access"));

        let mut imported = block();
        imported.css = Some("@import 'theme.css'; body { color: red; }".to_owned());
        assert!(refusal(&imported).contains("@import"));

        let mut scheme = block();
        scheme.html = "<a href=\"javascript:alert(1)\">go</a>".to_owned();
        assert!(refusal(&scheme).contains("javascript:"));
    }

    /// The wrapper inlines css and js verbatim; `</` in either would end the
    /// block early and turn the remainder into markup of the frame's document.
    #[test]
    fn an_inlined_part_may_not_close_its_own_block() {
        let mut closing = block();
        closing.js = Some("const end = '</script>';".to_owned());
        assert!(refusal(&closing).contains("`</`"));

        let mut styles = block();
        styles.css = Some("body { content: '</style>'; }".to_owned());
        assert!(refusal(&styles).contains("`</`"));
    }

    #[test]
    fn parts_are_bounded_together_and_apart() {
        let mut wide = block();
        wide.html = format!("<p>{}</p>", "a".repeat(MAX_CUSTOM_CODE_HTML_BYTES));
        assert!(refusal(&wide).contains("at most 16384 bytes"));

        let mut total = block();
        total.html = "<p>".to_owned() + &"a".repeat(MAX_CUSTOM_CODE_HTML_BYTES - 3);
        total.css = Some("a".repeat(MAX_CUSTOM_CODE_CSS_BYTES));
        total.js = Some("a".repeat(MAX_CUSTOM_CODE_JS_BYTES));
        assert_eq!(
            total.total_bytes(),
            MAX_CUSTOM_CODE_HTML_BYTES + MAX_CUSTOM_CODE_CSS_BYTES + MAX_CUSTOM_CODE_JS_BYTES
        );
        assert!(refusal(&total).contains("together must be at most 24576 bytes"));
    }

    #[test]
    fn a_control_byte_is_refused_by_name() {
        let mut nul = block();
        nul.html = "<p>hi\u{0}</p>".to_owned();
        assert!(refusal(&nul).contains("U+0000"), "{}", refusal(&nul));

        let mut newlines = block();
        newlines.html = "<p>\n\thi\r\n</p>".to_owned();
        newlines.validate().unwrap();
    }

    #[test]
    fn the_frame_needs_a_name_and_a_height_that_fits_a_page() {
        let mut unnamed = block();
        unnamed.title = "   ".to_owned();
        assert!(refusal(&unnamed).contains("title must not be blank"));

        for height in [
            0,
            MIN_CUSTOM_CODE_HEIGHT_PX - 1,
            MAX_CUSTOM_CODE_HEIGHT_PX + 1,
        ] {
            let mut section = block();
            section.height_px = height;
            assert!(
                refusal(&section).contains("height must be between 40 and 2000 pixels"),
                "height {height} was not refused"
            );
        }
    }

    /// An absent `capabilities` object means "nothing allowed", and a block
    /// that declares none serializes without the key at all — the stored JSON
    /// stays the shape the editor sent.
    #[test]
    fn capabilities_default_to_denied_and_stay_out_of_the_stored_json() {
        let parsed: CustomCodeSection =
            serde_json::from_str(r#"{"title":"Chart","html":"<p>x</p>","height_px":200}"#).unwrap();
        assert_eq!(parsed.capabilities, CustomCodeCapabilities::default());
        assert!(!parsed.capabilities.scripts);
        parsed.validate().unwrap();

        let back = serde_json::to_string(&parsed).unwrap();
        assert!(!back.contains("capabilities"), "{back}");
    }

    /// The editor states every capability, including the denied ones, rather
    /// than leaving them out (`web/src/sites/CustomCodeFields.tsx`): a switch
    /// a person turned off should reach the server as "off", not as silence.
    /// Both spellings mean the same thing here, and neither may become a
    /// refusal — the switches are the only way a block is authored.
    #[test]
    fn the_editors_explicit_denials_mean_the_same_as_saying_nothing() {
        let parsed: CustomCodeSection = serde_json::from_str(
            r#"{"title":"Opening hours","html":"<p>Open until six</p>",
                "css":"p { font-weight: 700; }",
                "capabilities":{"scripts":false,"inline_images":false},
                "height_px":180}"#,
        )
        .unwrap();
        assert_eq!(parsed.capabilities, CustomCodeCapabilities::default());
        parsed.validate().unwrap();
        assert_eq!(parsed.capabilities.sandbox_attribute(), "");
    }

    #[test]
    fn an_unknown_capability_is_refused_rather_than_ignored() {
        let error = serde_json::from_str::<CustomCodeSection>(
            r#"{"title":"Chart","html":"<p>x</p>","height_px":200,
                "capabilities":{"scripts":true,"network":true}}"#,
        )
        .expect_err("an unknown capability must not be silently dropped");
        assert!(error.to_string().contains("network"), "{error}");
    }
}
