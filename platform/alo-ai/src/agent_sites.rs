//! The **Website** tool set of the agent (ADR 0034, queue item A2.1) — the names
//! alo Sites contributes to its own agent, and the words that tell a model what
//! they take.
//!
//! The same seam every product before it uses ([`crate::agent_inventory`]): a
//! tool list carrying each tool's effect, a description block, and a paragraph
//! of guidance. Nothing here reads, writes or publishes anything — the reading
//! tools are executed inside the turn and the writes only from an approval, both
//! by `alo-jmap`'s `agent_sites`, against the caller's tenant-scoped store.
//!
//! Five rules shape the wording below, and each is a mistake it exists to
//! prevent:
//!
//! - **A question about the site is answered from what is on the internet.**
//!   `site_answer` reads the *published* site — the pages of the current
//!   publish, the live posts, the documents the owner deliberately added to the
//!   site's public knowledge (`alo_store::site_grounding`). Never the draft: a
//!   visitor asking what the opening hours are must be told what the page they
//!   can load says, not what somebody is halfway through writing.
//! - **Nothing a site tool writes is public.** A drafted page, a rewritten
//!   heading and a new description all land in the draft and stay there.
//!   Publishing is one separate tool, it changes something, and it therefore
//!   waits for the owner's tap (ADR 0047 §1) — which is the whole of "publishing
//!   is proposed, never silent".
//! - **An agent edits the words, never the wiring.** `site_page_edit` rewrites
//!   text the page already has, at a position `site_page_read` handed it. A
//!   link's target, an image's blob, a form's id and a block of custom code are
//!   not copy, are not offered, and are refused at the executor.
//! - **Nothing here invents a fact about the business.** Prices, addresses,
//!   opening hours, statistics, certifications, people and testimonials are the
//!   tenant's own; a website is the one surface where an invented one is read by
//!   strangers and believed.
//! - **Translating the site is the owner's, and the agent only says how far it
//!   got** (queue item A2.1b). `site_translation_status` counts, per language
//!   the site is set up in, how many pages already have a draft in it and how
//!   many are still missing; the translating itself is `POST
//!   /sites/:id/translation-proposals`, which shows every proposed page beside
//!   its original and keeps nothing the owner did not approve. A whole site
//!   rewritten into another language on the strength of one chat message is the
//!   change this split exists to prevent, so the tool line says outright that
//!   the model cannot do it.

use crate::agent_tool::AgentTool;

/// The Website tools, each declaring whether it reads or writes (ADR 0047 §1).
///
/// The jmap layer validates an approved tool against the union of this list and
/// every other product's ([`crate::is_agent_tool`]) and owns the execution of
/// each.
pub const SITES_TOOLS: &[AgentTool] = &[
    AgentTool::read("site_answer"),
    AgentTool::read("site_page_read"),
    AgentTool::read("site_seo_review"),
    AgentTool::read("site_translation_status"),
    AgentTool::write("site_page_draft"),
    AgentTool::write("site_page_edit"),
    AgentTool::write("site_publish"),
];

