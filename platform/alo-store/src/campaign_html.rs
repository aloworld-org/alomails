//! Campaign blocks compiled to email-safe HTML (alo Campaigns, ADR 0044, wave
//! C3.2).
//!
//! Queue item C3.2: *blocks → email-safe HTML, table layout and inline CSS,
//! because Outlook renders through Word. **A compiler, not a stylesheet.***
//! That last sentence is the whole design. A web page ships one stylesheet and
//! lets the browser apply it; a mail client is not a browser. Outlook on
//! Windows draws through Word's HTML engine, Gmail rewrites the document it is
//! given, and several clients drop `<head>` entirely — so a rule that is not on
//! the element it applies to is a rule that may not arrive. This module
//! therefore emits **no `<link>` and no `class` attribute at all, and nothing
//! a client can drop that the letter needs**: every declaration sits in the
//! `style=` of the tag it governs, beside the presentational HTML attribute
//! (`width`, `align`, `bgcolor`, `cellpadding`) that Word honours when it
//! ignores the CSS. The single `<style>` element the document does carry holds
//! only the dark-mode block, which can add nothing and can only swap a colour
//! already declared inline — see the next section.
//! [`tests::the_only_stylesheet_is_the_dark_block_and_the_letter_is_whole_without_it`]
//! holds the promise to the output rather than to this paragraph.
//!
//! ## The one stylesheet, and why it is not a contradiction
//!
//! Wave C3.5 — *the mail must read with images blocked: alt text on every
//! image, colour never the only carrier of meaning, and a dark-mode-safe
//! palette* — adds the single exception to the paragraph above: a `<style>`
//! element holding **one** `@media (prefers-color-scheme:dark)` block and
//! nothing else. `prefers-color-scheme` is a media query and a media query
//! cannot live in a `style=` attribute, so the choice was a stylesheet or no
//! dark mode at all.
//!
//! It is an exception rather than a retreat because **the letter is complete
//! without it**. Every element still declares its light colours on itself, no
//! `class` and no `<link>` appears, and a client that drops the head draws
//! exactly the document it drew before this wave — the dark block only ever
//! *replaces* a colour that is already there.
//! [`tests::the_only_stylesheet_is_the_dark_block_and_the_letter_is_whole_without_it`]
//! holds both halves of that.
//!
//! The block is generated from [`LETTER_PALETTE`], which is the same table the
//! renderer draws the light letter from, so the two cannot drift: each rule
//! selects the elements that *declare* a light colour and swaps in its dark
//! twin. No hook attribute is added to the markup for the stylesheet to aim at
//! — the declaration itself is the hook, which is why a new element using
//! `PAGE_BACKGROUND` is dark-mode-correct the moment it is written.
//!
//! ## The rest of C3.5: images and colour
//!
//! - **Images blocked is not a degraded reading of this letter, it is the same
//!   one.** The model has no image block, so the document contains no `<img>`,
//!   no `background-image` and no remote URL of any kind — there is nothing for
//!   a client to block. That is a stronger promise than alt text and it is
//!   pinned by
//!   [`tests::a_letter_reads_the_same_with_images_blocked_because_it_has_none`].
//!   Whether an image block should exist at all is the open question C3.1 left
//!   and this wave did not answer: a remote image is a fetch we can see, which
//!   is the open-tracking pixel ADR 0044 refused by default. It needs an ADR
//!   about who may fetch and what is logged, and mandatory alt text is what
//!   that ADR's block would carry.
//! - **Nothing in the letter is told by colour alone.** The header row of a
//!   table is `<th scope="col">` and bold before it is tinted; a code sample is
//!   monospace, framed and labelled with its language in words before it is
//!   tinted. Strip every colour out of the document and both still read as what
//!   they are, which is what a reader with a colour vision deficiency, a
//!   high-contrast mode, or a client that flattens backgrounds actually gets.
//!   [`tests::nothing_in_the_letter_is_told_by_colour_alone`] strips them and
//!   checks.
//!
//! ## What "email-safe" costs, itemised
//!
//! - **Layout is tables.** Not nostalgia: Word has no `float`, no `flex` and no
//!   reliable `max-width`, so a centred column of a fixed width is expressed as
//!   a full-width table containing a centred one. The width lives in the `width`
//!   attribute — which Word reads — and in `max-width` on a wrapping `<div>`,
//!   which everything else reads. The Outlook-only "ghost table" in a
//!   conditional comment is what lets those two coexist: Outlook takes the
//!   fixed 600 px, every other client takes the fluid `<div>` and narrows on a
//!   phone. No media query is involved, because a media query lives in a
//!   stylesheet and this document has none. Measured rather than assumed: both
//!   pinned letters render at a 360 px viewport with `scrollWidth` equal to
//!   `clientWidth` — nothing overflows, including the table and the code.
//! - **Fonts are the ones already on the machine.** The product's own typeface
//!   is a webfont, and a webfont in a mail is a request the client blocks or an
//!   attachment nobody wants. `Arial, Helvetica, sans-serif` is what actually
//!   renders.
//! - **Colours are hex literals copied from `web/src/ds/tokens.css`.** A mail
//!   cannot read a CSS custom property, so the tokens cannot be referenced —
//!   only quoted. Each constant below names the token it was copied from, which
//!   is the only thing that makes the drift findable when the palette moves.
//!   The dark half is quoted from the same file's navy scale, because the
//!   design system has no dark theme yet — see [`LETTER_PALETTE`].
//! - **`mso-line-height-rule:exactly` on every text run**, because Word
//!   otherwise reinterprets `line-height` as a minimum and the letter's spacing
//!   changes shape between two recipients of the same mail.
//! - **Newlines are `<br />`, not whitespace.** HTML collapses a line break and
//!   a writer does not expect that; in a code block, leading and internal
//!   spacing is rebuilt with `&#160;` so indentation survives collapsing too.
//!
//! ## The vocabulary is closed, and this is a total match
//!
//! [`CampaignBlock`] has four variants and no escape hatch, so [`block_html`]
//! matches all four with no fallback arm. A fifth block added to the model is a
//! compile error here rather than a block that silently vanishes from the mail
//! — which is the failure the golden files could not catch, since a golden only
//! pins what was rendered.
//!
//! ## The golden files, and what they are actually for
//!
//! *The same blocks must produce the same HTML, so a regression is visible
//! rather than discovered by a customer's recipients.* The output is a pure
//! function of the letter: no clock, no random ids, no iteration over a hash
//! map. `tests/campaign_html_golden.rs` pins three documents, each named for
//! the `schema_version` it is written against — a golden is only meaningful
//! against one version of the model, and a body declaring another version is
//! refused here rather than half-rendered.
//!
//! ## What this module deliberately does not do
//!
//! - **It does not send.** The unsubscribe footer it *does* now carry (C2.5)
//!   arrives as a required [`UnsubscribeInvitation`] on the letter rather than
//!   as something this module builds: the URL is per recipient, so only the
//!   sender knows it, and the words are the recipient's language, which this
//!   crate has no notion of. What is enforced here is that there is no way to
//!   render a letter without one — the earlier note in this list said such a
//!   parameter would be `None` at every call site and pinned empty by every
//!   golden, and making it required rather than optional is exactly what
//!   answers that.
//! - **It does not personalise, and that is now a guarantee rather than a gap.**
//!   [`crate::campaign_merge`] (C3.4) resolves a letter for one recipient
//!   *before* it reaches here, so this renderer only ever sees finished words
//!   and escapes them like any other. The order matters: escaping first would
//!   put a fallback containing an apostrophe into the letter as `&#39;` while
//!   the text part kept the apostrophe, which is two parts of one mail
//!   disagreeing about a character.
//! - **It does not draw images**, because the model has no image block and the
//!   reason is a decision C3.1 recorded: a remote image is a fetch we can see,
//!   which is the open-tracking pixel ADR 0044 refused. See the C3.5 section
//!   above for what that costs and what would have to be decided first.
//! - **It does not decide the plain-text part.** C3.3 reads the same
//!   [`CampaignBlock`] values into a text alternative; that is why the table's
//!   rectangular rule is enforced at save time and not in either renderer.
//! - **It emits no string of our own in any language.** Everything in the
//!   output is either markup or the writer's own words, so there is nothing
//!   here to translate — a rendered letter is in whatever language it was
//!   typed in.

