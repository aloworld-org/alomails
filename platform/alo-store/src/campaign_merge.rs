//! Personalisation, and the fallback that makes it safe to send (alo Campaigns,
//! ADR 0044, wave C3.4).
//!
//! Queue item C3.4: *personalisation with a **visible fallback for every merge
//! field**. "Hi ," is the classic bulk-mail failure and it comes from a field
//! nobody defaulted; a field with no fallback is a validation error at save
//! time, not a surprise at send time.*
//!
//! Everything below follows from that one sentence, so it is worth being exact
//! about the failure it describes. A tenant writes *"Hi {{first_name}},"*. Most
//! of their audience came from a billing customer with a name on it, so the
//! preview looks right and the letter is approved. The people whose address
//! arrived from a web form have no name, and they receive *"Hi ,"* — a mail that
//! announces it was generated, to precisely the recipients who know the sender
//! least. The letter cannot be recalled, and the number that moves is the
//! complaint rate. **The fix is not a better default at send time; it is that the
//! letter could not have been saved.**
//!
//! ## The grammar
//!
//! `{{field|fallback}}` — the field to insert, a vertical bar, and the words to
//! print when this recipient has no value for it.
//!
//! ```text
//! Hi {{first_name|there}}, we are writing to {{email|your address}}.
//! ```
//!
//! Spacing around either side is ignored (`{{ first_name | there }}` is the same
//! placeholder), because a writer spacing a template out is not making a
//! different request.
//!
//! ## Why the field names are a closed list
//!
//! This is the part that is easy to get wrong. Once every field must carry a
//! fallback, an *unknown* field is invisible: `{{frist_name|there}}` would
//! resolve to "there" for every recipient, in every preview and every seed send,
//! and read as a working letter that is merely impersonal. The typo would be
//! found by nobody. So [`CampaignMergeField`] is a closed vocabulary checked at save
//! time, and it is exactly what [`CampaignRecipient`](crate::CampaignRecipient)
//! can supply — a field the audience query cannot fill is a field that would
//! always print its fallback, which is the same failure wearing a different hat.
//!
//! (`hr_letters::MergeField` is a different vocabulary for a different document
//! — an employment letter about a colleague, whose facts come from an HR record
//! rather than from an audience. The two are named alike because the idea is the
//! same and share nothing because the facts are not; that is why the type here
//! carries the `Campaign` prefix the rest of this track's exports do.)
//!
//! ## Blank counts as missing
//!
//! A name of `"   "` in a billing row is not a name, and treating it as one
//! prints *"Hi ,"* through a fallback that was supposed to prevent exactly that.
//! Every value is trimmed and an empty result takes the fallback.
//! `{{first_name|…}}` additionally takes the first word of the recorded name and
//! drops a trailing comma from it, so a record written *"Dupont, Jean"* greets
//! "Dupont" rather than "Dupont,". It cannot know which half is the given name;
//! what it can do is never emit punctuation the writer did not type.
//!
//! ## Where the rule lives, and why it lives in one place
//!
//! **Validation is at the write gate** — [`CampaignContent`]'s block rules and
//! [`crate::campaign_record`]'s subject and preheader — so a body with an
//! undefaulted field is refused by `POST /campaigns/campaigns` with the reason,
//! while somebody is writing.
//!
//! **Resolution is one function applied before either renderer**
//! ([`personalise_campaign`]), never inside them. Two reasons, both learned from
//! the shape of the modules it feeds:
//!
//! - [`crate::campaign_html`] escapes the writer's words on the way out
//!   (`esc_inline`). A substitution performed *after* that would put a fallback
//!   containing an apostrophe into the letter as `&#39;` while the text part
//!   kept the apostrophe — the two parts of one mail disagreeing about a
//!   character. Personalising the [`CampaignContent`] first means both renderers
//!   receive the same resolved words and each does its own escaping to its own
//!   part, as before.
//! - A rule applied in two renderers is a rule that can differ in one of them.
//!   This module is the single parser: [`compile`] both validates and resolves,
//!   so what the gate refuses and what the send would have produced cannot drift
//!   apart.
//!
//! Resolution is **single pass and never recursive**: a value is copied into the
//! output and not re-scanned, so a customer whose recorded name is
//! `{{email|x}}` receives their odd name rather than their own address.
//!
//! ## Where merge fields are deliberately not read
//!
//! - **Code blocks.** `{{ … }}` is the template syntax of Handlebars, Vue,
//!   Angular, Jinja and Go, so a code sample is the one place in a letter where
//!   those four characters are plausibly the subject matter. Personalising them
//!   would rewrite somebody's documentation; refusing them would make a code
//!   sample unsendable. A code block is therefore literal, and that is also the
//!   answer to "how do I write `{{` in a campaign".
//! - **The topic** ([`crate::campaign_record`]). It is a label on the
//!   unsubscribe page (C2s.2), read by a recipient who is leaving, and nothing
//!   resolves it there — a placeholder in it would arrive on that page verbatim.
//!   So merge syntax in a topic is refused by name rather than left to leak.
//!
//! ## What this module deliberately does not do
//!
//! - **It does not send, and it is not a preview.** It turns one letter and one
//!   recipient's values into one resolved letter. Which recipients exist is
//!   wave C1's; the screen that shows the result is C3.6's; the send is C2's and
//!   waits on an IP.
//! - **It does not re-cap the text it produces.** A resolved paragraph is
//!   marginally longer or shorter than the source the caps were measured
//!   against; both renderers re-validate the content they are handed
//!   ([`CampaignContent::validate`]), so a value that pushed a block past its cap
//!   surfaces as a refusal rather than as a malformed letter.
//! - **It emits no string of our own in any language.** A fallback is the
//!   writer's own words, in whatever language the letter was typed in.