/// The description of each Website tool, spliced into the agent's system prompt
/// after the People tools ([`crate::agent`]).
///
/// Every line ends with a newline so the block concatenates into the list above
/// it without the caller knowing how many tools Sites has.
pub const SITES_TOOL_DOC: &str = "\
- site_answer: read what the user's website says ON THE INTERNET right now — the pages of the version that is actually published, the posts that are live, and the documents they put in the site's public knowledge. It reads nothing that is only a draft and changes nothing. args: {\"question\": string (what to look for, in the user's own words, REQUIRED), \"site\": string (which site, by name or address, optional — their only site when left out)}. Answer from the passages it returns and cite the page or post each one names. When it comes back with nothing published, say the site is not live yet rather than answering from anything else you can see.\n\
- site_page_read: read ONE page as it stands in the DRAFT — its title, its search-engine title and description, and every block on it with the position, the type and the exact text you may rewrite. It changes nothing. args: {\"page\": string (the page, by its title or its address, REQUIRED), \"site\": string (optional)}. This is where the position, the type and the pointer that site_page_edit needs come from: read the page before you propose an edit to it, and never work any of the three out for yourself.\n\
- site_seo_review: go through every page of the draft and report what search engines will find missing — a page with no description, a description that is too long or too short, two pages sharing a title, a page with no heading on it, a picture with no alt text. It reports; it changes nothing. args: {\"site\": string (optional)}. Report what it returns and nothing else: it counts what is on the pages, so never claim a position in anybody's results, a ranking, a keyword's difficulty or how much traffic a change would bring.\n\
- site_translation_status: read how far the site's OWN languages have got — for each language the site is set up in, how many of its pages already have a version written in that language and how many are still missing. It counts; it changes nothing and it translates nothing. args: {\"site\": string (optional)}. Report the numbers it returns and name the languages that are short. You CANNOT translate anything: whole-site translation is something the user starts on the website's Languages screen, where every proposed page is shown beside its original and nothing is kept until they approve it — say that plainly and never say you translated, are translating, or will translate a page or a site.\n\
- site_page_draft: write a NEW page into the site's draft — a heading, an opening line, and a block of text under each of its own subheadings. The page is SAVED AS A DRAFT and is NOT on the internet: it appears when the user publishes, which is a separate approval they give. It never becomes the home page and never replaces an existing page. args: {\"title\": string (REQUIRED), \"slug\": string (the address segment, lowercase letters, digits and hyphens, optional — made from the title when left out), \"seo_description\": string (the sentence search engines show, optional), \"heading\": string (the page's own headline, REQUIRED), \"intro\": string (one line under the headline, optional), \"sections\": [{\"heading\": string, \"body\": string}] (the blocks of the page, in order, optional)}.\n\
- site_page_edit: change the WORDS of a page that already exists, in the draft. It can retitle the page, set its search-engine title and description, and rewrite text that is already on it — one entry per piece of text, each naming the position, the type and the pointer site_page_read gave you. It cannot add, remove or reorder a block, and it cannot touch a link's target, an image, a form or any code on the page. Every rewrite lands in the DRAFT and is NOT on the internet until the user publishes. args: {\"page\": string (REQUIRED), \"site\": string (optional), \"title\": string (optional), \"seo_title\": string (optional), \"seo_description\": string (optional), \"copy\": [{\"index\": number, \"type\": string, \"pointer\": string, \"text\": string (the complete new wording)}] (optional)}. State at least one of them, and read the page first.\n\
- site_publish: put the site's draft ON THE INTERNET, exactly as it stands. This is the ONLY tool that makes anything public, and everything waiting in the draft goes live together — including changes somebody else made and anything you drafted earlier in this conversation. args: {\"site\": string (optional)}. Propose this only when the user asks for the site, a page or a change to go live; say what will become public in your own sentence, and never tell them anything is live until they have approved it.\n";