use crate::campaign_content::{
    CampaignBlock, CampaignContent, CodeBlock, HeadingBlock, ParagraphBlock, TableBlock,
};
use crate::campaign_unsubscribe_link::UnsubscribeInvitation;
use crate::error::Result;

/// The width of the reading column, in pixels.
///
/// 600 is the width Outlook's default reading pane fits without a horizontal
/// scroll bar, and every mail client has been built to expect it since.
pub const CAMPAIGN_LETTER_WIDTH_PX: u32 = 600;

/// The typeface stack. Web-safe only — see the module docs.
const SANS: &str = "Arial,Helvetica,sans-serif";
/// The monospace stack for code.
const MONO: &str = "Consolas,Menlo,Monaco,'Courier New',monospace";

/// `--bg-app` in `web/src/ds/tokens.css`, quoted rather than referenced.
const PAGE_BACKGROUND: &str = "#f4f1ec";
/// `--bg-surface`.
const CARD_BACKGROUND: &str = "#fffefc";
/// `--bg-sunken` — the tint behind a code sample and a table header.
const SUNKEN_BACKGROUND: &str = "#f0ece6";
/// `--ink`, the navy headings are set in.
const HEADING_COLOUR: &str = "#102a43";
/// `--text-secondary`, the slate prose is set in.
const TEXT_COLOUR: &str = "#475569";
/// `--border-default`.
const RULE_COLOUR: &str = "#ded7cd";

/// `--navy-700`, the ground a dark-mode letter sits on.
const DARK_PAGE_BACKGROUND: &str = "#0c2036";
/// `--navy-600` (`--ink`) — the card, one step up from the ground exactly as
/// the light card is one step up from the light ground.
const DARK_CARD_BACKGROUND: &str = "#102a43";
/// `--navy-500`, "soft navy — rail hover, dark cards". A recessed surface is
/// *lighter* than its card in the dark, which is the one place the dark palette
/// is not a mirror of the light one: light sinks by darkening, dark sinks by
/// lifting, and both read as "set back".
const DARK_SUNKEN_BACKGROUND: &str = "#1f3d5b";
/// `--warm-50` (`--cream`), what headings are set in on navy.
const DARK_HEADING_COLOUR: &str = "#f8f6f2";
/// `--navy-100`, the prose colour. Not white: a full-strength white on navy at
/// 16 px halates, and the letter is read in long lines.
const DARK_TEXT_COLOUR: &str = "#c6d2de";
/// `--navy-400`.
const DARK_RULE_COLOUR: &str = "#35506b";

/// What a colour is *for*, which decides the declaration the dark block has to
/// override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColourRole {
    /// Something is painted with it — `background-color`.
    Surface,
    /// Something is written in it — `color`.
    Ink,
    /// Something is drawn with it — the `border` shorthand.
    Rule,
}

/// The letter's whole palette: the light value every element declares on
/// itself, the dark value the one stylesheet swaps in, and the role that
/// decides how.
///
/// **This table is the dark-mode stylesheet.** [`dark_mode_style`] generates one
/// rule per row and there is no second list to keep in step: a colour added
/// here is a colour that inverts, and a colour used in the renderer but missing
/// here is caught by
/// [`tests::every_colour_the_letter_draws_with_has_a_dark_twin`] rather than
/// arriving as a light patch in a dark letter.
///
/// **The dark values are the navy scale of `web/src/ds/tokens.css` read the
/// other way up**, and none of them is invented here. That matters because the
/// product has *no dark theme yet* — when the design system grows one, these
/// six rows are the list to reconcile it against, and the reconciliation is a
/// change to this table alone.
///
/// The light values are unchanged by this wave, so a light-mode recipient
/// receives byte-for-byte what they received before it.
const LETTER_PALETTE: [(&str, &str, ColourRole); 6] = [
    (PAGE_BACKGROUND, DARK_PAGE_BACKGROUND, ColourRole::Surface),
    (CARD_BACKGROUND, DARK_CARD_BACKGROUND, ColourRole::Surface),
    (
        SUNKEN_BACKGROUND,
        DARK_SUNKEN_BACKGROUND,
        ColourRole::Surface,
    ),
    (HEADING_COLOUR, DARK_HEADING_COLOUR, ColourRole::Ink),
    (TEXT_COLOUR, DARK_TEXT_COLOUR, ColourRole::Ink),
    (RULE_COLOUR, DARK_RULE_COLOUR, ColourRole::Rule),
];

/// How many invisible characters follow the preheader.
///
/// A preview pane shows roughly 100 characters, and whatever the preheader does
/// not fill it takes from the top of the body — so a two-word preheader arrives
/// as "Spring prices Dear customer, we are writing to". The padding is a run of
/// zero-width non-joiners and non-breaking spaces, which occupy that budget and
/// draw nothing. 60 pairs cover a preheader of any length this model allows.
const PREHEADER_PADDING_PAIRS: usize = 60;

/// A campaign as the renderer needs it: the parts that reach a recipient.
///
/// Deliberately **not** [`crate::campaign_record::Campaign`]. The compiler has
/// no database in it and no opinion about who wrote the letter or when; taking
/// the record would tie a pure function to a row and make every renderer test
/// build one. The record's caller assembles this in one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CampaignLetter<'a> {
    /// The subject line. Reaches the document's `<title>`, escaped — it is not
    /// re-validated here, because subject rules belong to the record that
    /// stores it and a second copy of a rule is a second place it can differ.
    pub subject: &'a str,
    /// The preview text beside the subject, or `None`.
    ///
    /// `None` emits no hidden block at all, so the client falls back to the top
    /// of the body — the ordinary behaviour, and honest. A blank-but-present
    /// preheader would instead hand the pane a run of padding and show nothing.
    pub preheader: Option<&'a str>,
    /// The body.
    pub content: &'a CampaignContent,
    /// How this recipient leaves (C2.4/C2.5).
    ///
    /// **Required, not optional, and that is the point.** A bulk message with
    /// no way out is unlawful under GDPR Art. 21(3) and ePrivacy Art. 13, and
    /// undeliverable to Gmail and Outlook, which have required RFC 8058
    /// one-click from bulk senders since February 2024. Ten thousand messages
    /// sent without one cannot be un-sent, and "the sender forgot" is not a
    /// defence anybody can offer a regulator — so the compiler refuses instead
    /// of a reviewer noticing.
    pub unsubscribe: &'a UnsubscribeInvitation,
}