use crate::campaign_audience::CampaignRecipient;
use crate::campaign_content::{CampaignBlock, CampaignContent, ParagraphBlock, TableBlock};
use crate::campaign_html::CampaignLetter;
use crate::error::{Result, StoreError};

/// What opens a merge field.
const OPEN: &str = "{{";
/// What closes one.
const CLOSE: &str = "}}";
/// What separates the field from its fallback.
const BAR: char = '|';

/// The longest fallback a merge field may carry.
///
/// A fallback stands in for a name or an address, not for a sentence: past this
/// the writer is composing two versions of the letter inside one placeholder,
/// and the version nobody previews is the one most recipients get.
pub const CAMPAIGN_MERGE_FALLBACK_MAX: usize = 80;

/// A value a campaign may insert about the person reading it.
///
/// Closed on purpose — see the module docs. Every variant is something
/// [`CampaignRecipient`] actually holds, so no field can be one that always
/// prints its fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CampaignMergeField {
    /// The first word of the recorded name — the greeting field, and the one
    /// the whole item is named after.
    FirstName,
    /// The recorded name, whole, as the source that supplied it wrote it.
    Name,
    /// The address this copy of the letter is going to.
    Email,
    /// ISO 3166-1 alpha-2, where a billing customer names one.
    Country,
}

impl CampaignMergeField {
    /// Every field a campaign can use, in the order a composer would offer
    /// them: the one people want first, then the rest.
    pub const ALL: [CampaignMergeField; 4] = [
        CampaignMergeField::FirstName,
        CampaignMergeField::Name,
        CampaignMergeField::Email,
        CampaignMergeField::Country,
    ];

    /// The name written between the braces.
    pub fn as_str(self) -> &'static str {
        match self {
            CampaignMergeField::FirstName => "first_name",
            CampaignMergeField::Name => "name",
            CampaignMergeField::Email => "email",
            CampaignMergeField::Country => "country",
        }
    }

    /// The field by that name, or `None` when there is none.
    ///
    /// Case-sensitive: `{{First_Name|there}}` is refused rather than accepted,
    /// because a vocabulary that quietly accepts two spellings is a vocabulary
    /// a composer cannot offer as a list.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.as_str() == name)
    }
}