/// The Website paragraph of the agent's general instructions.
///
/// Kept apart from the per-tool lines above because it says what the *product*
/// is, not what a tool takes: it is the sentence that stops a model publishing
/// by implication, and the one that stops it filling a stranger's screen with
/// facts nobody at the company ever stated.
pub const SITES_GUIDANCE: &str = "For a website tool, NEVER invent a fact about the business: a price, an address, an opening time, a phone number, a delivery time, a statistic, a certification, a person or a customer's words are the user's own, and a website is read by strangers who will believe whatever is on it — ask for anything you have not been given rather than filling the gap. A page you draft or a wording you change is in the DRAFT: never say a change is live, online, updated or visible to anybody until the user has approved a publish, and say plainly that it is waiting for them. Write in the language of the page you are working on. When the user asks what their site says, read the published site rather than the draft, and cite the page you found it on. Translating the site is not yours to do: site_translation_status tells you which language is short how many pages, and the translating itself is something the user runs from the website's Languages screen and approves page by page — offer the count and point them there rather than offering to do it.\n";

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_website_tool_is_described_to_the_model() {
        for tool in SITES_TOOLS {
            assert!(
                SITES_TOOL_DOC.contains(&format!("- {}:", tool.name)),
                "{} has no description in the prompt",
                tool.name
            );
        }
        // …and nothing is described that cannot be executed.
        let described = SITES_TOOL_DOC.matches("\n- ").count() + 1;
        assert_eq!(described, SITES_TOOLS.len());
    }

    #[test]
    fn the_block_concatenates_cleanly_into_a_list() {
        assert!(SITES_TOOL_DOC.ends_with('\n'));
        assert!(SITES_TOOL_DOC.starts_with("- "));
        assert!(SITES_GUIDANCE.ends_with('\n'));
    }

    /// The sentence the whole item is named after. Publishing is one tool, it is
    /// declared a write, and every other tool's line says where its result
    /// actually lands — so a model that read only this block still cannot
    /// believe it put something on the internet.
    #[test]
    fn publishing_is_one_tool_that_waits_and_every_other_tool_says_it_did_not_publish() {
        assert!(crate::is_read_tool("site_answer"));
        assert!(crate::is_read_tool("site_page_read"));
        assert!(crate::is_read_tool("site_seo_review"));
        for changes in ["site_page_draft", "site_page_edit", "site_publish"] {
            assert!(!crate::is_read_tool(changes), "{changes}");
        }
        let line = |name: &str| {
            SITES_TOOL_DOC
                .lines()
                .find(|line| line.starts_with(&format!("- {name}:")))
                .expect("the tool is described")
                .to_owned()
        };
        assert!(line("site_page_draft").contains("NOT on the internet"));
        assert!(line("site_page_edit").contains("NOT on the internet"));
        assert!(line("site_publish").contains("ONLY tool that makes anything public"));
        assert!(SITES_GUIDANCE.contains("never say a change is live"));
    }

    /// A question about the site is answered from what a visitor can load, and
    /// the description says so — the draft is not the site.
    #[test]
    fn the_answering_tool_reads_the_published_site_and_not_the_draft() {
        let line = SITES_TOOL_DOC
            .lines()
            .find(|line| line.starts_with("- site_answer:"))
            .expect("site_answer is described");
        assert!(line.contains("ON THE INTERNET"), "{line}");
        assert!(line.contains("nothing that is only a draft"), "{line}");
        assert!(line.contains("say the site is not live yet"), "{line}");
        assert!(SITES_GUIDANCE.contains("read the published site rather than the draft"));
    }

    /// The editing pair, stated in the prompt: positions and pointers come from
    /// the read, never from the model. A guessed index is how an edit lands on
    /// the wrong block.
    #[test]
    fn an_edit_names_a_position_the_read_tool_gave_it() {
        let read = SITES_TOOL_DOC
            .lines()
            .find(|line| line.starts_with("- site_page_read:"))
            .expect("site_page_read is described");
        assert!(read.contains("never work any of the three out for yourself"));
        let edit = SITES_TOOL_DOC
            .lines()
            .find(|line| line.starts_with("- site_page_edit:"))
            .expect("site_page_edit is described");
        assert!(edit.contains("site_page_read gave you"), "{edit}");
        assert!(edit.contains("read the page first"), "{edit}");
        // The wiring an edit may not touch, named where the model reads it.
        assert!(edit.contains("cannot touch a link's target"), "{edit}");
        assert!(
            edit.contains("cannot add, remove or reorder a block"),
            "{edit}"
        );
    }

    /// The one mistake a website makes that no reviewer downstream catches: a
    /// fact nobody at the company ever stated, published for strangers to read.
    #[test]
    fn nothing_here_offers_the_model_a_fact_to_make_up() {
        assert!(SITES_GUIDANCE.contains("NEVER invent a fact about the business"));
        for named in ["a price", "an address", "an opening time", "a statistic"] {
            assert!(SITES_GUIDANCE.contains(named), "{named}");
        }
        // The drafting tool takes headings and prose and nothing that would
        // carry a claim in structured form — no prices, no people, no quotes.
        let draft = SITES_TOOL_DOC
            .lines()
            .find(|line| line.starts_with("- site_page_draft:"))
            .expect("site_page_draft is described");
        for forbidden in ["\"price\"", "\"tiers\"", "\"testimonials\"", "\"members\""] {
            assert!(!draft.contains(forbidden), "{forbidden}");
        }
    }

    /// The item's own sentence (A2.1b): the agent reports how far the languages
    /// got and cannot translate. A model that read only this block still knows
    /// the translating is the user's, and where they do it.
    #[test]
    fn the_translation_tool_counts_and_says_it_cannot_translate() {
        assert!(crate::is_read_tool("site_translation_status"));
        let line = SITES_TOOL_DOC
            .lines()
            .find(|line| line.starts_with("- site_translation_status:"))
            .expect("site_translation_status is described");
        assert!(line.contains("how many are still missing"), "{line}");
        assert!(line.contains("changes nothing"), "{line}");
        assert!(line.contains("CANNOT translate anything"), "{line}");
        assert!(line.contains("Languages screen"), "{line}");
        assert!(
            line.contains("never say you translated"),
            "the tense a model reaches for first is the one to forbid: {line}"
        );
        assert!(SITES_GUIDANCE.contains("Translating the site is not yours to do"));
        // And no write anywhere in the set claims the language work: a tool that
        // translated would have to be declared one, and none is.
        for tool in SITES_TOOLS {
            assert!(
                !tool.name.contains("translate"),
                "{} would be a second translation path",
                tool.name
            );
        }
    }

    /// A review reports what is on the page. Everything else a search-engine
    /// tool could be asked for — a ranking, a position, traffic — is a claim
    /// about somebody else's index that we cannot see and would not be true.
    #[test]
    fn the_review_never_claims_a_ranking() {
        let line = SITES_TOOL_DOC
            .lines()
            .find(|line| line.starts_with("- site_seo_review:"))
            .expect("site_seo_review is described");
        assert!(line.contains("never claim a position"), "{line}");
        assert!(line.contains("how much traffic"), "{line}");
    }
}