/// Compiles a campaign into the `text/html` part of a mail.
///
/// The output is a complete document — doctype, head, body — because that is
/// what a MIME part has to be, and what C3.6's preview puts in a frame.
///
/// # Errors
/// [`crate::error::StoreError::Validation`] when the body would not pass the
/// write gate: a `schema_version` this build does not speak, a block type an
/// email cannot carry, a ragged table, a blank heading, a duplicate block id.
/// The renderer refuses rather than drawing something no writer could have
/// saved — the same discipline as reading a body back out of the column, and
/// for the same reason: [`CampaignContent`]'s fields are public, so a value can
/// reach here without ever having passed the gate.
pub fn render_campaign_html(letter: &CampaignLetter<'_>) -> Result<String> {
    letter.content.validate()?;

    let mut out = String::with_capacity(2_048);
    out.push_str(
        "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Transitional//EN\" \
         \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd\">\n",
    );
    out.push_str("<html xmlns=\"http://www.w3.org/1999/xhtml\">\n");
    head_html(letter.subject, &mut out);
    body_html(letter, &mut out);
    out.push_str("</html>\n");
    Ok(out)
}

/// The head: the `<meta>` tags a mail client acts on, the title, the Outlook
/// DPI fix, and the dark-mode block.
///
/// `x-apple-disable-message-reformatting` stops Apple Mail rescaling the
/// column, and the `PixelsPerInch` block stops Outlook multiplying every pixel
/// by 1.25 on a high-DPI screen — which is why a letter that looked right in
/// the preview arrives with the table wider than the column.
///
/// The two colour-scheme declarations are what stop a client *inverting* the
/// letter for us. Apple Mail and Outlook.com force their own transform on a
/// document that does not say it handles both schemes, and a forced inversion
/// on top of [`dark_mode_style`]'s repaint is a letter inverted twice — light
/// text on a light card. Declaring both schemes says "we drew this one, leave
/// it alone". `supported-color-schemes` is the older spelling and several
/// shipped clients still read only that one, so both are emitted.
fn head_html(subject: &str, out: &mut String) {
    out.push_str("<head>\n");
    out.push_str("<meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\" />\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\" />\n");
    out.push_str("<meta name=\"x-apple-disable-message-reformatting\" />\n");
    out.push_str(
        "<meta name=\"format-detection\" content=\"telephone=no,date=no,address=no\" />\n",
    );
    out.push_str("<meta name=\"color-scheme\" content=\"light dark\" />\n");
    out.push_str("<meta name=\"supported-color-schemes\" content=\"light dark\" />\n");
    out.push_str("<title>");
    out.push_str(&esc(subject));
    out.push_str("</title>\n");
    out.push_str(
        "<!--[if mso]><xml><o:OfficeDocumentSettings>\
         <o:PixelsPerInch>96</o:PixelsPerInch>\
         </o:OfficeDocumentSettings></xml><![endif]-->\n",
    );
    dark_mode_style(out);
    out.push_str("</head>\n");
}

/// The only stylesheet in the document: one `@media (prefers-color-scheme:dark)`
/// block, generated from [`LETTER_PALETTE`].
///
/// **Each rule selects on the light declaration it replaces** —
/// `[style*="background-color:#f4f1ec"]` is every element that paints itself
/// the light page colour — so the markup needs no `class` and no hook
/// attribute, and an element written after this function is dark-correct
/// without anybody remembering to label it. `!important` is required rather
/// than shouted: an inline declaration outranks a stylesheet rule of any
/// specificity, and every colour in this letter is inline.
///
/// Two things about the selectors are worth knowing before editing them.
/// `background-color:#…` *contains* `color:#…`, so an ink rule would also match
/// a surface that happened to use the same hex —
/// [`tests::a_colour_means_one_thing_in_this_letter`] pins that no hex is used
/// for two roles, which is what makes the substring match exact. And a light
/// value and a *dark* value may legitimately be the same hex
/// (`HEADING_COLOUR` and `DARK_CARD_BACKGROUND` are both `#102a43`) without any
/// interaction at all: a selector reads the attribute the renderer wrote, never
/// the colour the cascade computed.
///
/// Word ignores the whole element — it implements no media query — which is the
/// intended outcome: Outlook on Windows draws the light letter.
fn dark_mode_style(out: &mut String) {
    out.push_str("<style type=\"text/css\">\n@media (prefers-color-scheme:dark){\n");
    for (light, dark, role) in LETTER_PALETTE {
        match role {
            ColourRole::Surface => out.push_str(&format!(
                "[style*=\"background-color:{light}\"]{{background-color:{dark}!important;}}\n"
            )),
            ColourRole::Ink => {
                out.push_str(&format!(
                    "[style*=\"color:{light}\"]{{color:{dark}!important;}}\n"
                ));
            }
            ColourRole::Rule => out.push_str(&format!(
                "[style*=\"solid {light}\"]{{border-color:{dark}!important;}}\n"
            )),
        }
    }
    out.push_str("}\n</style>\n");
}

/// The body: the hidden preheader, then the two nested layout tables that make
/// a centred column of [`CAMPAIGN_LETTER_WIDTH_PX`].
fn body_html(letter: &CampaignLetter<'_>, out: &mut String) {
    // `color-scheme` is on the body as well as in the head because Apple Mail
    // reads it there and iOS Mail has shipped versions that read only there.
    // It is a declaration about which schemes the document handles, not a
    // colour, so it does not belong in `LETTER_PALETTE`.
    out.push_str(&format!(
        "<body style=\"margin:0;padding:0;width:100%;background-color:{PAGE_BACKGROUND};\
         color-scheme:light dark;supported-color-schemes:light dark;\
         -webkit-text-size-adjust:100%;-ms-text-size-adjust:100%;\">\n"
    ));

    if let Some(preheader) = letter.preheader {
        preheader_html(preheader, out);
    }

    // The outer table paints the page and centres what is inside it. Layout
    // tables say `role="presentation"` so a screen reader announces the letter
    // rather than "table, one column"; the writer's own tables deliberately do
    // not — see `table_html`.
    out.push_str(&format!(
        "<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" \
         border=\"0\" bgcolor=\"{PAGE_BACKGROUND}\" \
         style=\"width:100%;border-collapse:collapse;background-color:{PAGE_BACKGROUND};\">\n"
    ));
    out.push_str("<tr>\n<td align=\"center\" style=\"padding:24px 12px;\">\n");

    // The ghost table: Outlook reads it and gets a fixed column, every other
    // client skips the comment and gets the fluid <div> below.
    out.push_str(&format!(
        "<!--[if mso]><table role=\"presentation\" width=\"{CAMPAIGN_LETTER_WIDTH_PX}\" \
         align=\"center\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\" \
         style=\"width:{CAMPAIGN_LETTER_WIDTH_PX}px;\"><tr><td><![endif]-->\n"
    ));
    out.push_str(&format!(
        "<div style=\"max-width:{CAMPAIGN_LETTER_WIDTH_PX}px;margin:0 auto;\">\n"
    ));
    out.push_str(&format!(
        "<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" \
         border=\"0\" bgcolor=\"{CARD_BACKGROUND}\" \
         style=\"width:100%;border-collapse:collapse;background-color:{CARD_BACKGROUND};\">\n"
    ));
    // The bottom padding is 16 px short of the top on purpose: every block
    // carries a 16 px margin below it, including the last one, and a letter
    // that padded both ends equally would sit visibly high in its card. Taking
    // it off the cell rather than off the final block keeps each block's
    // rendering independent of where it happens to be — which is what makes a
    // golden file a diff of one block rather than of the whole letter.
    out.push_str("<tr>\n<td style=\"padding:32px 28px 16px 28px;\">\n");

    for block in &letter.content.blocks {
        block_html(block, out);
    }

    out.push_str("</td>\n</tr>\n</table>\n");
    unsubscribe_html(letter.unsubscribe, out);
    out.push_str("</div>\n");
    out.push_str("<!--[if mso]></td></tr></table><![endif]-->\n");
    out.push_str("</td>\n</tr>\n</table>\n");
    out.push_str("</body>\n");
}