/// One recipient's values, as the letter will see them.
///
/// Built from a [`CampaignRecipient`] at send or preview time. The fields are
/// public because this is a plain carrier — the rules are in [`compile`], and a
/// value that arrives blank takes the writer's fallback wherever it came from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CampaignMergeValues {
    /// The address this copy is going to.
    pub email: String,
    /// The best name any source offers, or `None` when every source left it
    /// blank. Never invented from the address — [`crate::campaign_audience`]
    /// records why, and this is the module that would otherwise be tempted.
    pub name: Option<String>,
    /// ISO 3166-1 alpha-2, or `None` when no source names one.
    pub country: Option<String>,
}

impl CampaignMergeValues {
    /// The values of a person the audience query returned.
    pub fn for_recipient(recipient: &CampaignRecipient) -> Self {
        CampaignMergeValues {
            email: recipient.address.clone(),
            name: recipient.name.clone(),
            country: recipient.country.clone(),
        }
    }

    /// This recipient's value for a field, or `None` when they have none — in
    /// which case the writer's fallback is printed.
    fn value(&self, field: CampaignMergeField) -> Option<String> {
        match field {
            CampaignMergeField::Email => present(&self.email).map(str::to_owned),
            CampaignMergeField::Name => self.name.as_deref().and_then(present).map(str::to_owned),
            CampaignMergeField::Country => {
                self.country.as_deref().and_then(present).map(str::to_owned)
            }
            CampaignMergeField::FirstName => self
                .name
                .as_deref()
                .and_then(present)
                .and_then(first_name)
                .map(str::to_owned),
        }
    }
}

impl From<&CampaignRecipient> for CampaignMergeValues {
    fn from(recipient: &CampaignRecipient) -> Self {
        Self::for_recipient(recipient)
    }
}

/// A letter with one recipient's values already in it.
///
/// Owned, because resolution produces new strings; [`letter`](Self::letter)
/// hands the borrowing view the HTML renderer takes, and
/// [`content`](Self::content) the one the text renderer takes. Both parts come
/// from this one value, which is what stops them personalising differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonalisedLetter {
    /// The subject, resolved.
    pub subject: String,
    /// The preview text, resolved, or `None`.
    pub preheader: Option<String>,
    /// The body, resolved.
    pub content: CampaignContent,
}

impl PersonalisedLetter {
    /// The view [`crate::campaign_html::render_campaign_html`] takes.
    pub fn letter(&self) -> CampaignLetter<'_> {
        CampaignLetter {
            subject: &self.subject,
            preheader: self.preheader.as_deref(),
            content: &self.content,
        }
    }

    /// The body [`crate::campaign_text::render_campaign_text`] takes.
    pub fn content(&self) -> &CampaignContent {
        &self.content
    }
}

/// Checks that every merge field in a run of the writer's text names a known
/// value and carries a fallback.
///
/// `what` names where the text sits (`"a paragraph"`, `"the subject line"`), so
/// a writer with a long letter is told which block to look at rather than that
/// something somewhere is wrong.
///
/// # Errors
/// [`StoreError::Validation`] on a field with no fallback, a blank or
/// over-long fallback, a field name this build does not know, or a `{{` that is
/// never closed.
pub fn validate_merge_text(what: &str, text: &str) -> Result<()> {
    compile_at(what, text, None).map(|_| ())
}

/// Refuses merge syntax outright, for the fields a letter never personalises.
///
/// Used by the topic, which is drawn on the unsubscribe page and resolved by
/// nothing — see the module docs.
///
/// # Errors
/// [`StoreError::Validation`] when the text contains `{{`.
pub fn reject_merge_fields(what: &str, text: &str) -> Result<()> {
    if text.contains(OPEN) {
        return Err(StoreError::Validation(format!(
            "{what} is not personalised — it is shown as written, so a {OPEN}field{CLOSE} in it \
             would arrive in front of a reader exactly like that"
        )));
    }
    Ok(())
}

/// Resolves one run of the writer's text against one recipient.
///
/// # Errors
/// [`StoreError::Validation`] as [`validate_merge_text`] — text that could not
/// have been saved is refused here too, because [`CampaignContent`]'s fields
/// are public and a value can reach a renderer without passing the gate.
pub fn resolve_merge_text(text: &str, values: &CampaignMergeValues) -> Result<String> {
    compile(text, Some(values))
}

/// Resolves a whole body against one recipient, leaving code blocks literal.
///
/// # Errors
/// [`StoreError::Validation`] on a body that would not pass the write gate, or
/// on a merge field in it that would not.
pub fn resolve_merge_content(
    content: &CampaignContent,
    values: &CampaignMergeValues,
) -> Result<CampaignContent> {
    content.validate()?;
    let mut blocks = Vec::with_capacity(content.blocks.len());
    for block in &content.blocks {
        blocks.push(resolve_block(block, values)?);
    }
    Ok(CampaignContent {
        schema_version: content.schema_version,
        blocks,
    })
}

/// Turns a letter and a recipient into the letter that recipient receives.
///
/// **This is the entry point a preview (C3.6) and a send (C2) both use**, and
/// running it once for both MIME parts is what guarantees they say the same
/// thing: [`PersonalisedLetter::letter`] feeds the HTML renderer and
/// [`PersonalisedLetter::content`] the text one, from the same resolved values.
///
/// # Errors
/// [`StoreError::Validation`] on a letter that would not pass the write gate,
/// in its body, its subject or its preheader.
pub fn personalise_campaign(
    letter: &CampaignLetter<'_>,
    values: &CampaignMergeValues,
) -> Result<PersonalisedLetter> {
    Ok(PersonalisedLetter {
        subject: compile_at("the subject line", letter.subject, Some(values))?,
        preheader: letter
            .preheader
            .map(|preheader| compile_at("preview text", preheader, Some(values)))
            .transpose()?,
        content: resolve_merge_content(letter.content, values)?,
    })
}

/// One block, resolved. A total match over a closed vocabulary, as both
/// renderers keep: a fifth block added to the model is a compile error here
/// rather than a block that quietly stops being personalised.
fn resolve_block(block: &CampaignBlock, values: &CampaignMergeValues) -> Result<CampaignBlock> {
    Ok(match block {
        CampaignBlock::Heading(heading) => {
            let mut resolved = heading.clone();
            resolved.text = compile_at("a heading", &heading.text, Some(values))?;
            CampaignBlock::Heading(resolved)
        }
        CampaignBlock::Paragraph(paragraph) => CampaignBlock::Paragraph(ParagraphBlock {
            id: paragraph.id.clone(),
            text: compile_at("a paragraph", &paragraph.text, Some(values))?,
        }),
        CampaignBlock::Table(table) => {
            let mut rows = Vec::with_capacity(table.rows.len());
            for row in &table.rows {
                let mut cells = Vec::with_capacity(row.len());
                for cell in row {
                    cells.push(compile_at("a table cell", cell, Some(values))?);
                }
                rows.push(cells);
            }
            CampaignBlock::Table(TableBlock {
                id: table.id.clone(),
                rows,
            })
        }
        // Literal, and the module docs say why: `{{ … }}` in a code sample is
        // somebody else's template, not ours to resolve.
        CampaignBlock::Code(code) => CampaignBlock::Code(code.clone()),
    })
}

/// [`compile`], with the location of the text folded into whatever it refuses.
///
/// A writer with a forty-block letter needs to be told which block to look at,
/// and the same prefix is used by the write gate and by resolution — so the
/// message somebody reads while composing is the message the send would have
/// produced.
fn compile_at(what: &str, text: &str, values: Option<&CampaignMergeValues>) -> Result<String> {
    compile(text, values).map_err(|error| match error {
        StoreError::Validation(detail) => StoreError::Validation(format!("{what}: {detail}")),
        other => other,
    })
}