/// The visible way out (C2.5), under the card and inside the same column.
///
/// **Under the card rather than in it**, which is the convention every bulk
/// sender follows and every recipient has learned: the card holds the tenant's
/// words, and this is the machinery around them. Inside it, the footer would
/// read as part of the message.
///
/// A plain link in the letter's own prose colour — never a button, never
/// disguised as anything else. ADR 0044 §3 is explicit that a recipient who
/// cannot find the way out presses the spam button instead, and a "manage your
/// preferences" euphemism is exactly how they fail to find it. The words are
/// the caller's (see [`UnsubscribeInvitation::link_text`]) because this crate
/// knows nothing about the language the letter is written in.
///
/// The colour is a **plain inline declaration with no `!important`**, and that
/// is deliberate. [`dark_mode_style`] repaints prose by matching
/// `[style*="color:…"]` and winning with `!important`; an inline `!important`
/// here would beat that rule — a style attribute outranks a stylesheet even
/// between two important declarations — and the footer would stay slate on the
/// dark navy card, the one link in the message a recipient needs to be able to
/// read. Following the document's own mechanism costs nothing and inherits the
/// dark mode the rest of the letter already has.
fn unsubscribe_html(unsubscribe: &UnsubscribeInvitation, out: &mut String) {
    out.push_str(&format!(
        "<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" \
         border=\"0\" style=\"width:100%;border-collapse:collapse;\">\n\
         <tr>\n<td align=\"center\" style=\"padding:16px 28px 8px 28px;\
         font-family:{SANS};font-size:12px;line-height:18px;\
         mso-line-height-rule:exactly;color:{TEXT_COLOUR};\">\n"
    ));
    out.push_str(&format!(
        "<a href=\"{}\" style=\"color:{TEXT_COLOUR};text-decoration:underline;\">{}</a>\n",
        esc(unsubscribe.url.trim()),
        esc_text(&unsubscribe.link_text)
    ));
    out.push_str("</td>\n</tr>\n</table>\n");
}

/// The preview text, hidden six ways because no single way works everywhere.
///
/// `display:none` is enough for most clients, `mso-hide:all` for Word, and the
/// zero heights and `opacity` for the ones that honour neither. The padding
/// that follows is explained at [`PREHEADER_PADDING_PAIRS`].
fn preheader_html(preheader: &str, out: &mut String) {
    out.push_str(
        "<div style=\"display:none;font-size:1px;line-height:1px;max-height:0;max-width:0;\
         opacity:0;overflow:hidden;mso-hide:all;\">",
    );
    out.push_str(&esc_text(preheader));
    for _ in 0..PREHEADER_PADDING_PAIRS {
        // Zero-width non-joiner and a non-breaking space: they take up the
        // preview pane's character budget and draw nothing.
        out.push_str("&#8204;&#160;");
    }
    out.push_str("</div>\n");
}

/// One block. A total match over a closed vocabulary — see the module docs.
fn block_html(block: &CampaignBlock, out: &mut String) {
    match block {
        CampaignBlock::Heading(heading) => heading_html(heading, out),
        CampaignBlock::Paragraph(paragraph) => paragraph_html(paragraph, out),
        CampaignBlock::Table(table) => table_html(table, out),
        CampaignBlock::Code(code) => code_html(code, out),
    }
}

/// A heading. `margin:0` is not tidiness: Word applies its own margins to
/// `<h1>` and `<h2>`, so a heading that did not state them would be spaced
/// differently for an Outlook reader than for everybody else.
fn heading_html(heading: &HeadingBlock, out: &mut String) {
    let (tag, size, line) = if heading.level == 1 {
        ("h1", 28, 34)
    } else {
        ("h2", 20, 26)
    };
    out.push_str(&format!(
        "<{tag} style=\"margin:0 0 16px 0;font-family:{SANS};font-size:{size}px;\
         line-height:{line}px;font-weight:bold;color:{HEADING_COLOUR};\
         mso-line-height-rule:exactly;\">"
    ));
    out.push_str(&esc_inline(&heading.text));
    out.push_str(&format!("</{tag}>\n"));
}

/// Prose.
///
/// An **empty** paragraph renders a non-breaking space rather than nothing. The
/// Docs editor opens a body with one empty paragraph and writers use more of
/// them as deliberate blank lines; an empty `<p></p>` collapses to no height in
/// Word, so the spacing the writer put in would quietly disappear between the
/// composer and the inbox.
fn paragraph_html(paragraph: &ParagraphBlock, out: &mut String) {
    out.push_str(&format!(
        "<p style=\"margin:0 0 16px 0;font-family:{SANS};font-size:16px;line-height:24px;\
         color:{TEXT_COLOUR};mso-line-height-rule:exactly;\">"
    ));
    if paragraph.text.is_empty() {
        out.push_str("&#160;");
    } else {
        out.push_str(&esc_inline(&paragraph.text));
    }
    out.push_str("</p>\n");
}