/// **The one parser.** With `values` it resolves; without them it validates and
/// its output is discarded.
///
/// Two functions would be two grammars, and a grammar the gate reads differently
/// from the grammar the send reads is the whole failure this module exists to
/// prevent — the letter would pass validation and arrive wrong.
///
/// Substituted values are appended to the output and never re-examined, so a
/// recipient whose name happens to contain `{{email|x}}` receives their name.
fn compile(text: &str, values: Option<&CampaignMergeValues>) -> Result<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find(OPEN) {
        out.push_str(&rest[..open]);
        let after = &rest[open + OPEN.len()..];
        let Some(close) = after.find(CLOSE) else {
            return Err(invalid(format!(
                "a merge field was opened with {OPEN} and never closed with {CLOSE} — write it as \
                 {OPEN}first_name{BAR}there{CLOSE}, or put the braces in a code block if they are \
                 the subject matter"
            )));
        };
        let (field, fallback) = parse_placeholder(&after[..close])?;
        if let Some(values) = values {
            match values.value(field) {
                Some(value) => out.push_str(&value),
                None => out.push_str(fallback),
            }
        }
        rest = &after[close + CLOSE.len()..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The inside of one `{{ … }}`: the field, and the fallback it must carry.
fn parse_placeholder(inner: &str) -> Result<(CampaignMergeField, &str)> {
    let (name, fallback) = match inner.split_once(BAR) {
        Some((name, fallback)) => (name.trim(), Some(fallback.trim())),
        None => (inner.trim(), None),
    };
    if name.is_empty() {
        return Err(invalid(format!(
            "a merge field says which value it stands for — {}",
            vocabulary()
        )));
    }
    let Some(field) = CampaignMergeField::parse(name) else {
        return Err(invalid(format!(
            "a campaign has no merge field called {name:?} — {}. A name that is not one of those \
             would print its fallback to every recipient and read like a letter that simply was \
             not personalised",
            vocabulary()
        )));
    };
    let Some(fallback) = fallback else {
        return Err(invalid(format!(
            "{OPEN}{name}{CLOSE} has no fallback, and the recipients without a {name} are exactly \
             the ones who would notice: write {OPEN}{name}{BAR}the words to use instead{CLOSE}"
        )));
    };
    if fallback.is_empty() {
        return Err(invalid(format!(
            "the fallback after {OPEN}{name}{BAR} is blank, which is the \"Hi ,\" this rule \
             exists to prevent — say what a recipient with no {name} should read"
        )));
    }
    if fallback.chars().count() > CAMPAIGN_MERGE_FALLBACK_MAX {
        return Err(invalid(format!(
            "a fallback fits in {CAMPAIGN_MERGE_FALLBACK_MAX} characters — it stands in for a \
             name, not for a second version of the sentence"
        )));
    }
    if fallback.contains([BAR, '{', '}']) {
        return Err(invalid(
            "a fallback is plain text — it cannot contain a brace or a bar, because those are \
             what a merge field is written with",
        ));
    }
    Ok((field, fallback))
}

/// The vocabulary, spelled out for an error somebody can act on.
fn vocabulary() -> String {
    let names: Vec<&str> = CampaignMergeField::ALL
        .iter()
        .map(|field| field.as_str())
        .collect();
    format!("a campaign can personalise {}", names.join(", "))
}

/// A value, trimmed, or `None` when there was nothing in it. Blank is missing —
/// see the module docs.
fn present(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

/// The first word of a recorded name, without a trailing comma or semicolon.
///
/// A record written "Dupont, Jean" greets "Dupont": which half is the given
/// name is not knowable from the string, and printing the punctuation would be
/// a second, more visible mistake on top of the first.
fn first_name(name: &str) -> Option<&str> {
    let word = name.split_whitespace().next()?;
    present(word.trim_end_matches([',', ';']))
}

/// The rejection every rule in this module returns.
fn invalid(detail: impl Into<String>) -> StoreError {
    StoreError::Validation(detail.into())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::campaign_content::CAMPAIGN_CONTENT_SCHEMA_VERSION;
    use crate::campaign_html::render_campaign_html;
    use crate::campaign_text::render_campaign_text;
    use serde_json::json;

    fn values() -> CampaignMergeValues {
        CampaignMergeValues {
            email: "jean.dupont@example.fr".to_owned(),
            name: Some("Jean Dupont".to_owned()),
            country: Some("FR".to_owned()),
        }
    }

    fn nameless() -> CampaignMergeValues {
        CampaignMergeValues {
            email: "someone@example.fr".to_owned(),
            name: None,
            country: None,
        }
    }

    fn detail(result: Result<String>) -> String {
        match result {
            Err(StoreError::Validation(detail)) => detail,
            other => panic!("expected a validation error, got {other:?}"),
        }
    }

    fn rejected(text: &str) -> String {
        detail(compile(text, None))
    }

    fn body(blocks: serde_json::Value) -> CampaignContent {
        CampaignContent::from_value(json!({ "schema_version": 1, "blocks": blocks }))
            .expect("a valid body")
    }

    #[test]
    fn a_field_with_no_fallback_cannot_be_saved_because_hi_comma_cannot_be_recalled() {
        // The item, in one test: the letter that would arrive as "Hi ," is
        // refused while somebody is writing it.
        let detail = rejected("Hi {{first_name}},");
        assert!(
            detail.contains("first_name") && detail.contains("fallback"),
            "{detail}"
        );
        // And a fallback that is present but says nothing is the same failure.
        let blank = rejected("Hi {{first_name|}},");
        assert!(blank.contains("blank"), "{blank}");
        let spaces = rejected("Hi {{first_name|   }},");
        assert!(spaces.contains("blank"), "{spaces}");
        // Written properly, it passes and personalises.
        assert_eq!(
            resolve_merge_text("Hi {{first_name|there}},", &values()).unwrap(),
            "Hi Jean,"
        );
        assert_eq!(
            resolve_merge_text("Hi {{first_name|there}},", &nameless()).unwrap(),
            "Hi there,"
        );
    }

    #[test]
    fn a_misspelled_field_is_refused_rather_than_silently_always_falling_back() {
        // The reason the vocabulary is closed: with a mandatory fallback, an
        // unknown field resolves to the fallback for everybody and reads like a
        // letter that was simply not personalised. Nobody would ever find it.
        let detail = rejected("Hi {{frist_name|there}},");
        assert!(detail.contains("frist_name"), "{detail}");
        assert!(
            detail.contains("first_name"),
            "the writer is told the real names: {detail}"
        );
        // Case is part of the name, so a composer can offer the list verbatim.
        assert!(rejected("{{First_Name|there}}").contains("First_Name"));
        assert!(rejected("{{|there}}").contains("stands for"));
    }

    #[test]
    fn every_field_in_the_vocabulary_resolves_and_falls_back() {
        for field in CampaignMergeField::ALL {
            let name = field.as_str();
            let text = format!("[{OPEN}{name}{BAR}missing{CLOSE}]");
            let resolved = resolve_merge_text(&text, &values()).unwrap();
            assert_ne!(resolved, "[missing]", "{name} should have resolved");
            assert!(!resolved.contains(OPEN), "{name} left a placeholder behind");
            // A recipient with nothing recorded gets the writer's words, and
            // `email` is the one value every recipient has by definition.
            let empty = CampaignMergeValues::default();
            assert_eq!(
                resolve_merge_text(&text, &empty).unwrap(),
                "[missing]",
                "{name} must fall back when there is no value"
            );
        }
        assert_eq!(
            resolve_merge_text("{{name|—}} / {{email|—}} / {{country|—}}", &values()).unwrap(),
            "Jean Dupont / jean.dupont@example.fr / FR"
        );
    }

    #[test]
    fn a_blank_value_is_a_missing_value_rather_than_a_greeting_with_nothing_in_it() {
        let blank = CampaignMergeValues {
            email: "  ".to_owned(),
            name: Some("   ".to_owned()),
            country: Some("".to_owned()),
        };
        assert_eq!(
            resolve_merge_text(
                "{{first_name|there}}/{{name|friend}}/{{country|nowhere}}",
                &blank
            )
            .unwrap(),
            "there/friend/nowhere"
        );
        assert_eq!(
            resolve_merge_text("{{email|your address}}", &blank).unwrap(),
            "your address"
        );
    }

    #[test]
    fn a_first_name_is_the_first_word_and_never_carries_its_punctuation() {
        let named = |name: &str| CampaignMergeValues {
            email: "x@example.fr".to_owned(),
            name: Some(name.to_owned()),
            country: None,
        };
        for (recorded, greeting) in [
            ("Jean Dupont", "Jean"),
            ("  Jean   Dupont  ", "Jean"),
            ("Dupont, Jean", "Dupont"),
            ("Jean", "Jean"),
        ] {
            assert_eq!(
                resolve_merge_text("{{first_name|there}}", &named(recorded)).unwrap(),
                greeting,
                "recorded as {recorded:?}"
            );
        }
        // A name that is only punctuation leaves nothing to greet, so the
        // writer's fallback is used rather than a comma on its own.
        assert_eq!(
            resolve_merge_text("{{first_name|there}}", &named(",")).unwrap(),
            "there"
        );
    }

    #[test]
    fn a_value_is_never_re_scanned_as_a_template() {
        // A recipient whose recorded name contains a placeholder receives their
        // odd name, not their own address — resolution is one pass.
        let odd = CampaignMergeValues {
            email: "odd@example.fr".to_owned(),
            name: Some("{{email|x}}".to_owned()),
            country: None,
        };
        assert_eq!(
            resolve_merge_text("Hi {{name|there}},", &odd).unwrap(),
            "Hi {{email|x}},"
        );
    }

    #[test]
    fn an_unclosed_field_is_refused_rather_than_printed_at_the_recipient() {
        let detail = rejected("Hi {{first_name|there,");
        assert!(detail.contains("never closed"), "{detail}");
        // A stray closer is ordinary punctuation and stays as typed.
        assert_eq!(resolve_merge_text("a}}b", &values()).unwrap(), "a}}b");
        // Text with no placeholder at all is returned byte for byte.
        assert_eq!(
            resolve_merge_text("Prices are 10 % off — { not a field }", &values()).unwrap(),
            "Prices are 10 % off — { not a field }"
        );
    }

    #[test]
    fn a_fallback_is_short_plain_text() {
        let long = "x".repeat(CAMPAIGN_MERGE_FALLBACK_MAX + 1);
        assert!(rejected(&format!("{OPEN}name{BAR}{long}{CLOSE}")).contains("characters"));
        assert!(rejected("{{name|a|b}}").contains("bar"));
        assert!(rejected("{{name|{x}}}").contains("brace"));
        // Spacing around the parts is ignored — a writer spacing a template out
        // is not making a different request.
        assert_eq!(
            resolve_merge_text("{{ first_name | there }}", &nameless()).unwrap(),
            "there"
        );
    }

    #[test]
    fn a_topic_refuses_merge_syntax_because_nothing_resolves_it_on_the_way_out() {
        let detail = match reject_merge_fields("a topic", "{{first_name|there}} news") {
            Err(StoreError::Validation(detail)) => detail,
            other => panic!("expected a validation error, got {other:?}"),
        };
        assert!(detail.contains("as written"), "{detail}");
        assert!(reject_merge_fields("a topic", "Monthly newsletter").is_ok());
    }

    #[test]
    fn a_code_block_is_literal_so_a_sample_of_somebody_elses_template_survives() {
        // `{{ … }}` is Handlebars, Vue, Angular, Jinja and Go. A campaign that
        // could not carry a code sample containing them would be unable to
        // document any of those, and a campaign that resolved them would send
        // somebody's documentation with the examples filled in.
        let content = body(json!([
            { "type": "code", "id": "c1", "code": "<p>{{ user.name }}</p>", "language": "html" },
        ]));
        let resolved = resolve_merge_content(&content, &values()).unwrap();
        assert_eq!(resolved, content, "a code block is never personalised");
    }

    #[test]
    fn a_body_resolves_everywhere_the_writer_can_type() {
        let content = body(json!([
            { "type": "heading", "id": "h1", "level": 1, "text": "For {{first_name|you}}" },
            { "type": "paragraph", "id": "p1", "text": "We write to {{email|your address}}." },
            { "type": "table", "id": "t1", "rows": [
                ["Customer", "Country"],
                ["{{name|a customer}}", "{{country|—}}"],
            ] },
        ]));
        let resolved = resolve_merge_content(&content, &values()).unwrap();
        let rendered = render_campaign_text(&resolved).unwrap();
        assert!(rendered.contains("For Jean"), "{rendered}");
        assert!(rendered.contains("jean.dupont@example.fr"), "{rendered}");
        assert!(rendered.contains("Jean Dupont"), "{rendered}");
        assert!(rendered.contains("FR"), "{rendered}");
        assert!(
            !rendered.contains(OPEN),
            "no placeholder may survive: {rendered}"
        );
    }

    #[test]
    fn both_parts_of_one_mail_personalise_identically_including_the_punctuation() {
        // The reason resolution is not inside the renderers. The HTML part
        // escapes on the way out, so a substitution performed after escaping
        // would turn an apostrophe into `&#39;` in one part and leave it in the
        // other — two parts of one mail disagreeing about a character.
        let apostrophe = CampaignMergeValues {
            email: "o.brien@example.ie".to_owned(),
            name: Some("O'Brien & Sons".to_owned()),
            country: None,
        };
        let content = body(json!([
            { "type": "paragraph", "id": "p1", "text": "Dear {{name|customer}}," },
        ]));
        let letter = CampaignLetter {
            subject: "A note for {{first_name|you}}",
            preheader: Some("Written to {{email|you}}"),
            content: &content,
        };
        let personalised = personalise_campaign(&letter, &apostrophe).unwrap();

        assert_eq!(personalised.subject, "A note for O'Brien");
        assert_eq!(
            personalised.preheader.as_deref(),
            Some("Written to o.brien@example.ie")
        );

        let html = render_campaign_html(&personalised.letter()).unwrap();
        let text = render_campaign_text(personalised.content()).unwrap();
        // Each part escapes for itself, from the same resolved words.
        assert!(html.contains("Dear O&#39;Brien &amp; Sons,"), "{html}");
        assert!(text.contains("Dear O'Brien & Sons,"), "{text}");
        assert!(!html.contains(OPEN) && !text.contains(OPEN));
        assert!(
            html.contains("<title>A note for O&#39;Brien</title>"),
            "{html}"
        );
    }

    #[test]
    fn personalising_refuses_a_letter_that_could_not_have_been_saved() {
        // `CampaignContent`'s fields are public, so a value can reach here
        // without passing the write gate — the same reason both renderers
        // re-validate what they are handed.
        // Built through the public fields rather than through `from_value`,
        // because the gate refuses this body — which is the point: the only way
        // to hold one is to assemble it in Rust, and that path must be refused
        // here too.
        let content = CampaignContent {
            schema_version: CAMPAIGN_CONTENT_SCHEMA_VERSION,
            blocks: vec![CampaignBlock::Paragraph(ParagraphBlock {
                id: "p1".to_owned(),
                text: "Hi {{first_name}},".to_owned(),
            })],
        };
        let letter = CampaignLetter {
            subject: "Hello",
            preheader: None,
            content: &content,
        };
        let reported = detail(
            personalise_campaign(&letter, &values()).map(|personalised| personalised.subject),
        );
        assert!(reported.contains("a paragraph: "), "{reported}");

        let empty = CampaignContent::empty();
        let bad_subject = CampaignLetter {
            subject: "Hi {{first_name}}",
            preheader: None,
            content: &empty,
        };
        let reported =
            detail(personalise_campaign(&bad_subject, &values()).map(|letter| letter.subject));
        assert!(
            reported.contains("the subject line"),
            "the writer is told which field to look at: {reported}"
        );
    }

    #[test]
    fn the_location_is_named_so_a_long_letter_says_which_field_to_look_at() {
        let detail = match validate_merge_text("a paragraph", "Hi {{first_name}}") {
            Err(StoreError::Validation(detail)) => detail,
            other => panic!("expected a validation error, got {other:?}"),
        };
        assert!(detail.starts_with("a paragraph: "), "{detail}");
    }
}