/// The writer's table — a **data** table, and the one table in this document
/// that is not `role="presentation"`.
///
/// Marking it presentational would be the copy-paste bug this file is most
/// exposed to, and it would tell a screen reader to read a price list as
/// unrelated runs of text. `rows[0]` is the header, as in Docs, so it is
/// `<th scope="col">`.
fn table_html(table: &TableBlock, out: &mut String) {
    out.push_str(
        "<table width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\" \
         style=\"width:100%;border-collapse:collapse;margin:0 0 16px 0;\">\n",
    );
    for (index, row) in table.rows.iter().enumerate() {
        let header = index == 0;
        out.push_str("<tr>\n");
        for cell in row {
            let tag = if header { "th" } else { "td" };
            let weight = if header { "bold" } else { "normal" };
            let colour = if header { HEADING_COLOUR } else { TEXT_COLOUR };
            let scope = if header { " scope=\"col\"" } else { "" };
            // The tint is stated twice for the same reason every other tinted
            // cell in this document states it twice — Word honours the
            // presentational attribute when it ignores the CSS. It is
            // decoration either way: the header row is a `<th scope="col">` in
            // bold before it is a colour, which is what a reader who never
            // sees the tint has to go on.
            let (bgcolor, background) = if header {
                (
                    format!(" bgcolor=\"{SUNKEN_BACKGROUND}\""),
                    format!("background-color:{SUNKEN_BACKGROUND};"),
                )
            } else {
                (String::new(), String::new())
            };
            out.push_str(&format!(
                "<{tag}{scope} align=\"left\" valign=\"top\"{bgcolor} \
                 style=\"padding:8px 10px;border:1px solid {RULE_COLOUR};{background}\
                 font-family:{SANS};font-size:14px;line-height:20px;font-weight:{weight};\
                 color:{colour};text-align:left;mso-line-height-rule:exactly;\">"
            ));
            out.push_str(&esc_inline(cell));
            out.push_str(&format!("</{tag}>\n"));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>\n");
}

/// A code sample: a tinted, bordered cell in a monospace face, under a visible
/// label naming the language.
///
/// The label is visible rather than a `class` attribute, and that is the point
/// — a class needs a stylesheet to mean anything and this document has none, so
/// the language would arrive as invisible metadata. Written as the editor
/// recorded it: a language token is `bash` or `c++` in every locale, so there
/// is nothing to translate.
///
/// `<pre>` is avoided because Word neither honours `white-space:pre` nor keeps
/// a background behind it; the indentation is rebuilt in [`esc_code`] instead.
fn code_html(code: &CodeBlock, out: &mut String) {
    // A table is never laid out narrower than its longest unbreakable word,
    // and code is made of those — one `https://…` URL in a sample can push the
    // card past the edge of a phone and take the prose with it. Two
    // declarations guard against that, because no client honours both:
    // `word-break` on the cell below is what Chrome and WebKit act on
    // (measured: a 106-character URL in this frame overflows nothing at a
    // 360 px viewport, with or without the line here), and
    // `table-layout:fixed` is the one Word acts on, since it implements
    // neither `word-break` nor `overflow-wrap`. The frame has a single column,
    // so fixing its layout changes nothing that can be seen.
    out.push_str(
        "<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" \
         border=\"0\" \
         style=\"width:100%;table-layout:fixed;border-collapse:collapse;margin:0 0 16px 0;\">\n",
    );
    out.push_str(&format!(
        "<tr>\n<td align=\"left\" bgcolor=\"{SUNKEN_BACKGROUND}\" \
         style=\"padding:8px 12px;border:1px solid {RULE_COLOUR};border-bottom:none;\
         background-color:{SUNKEN_BACKGROUND};font-family:{MONO};font-size:12px;\
         line-height:16px;color:{TEXT_COLOUR};text-align:left;\
         mso-line-height-rule:exactly;\">"
    ));
    out.push_str(&esc(&code.language));
    out.push_str("</td>\n</tr>\n");
    out.push_str(&format!(
        "<tr>\n<td align=\"left\" bgcolor=\"{CARD_BACKGROUND}\" \
         style=\"padding:12px;border:1px solid {RULE_COLOUR};background-color:{CARD_BACKGROUND};\
         font-family:{MONO};font-size:13px;line-height:20px;color:{HEADING_COLOUR};\
         text-align:left;word-break:break-word;mso-line-height-rule:exactly;\">"
    ));
    out.push_str(&esc_code(&code.code));
    out.push_str("</td>\n</tr>\n</table>\n");
}

/// Escapes text for HTML.
///
/// All five of `&<>"'`, whatever the position: this renderer puts the writer's
/// words into element content and the subject into a `<title>`, and one escaper
/// that is safe everywhere is worth more than two that have to be chosen
/// between. C0 control characters are dropped rather than escaped — they are
/// not valid in an HTML document at all, and a client that meets one may stop
/// parsing mid-letter. `\n` and `\t` survive here and are dealt with by the
/// callers that care about them.
fn esc(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Escapes a run of text that must not carry line structure — a preheader,
/// which lives in a hidden one-line block. Newlines and tabs become spaces so a
/// pasted two-line preheader does not arrive with a literal break in a preview
/// pane that cannot draw one.
fn esc_text(value: &str) -> String {
    esc(value)
        .replace(['\n', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Escapes text where the writer's line breaks are meaning.
///
/// `\r\n` is folded to `\n` first: a body pasted from a Windows editor would
/// otherwise arrive with every break doubled. A trailing newline draws a
/// trailing blank line the writer did not intend to send, so it is dropped.
fn esc_inline(value: &str) -> String {
    let text = value.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.strip_suffix('\n').unwrap_or(&text);
    esc(text).replace('\n', "<br />")
}

/// Escapes code, rebuilding the layout HTML would collapse.
///
/// A tab becomes four spaces — a tab in HTML is one collapsible whitespace
/// character, so an indented sample would arrive flat. Then, per line: every
/// leading space becomes `&#160;`, because HTML drops whitespace at the start
/// of a line however much of it there is; and inside the line, a run of two or
/// more spaces keeps all but its last as `&#160;`, so aligned comments stay
/// aligned while the line still has somewhere to wrap on a phone.
fn esc_code(value: &str) -> String {
    let text = value.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.strip_suffix('\n').unwrap_or(&text);
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push_str("<br />");
        }
        out.push_str(&esc_code_line(&line.replace('\t', "    ")));
    }
    out
}

/// One line of code, with its spacing rebuilt. See [`esc_code`].
fn esc_code_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut at_line_start = true;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != ' ' {
            at_line_start = false;
            out.push_str(&esc(&c.to_string()));
            continue;
        }
        // Count this space and the ones directly after it.
        let mut run = 1_usize;
        while chars.peek() == Some(&' ') {
            chars.next();
            run += 1;
        }
        if at_line_start {
            for _ in 0..run {
                out.push_str("&#160;");
            }
        } else {
            for _ in 1..run {
                out.push_str("&#160;");
            }
            out.push(' ');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::error::StoreError;
    use serde_json::json;

    fn body(blocks: serde_json::Value) -> CampaignContent {
        CampaignContent::from_value(json!({ "schema_version": 1, "blocks": blocks }))
            .expect("the fixture body is valid")
    }

    fn render(subject: &str, preheader: Option<&str>, content: &CampaignContent) -> String {
        render_campaign_html(&CampaignLetter {
            subject,
            preheader,
            content,
            unsubscribe: &crate::campaign_unsubscribe_link::an_invitation(),
        })
        .expect("a validated body renders")
    }

    fn letter() -> CampaignContent {
        body(json!([
            { "type": "heading", "id": "h1", "level": 1, "text": "Spring prices" },
            { "type": "paragraph", "id": "p1", "text": "Everything below is per litre." },
            { "type": "table", "id": "t1", "rows": [["Product", "Price"], ["Oil", "€12"]] },
            { "type": "code", "id": "c1", "code": "curl https://alo", "language": "bash" },
        ]))
    }

    /// The item's own sentence, as a test: *the same blocks must produce the
    /// same HTML.* Nothing in the output may come from a clock, a random
    /// source, or the iteration order of a hash map.
    #[test]
    fn the_same_blocks_produce_the_same_html_every_time() {
        let content = letter();
        let first = render("Spring prices", Some("Per litre, from Monday"), &content);
        let second = render("Spring prices", Some("Per litre, from Monday"), &content);
        assert_eq!(first, second, "the renderer must be a pure function");
    }

    /// Everything between `<style>` and `</style>`, and the document without
    /// it. A client that drops the head sees the second one, and so does every
    /// test that is about the light letter.
    fn split_stylesheet(html: &str) -> (String, String) {
        let open = html.find("<style").expect("the dark-mode block");
        let content_from = html[open..].find('>').expect("an unclosed <style") + open + 1;
        let close = html.find("</style>").expect("an unclosed stylesheet");
        let end = close + "</style>\n".len();
        (
            html[content_from..close].to_owned(),
            format!("{}{}", &html[..open], &html[end..]),
        )
    }

    /// *A compiler, not a stylesheet* — with the one exception C3.5 bought, and
    /// this test is where the exception is spelled out rather than assumed.
    ///
    /// The document carries exactly one `<style>`, it holds nothing but the
    /// dark-mode media query, and **the letter is whole without it**: every
    /// light colour is still declared on the element it paints, so a client
    /// that drops the head draws the same letter it drew before dark mode
    /// existed. No `class` and no `<link>` — those would need a stylesheet to
    /// *mean* anything, which is the difference.
    #[test]
    fn the_only_stylesheet_is_the_dark_block_and_the_letter_is_whole_without_it() {
        let html = render("Spring prices", Some("Per litre"), &letter());
        assert_eq!(html.matches("<style").count(), 1, "{html}");
        assert_eq!(html.matches("</style>").count(), 1, "{html}");
        assert!(!html.contains("<link"), "a linked stylesheet: {html}");
        assert!(!html.contains("class=\""), "a class attribute: {html}");

        let (stylesheet, without) = split_stylesheet(&html);
        let outside_the_media_block = stylesheet
            .trim()
            .strip_prefix("@media (prefers-color-scheme:dark){")
            .and_then(|rest| rest.trim_end().strip_suffix('}'))
            .expect("the stylesheet is one media block");
        assert!(
            !outside_the_media_block.contains('@') && !outside_the_media_block.contains('<'),
            "the stylesheet holds more than the dark block: {stylesheet}"
        );
        assert!(!without.contains("@media"), "{without}");

        // The letter without the stylesheet is still fully painted.
        for (light, _, _) in LETTER_PALETTE {
            assert!(
                without.contains(light),
                "{light} is drawn only by the stylesheet: {without}"
            );
        }
    }

    /// A colour the renderer draws with but [`LETTER_PALETTE`] does not know
    /// about is a light patch in a dark letter — a white card under white text,
    /// or a border that stays warm stone on navy. The palette is the list, and
    /// this is what makes it the list.
    #[test]
    fn every_colour_the_letter_draws_with_has_a_dark_twin() {
        let (_, without) = split_stylesheet(&render("Spring prices", Some("Per litre"), &letter()));
        let known: Vec<&str> = LETTER_PALETTE.iter().map(|(light, _, _)| *light).collect();
        let bytes: Vec<char> = without.chars().collect();
        for (index, c) in bytes.iter().enumerate() {
            if *c != '#' || index + 6 >= bytes.len() {
                continue;
            }
            let candidate: String = bytes[index..index + 7].iter().collect();
            if !candidate[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }
            assert!(
                known.contains(&candidate.as_str()),
                "{candidate} is drawn but has no dark twin: add it to LETTER_PALETTE"
            );
        }
    }

    /// The dark rules select on the light declaration they replace, and
    /// `background-color:#…` contains `color:#…` — so a hex used both as ink and
    /// as a surface would make one rule fire on the other's elements. Keeping
    /// every colour to one role is what makes the substring match exact.
    #[test]
    fn a_colour_means_one_thing_in_this_letter() {
        for (index, (light, dark, role)) in LETTER_PALETTE.iter().enumerate() {
            for (other_light, other_dark, other_role) in LETTER_PALETTE.iter().skip(index + 1) {
                assert_ne!(
                    light, other_light,
                    "{light} is both {role:?} and {other_role:?}"
                );
                assert_ne!(dark, other_dark, "the dark palette reuses {dark}");
            }
            assert_eq!(light.len(), 7, "{light} is not a six-digit hex");
            assert_eq!(dark.len(), 7, "{dark} is not a six-digit hex");
        }
    }

    /// A dark-mode reader gets the letter **repainted**, not inverted: our
    /// colours swapped for ours, and the two colour-scheme declarations that
    /// stop a client transforming it a second time on top.
    #[test]
    fn a_dark_mode_reader_gets_the_letter_repainted_rather_than_inverted() {
        let html = render("Spring prices", Some("Per litre"), &letter());
        assert!(html.contains("<meta name=\"color-scheme\" content=\"light dark\" />"));
        assert!(html.contains("<meta name=\"supported-color-schemes\" content=\"light dark\" />"));
        assert!(html.contains("color-scheme:light dark;"), "{html}");

        let (stylesheet, without) = split_stylesheet(&html);
        assert!(stylesheet.contains("@media (prefers-color-scheme:dark){"));
        assert!(
            stylesheet.contains(
                "[style*=\"background-color:#f4f1ec\"]{background-color:#0c2036!important;}"
            ),
            "{stylesheet}"
        );
        assert!(
            stylesheet.contains("[style*=\"color:#475569\"]{color:#c6d2de!important;}"),
            "{stylesheet}"
        );
        assert!(
            stylesheet.contains("[style*=\"solid #ded7cd\"]{border-color:#35506b!important;}"),
            "{stylesheet}"
        );
        assert_eq!(
            stylesheet.matches("!important").count(),
            LETTER_PALETTE.len(),
            "one rule per colour, and an inline declaration outranks a rule that does not shout"
        );

        // A dark value never reaches the markup: it exists only for a reader
        // whose client asked for it.
        for (_, dark, _) in LETTER_PALETTE {
            if LETTER_PALETTE.iter().any(|(light, _, _)| *light == dark) {
                continue; // #102a43 is the light ink and the dark card at once.
            }
            assert!(
                !without.contains(dark),
                "{dark} was drawn into the letter itself: {without}"
            );
        }
    }

    /// The relative luminance of a hex colour, WCAG 2.1 §relative luminance.
    fn luminance(hex: &str) -> f64 {
        let channel = |from: usize| {
            let raw = u8::from_str_radix(&hex[from..from + 2], 16).expect("a six-digit hex");
            let value = f64::from(raw) / 255.0;
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
    }

    /// The WCAG contrast ratio between two hex colours.
    fn contrast(one: &str, other: &str) -> f64 {
        let (a, b) = (luminance(one), luminance(other));
        let (light, dark) = if a > b { (a, b) } else { (b, a) };
        (light + 0.05) / (dark + 0.05)
    }

    fn by_role(role: ColourRole, dark: bool) -> Vec<&'static str> {
        LETTER_PALETTE
            .iter()
            .filter(|(_, _, r)| *r == role)
            .map(|(l, d, _)| if dark { *d } else { *l })
            .collect()
    }

    /// *A dark-mode-safe palette*, measured rather than eyeballed.
    ///
    /// Every ink on every surface reads at WCAG AA for body text (4.5:1) in
    /// both palettes — the letter is 16 px prose, so AA is the floor and not a
    /// target. The border is the one element below that in both palettes, as it
    /// is in the design system it was copied from and in every mail a recipient
    /// already gets: it is decoration over a table that is already a table, so
    /// the invariant it has to meet is *no worse in the dark than in the
    /// light*.
    #[test]
    fn the_dark_palette_reads_at_least_as_well_as_the_light_one() {
        for dark in [false, true] {
            let which = if dark { "dark" } else { "light" };
            for ink in by_role(ColourRole::Ink, dark) {
                for surface in by_role(ColourRole::Surface, dark) {
                    let ratio = contrast(ink, surface);
                    assert!(
                        ratio >= 4.5,
                        "the {which} palette sets {ink} on {surface} at {ratio:.2}:1, under AA"
                    );
                }
            }
        }

        let light_rule = by_role(ColourRole::Rule, false)[0];
        let dark_rule = by_role(ColourRole::Rule, true)[0];
        for (index, surface) in by_role(ColourRole::Surface, false).iter().enumerate() {
            let dark_surface = by_role(ColourRole::Surface, true)[index];
            let (light_ratio, dark_ratio) = (
                contrast(light_rule, surface),
                contrast(dark_rule, dark_surface),
            );
            assert!(
                dark_ratio >= light_ratio,
                "a border on {dark_surface} reads at {dark_ratio:.2}:1 where the light letter \
                 gives {light_ratio:.2}:1"
            );
        }
    }

    /// Strips every colour out of the document. What is left is what a reader
    /// with a colour vision deficiency, a forced high-contrast mode, or a
    /// client that flattens backgrounds has to read the letter from.
    ///
    /// The dark-mode block goes first and whole: it is a list of colours by
    /// definition, and it is the letter's own markup this test is about.
    fn without_colour(html: &str) -> String {
        let mut out = split_stylesheet(html).1;
        for (light, dark, role) in LETTER_PALETTE {
            for value in [light, dark] {
                out = match role {
                    ColourRole::Surface => out
                        .replace(&format!("background-color:{value};"), "")
                        .replace(&format!(" bgcolor=\"{value}\""), ""),
                    ColourRole::Ink => out.replace(&format!("color:{value};"), ""),
                    ColourRole::Rule => out.replace(&format!("solid {value}"), "solid"),
                };
            }
        }
        out
    }

    /// *Colour never the only carrier of meaning.* Every distinction the letter
    /// draws is doubled by something that is not a colour, and the way to prove
    /// it is to take the colours away and read what is left.
    #[test]
    fn nothing_in_the_letter_is_told_by_colour_alone() {
        let flat = without_colour(&render("Spring prices", None, &letter()));
        assert!(
            !flat.contains("background-color") && !flat.contains("bgcolor"),
            "a tint survived the strip, so the test proves nothing: {flat}"
        );

        // The header row: a header cell, scoped, in bold — three carriers
        // before the tint that is now gone.
        assert!(flat.contains("<th scope=\"col\""), "{flat}");
        assert!(flat.contains("font-weight:bold"), "{flat}");
        // The code sample: a monospace face, a frame, and its language written
        // out in words rather than implied by the tint.
        assert!(flat.contains(MONO), "{flat}");
        assert!(flat.contains("border:1px solid;"), "{flat}");
        assert!(flat.contains(">bash</td>"), "{flat}");
        // The headings: their own tags and their own sizes.
        assert!(flat.contains("<h1 style="), "{flat}");
        assert!(flat.contains("font-size:28px"), "{flat}");
        // And the prose is still prose.
        assert!(flat.contains("Everything below is per litre."), "{flat}");
    }

    /// *The mail must read with images blocked. Half of recipients see that
    /// version and they are not a degraded audience.*
    ///
    /// Here they see the same letter, because there is nothing to block: no
    /// image element, no background image, no remote reference of any kind.
    /// That is a stronger promise than alt text and it is the one this model
    /// can make today — see the module docs for the block that does not exist
    /// and the decision it waits on.
    #[test]
    fn a_letter_reads_the_same_with_images_blocked_because_it_has_none() {
        let html = render("Spring prices", Some("Per litre"), &letter());
        for absent in [
            "<img",
            "<picture",
            "<svg",
            "<video",
            "background-image",
            "url(",
            " src=",
            "list-style-image",
        ] {
            assert!(
                !html.contains(absent),
                "{absent} is something a client can block: {html}"
            );
        }
    }

    /// Drops everything between `<!--` and `-->`.
    ///
    /// What is left is the document every client sees. The Outlook-only ghost
    /// table lives inside a conditional comment and is sized by its `width`
    /// attribute, which is the only sizing Word reads there — so it is markup
    /// rather than a drawn element, and the rules below do not apply to it.
    fn without_conditional_comments(html: &str) -> String {
        let mut out = String::with_capacity(html.len());
        let mut rest = html;
        while let Some(open) = rest.find("<!--") {
            out.push_str(&rest[..open]);
            let after = &rest[open..];
            match after.find("-->") {
                Some(close) => rest = &after[close + 3..],
                None => panic!("an unclosed comment: {after}"),
            }
        }
        out.push_str(rest);
        out
    }

    /// Every element that draws something states how it draws it, on itself.
    #[test]
    fn every_drawn_element_carries_its_own_inline_style() {
        let html = without_conditional_comments(&render("Spring prices", None, &letter()));
        for tag in ["<h1", "<h2", "<p", "<td", "<th", "<table", "<body", "<div"] {
            let mut from = 0;
            while let Some(at) = html[from..].find(tag) {
                let start = from + at;
                let end = html[start..].find('>').expect("an unclosed tag") + start;
                let opening = &html[start..end];
                assert!(
                    opening.contains("style=\""),
                    "an element draws with no style of its own: {opening}"
                );
                from = end;
            }
        }
    }

    /// The writer's table is a data table, and only the layout ones are
    /// presentational. Getting this backwards reads a price list to a screen
    /// reader as unrelated runs of text.
    #[test]
    fn the_writers_table_is_a_data_table_and_the_layout_ones_are_not() {
        let html = render("Spring prices", None, &letter());
        assert!(
            html.contains("<th scope=\"col\""),
            "the header row must be header cells: {html}"
        );
        // The writer's table opens without a role; every other table has one.
        assert!(
            html.contains("<table width=\"100%\" cellpadding=\"0\""),
            "the data table must not be role=presentation: {html}"
        );
        let presentational = html.matches("role=\"presentation\"").count();
        assert_eq!(
            presentational, 5,
            "the two layout tables, the ghost table, the code frame and the \
             unsubscribe footer — every table that is not the writer's own data \
             announces itself as layout, so a screen reader reads the letter \
             rather than \"table, one column\": {html}"
        );
    }

    /// Nothing a writer types can become markup. The validator already refuses
    /// a language token that is not a plain name; the renderer escapes it
    /// anyway, because a second lock costs one function call.
    #[test]
    fn no_word_a_writer_typed_can_become_markup() {
        let hostile = body(json!([
            { "type": "heading", "id": "h1", "level": 2, "text": "<script>alert(1)</script>" },
            { "type": "paragraph", "id": "p1", "text": "a & b \"quoted\" 'single' <b>bold</b>" },
            { "type": "table", "id": "t1", "rows": [["<td>", "\"x\""], ["</table>", "&amp;"]] },
            { "type": "code", "id": "c1", "code": "</td></tr></table><img src=x>", "language": "sh" },
        ]));
        let html = render(
            "</title><script>alert(1)</script>",
            Some("<b>hi</b>"),
            &hostile,
        );
        assert!(!html.contains("<script"), "a script survived: {html}");
        assert!(!html.contains("<img"), "an image tag survived: {html}");
        assert!(!html.contains("<b>"), "markup survived: {html}");
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(
            html.contains("&amp;amp;"),
            "an entity must be escaped once more"
        );
        assert!(
            html.contains("<title>&lt;/title&gt;&lt;script&gt;"),
            "the subject must not close its own title: {html}"
        );
        // The letter is still a document: every table it opened, it closed.
        assert_eq!(
            html.matches("<table").count(),
            html.matches("</table>").count()
        );
    }

    /// A line break is meaning. HTML collapses one; Windows sends two.
    #[test]
    fn a_writers_line_breaks_survive_and_are_not_doubled() {
        let content = body(json!([
            { "type": "paragraph", "id": "p1", "text": "First line\r\nsecond line\nthird\n" },
        ]));
        let html = render("s", None, &content);
        assert!(
            html.contains("First line<br />second line<br />third</p>"),
            "{html}"
        );
        assert_eq!(
            html.matches("<br />").count(),
            2,
            "a trailing newline is not a line: {html}"
        );
    }

    /// An empty paragraph is a blank line the writer put there on purpose, and
    /// an empty `<p>` collapses to nothing in Word.
    #[test]
    fn an_empty_paragraph_is_a_blank_line_rather_than_nothing() {
        let content = body(json!([
            { "type": "paragraph", "id": "p1", "text": "Above" },
            { "type": "paragraph", "id": "p2", "text": "" },
            { "type": "paragraph", "id": "p3", "text": "Below" },
        ]));
        let html = render("s", None, &content);
        assert!(
            html.contains(">&#160;</p>"),
            "the blank line vanished: {html}"
        );
    }

    /// Indentation is the only thing a code sample has, and HTML throws it
    /// away.
    #[test]
    fn code_keeps_its_indentation_and_its_alignment() {
        let content = body(json!([
            { "type": "code", "id": "c1", "code": "fn main() {\n\tlet a = 1;  // one\n}", "language": "rust" },
        ]));
        let html = render("s", None, &content);
        // The tab became four spaces, all of them at the start of the line, so
        // all four are non-breaking.
        assert!(
            html.contains("&#160;&#160;&#160;&#160;let"),
            "indentation lost: {html}"
        );
        // Two interior spaces keep one non-breaking and one that can wrap.
        assert!(
            html.contains("=&#160;1;&#160; //&#160;one") || html.contains("1;&#160; // one"),
            "{html}"
        );
        assert!(
            html.contains("rust"),
            "the language must be visible, not a class: {html}"
        );
    }

    /// A preheader that is absent must not become a run of padding with nothing
    /// in front of it — a preview pane would then show a blank line where the
    /// top of the letter should be.
    #[test]
    fn a_letter_with_no_preheader_has_no_hidden_block_at_all() {
        let html = render("Spring prices", None, &letter());
        assert!(!html.contains("mso-hide:all"), "{html}");
        assert!(!html.contains("&#8204;"), "{html}");

        let with = render("Spring prices", Some("Per litre,\n from Monday"), &letter());
        assert!(with.contains("mso-hide:all"), "{with}");
        // Folded to one line: a preview pane cannot draw a break.
        assert!(with.contains(">Per litre, from Monday&#8204;"), "{with}");
        assert_eq!(with.matches("&#8204;").count(), PREHEADER_PADDING_PAIRS);
    }

    /// The envelope exists so a golden file means something. A body from a
    /// newer build is refused by name rather than half-drawn.
    #[test]
    fn a_body_written_in_another_model_is_refused_rather_than_half_drawn() {
        let content = CampaignContent {
            schema_version: 2,
            blocks: Vec::new(),
        };
        match render_campaign_html(&CampaignLetter {
            subject: "s",
            preheader: None,
            content: &content,
            unsubscribe: &crate::campaign_unsubscribe_link::an_invitation(),
        }) {
            Err(StoreError::Validation(detail)) => {
                assert!(detail.contains("schema_version"), "{detail}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// [`CampaignContent`]'s fields are public, so a value can reach the
    /// renderer without ever having passed the write gate. Drawing it would
    /// produce a letter no writer could have saved.
    #[test]
    fn a_body_that_never_passed_the_write_gate_is_refused_rather_than_drawn() {
        let ragged = CampaignContent {
            schema_version: 1,
            blocks: vec![CampaignBlock::Table(TableBlock {
                id: "t1".to_owned(),
                rows: vec![vec!["a".to_owned(), "b".to_owned()], vec!["a".to_owned()]],
            })],
        };
        match render_campaign_html(&CampaignLetter {
            subject: "s",
            preheader: None,
            content: &ragged,
            unsubscribe: &crate::campaign_unsubscribe_link::an_invitation(),
        }) {
            Err(StoreError::Validation(detail)) => assert!(detail.contains("columns"), "{detail}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// An empty body is a legitimate draft, and it must still be a document.
    #[test]
    fn an_empty_campaign_is_still_a_whole_document() {
        let html = render("Nothing yet", None, &CampaignContent::empty());
        assert!(html.starts_with("<!DOCTYPE html PUBLIC"));
        assert!(html.contains("<title>Nothing yet</title>"));
        assert!(html.trim_end().ends_with("</html>"));
    }

    /// Word needs the width as an attribute and everything else needs it as
    /// CSS, and the ghost table is what lets both be true at once.
    #[test]
    fn outlook_gets_a_fixed_column_and_a_phone_gets_a_fluid_one() {
        let html = render("s", None, &letter());
        assert!(
            html.contains("<!--[if mso]><table role=\"presentation\" width=\"600\""),
            "{html}"
        );
        assert!(
            html.contains("<!--[if mso]></td></tr></table><![endif]-->"),
            "{html}"
        );
        assert!(html.contains("max-width:600px;margin:0 auto;"), "{html}");
        assert!(
            html.contains("<o:PixelsPerInch>96</o:PixelsPerInch>"),
            "{html}"
        );
    }

    /// Word reinterprets `line-height` as a minimum unless told not to, which
    /// changes the letter's shape between two recipients of the same mail.
    #[test]
    fn every_run_of_text_pins_its_line_height_for_word() {
        let html = render("s", None, &letter());
        let runs = html.matches("line-height:").count();
        let pinned = html.matches("mso-line-height-rule:exactly").count();
        assert_eq!(
            runs - html.matches("line-height:1px").count(),
            pinned,
            "a text run left its line height for Word to reinterpret: {html}"
        );
    }

    /// A control character is not valid in an HTML document and a client that
    /// meets one may stop parsing mid-letter.
    #[test]
    fn a_control_character_is_dropped_rather_than_carried_into_the_document() {
        let content = body(json!([
            { "type": "paragraph", "id": "p1", "text": "before\u{0}after\u{7}" },
        ]));
        let html = render("s", None, &content);
        assert!(html.contains("beforeafter"), "{html}");
        assert!(!html.contains('\u{0}'));
        assert!(!html.contains('\u{7}'));
    }

    /// Accented text and a currency symbol are the ordinary case in a European
    /// product, and they travel as themselves under a `utf-8` declaration.
    #[test]
    fn european_text_travels_as_itself() {
        let content = body(json!([
            { "type": "paragraph", "id": "p1", "text": "Prijzen per liter — Genève, 12 €" },
        ]));
        let html = render("Prijzen", None, &content);
        assert!(html.contains("charset=utf-8"));
        assert!(html.contains("Prijzen per liter — Genève, 12 €"), "{html}");
    }
}
