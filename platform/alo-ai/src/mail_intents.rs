//! alo Mail's verbs (ADR 0058, AC.4) — the mailbox, the address book, and
//! nothing that sends by itself.
//!
//! This is the whole of what the Mail agent may do, and the words a model
//! reads about it. The executors live beside Mail's routes in `alo-jmap`
//! (`mail_intents.rs`), through the asker's tenant-scoped store. The address
//! book is Mail's too: `find_contact` answers "what is Ben's address" from
//! the caller's OWN contacts — a personal address book, never a company
//! directory — and it is deliberately the only contacts verb, because an
//! address book an agent could write to is one a misheard name can quietly
//! corrupt.
//!
//! Mail's app surface is JMAP (RFC 8620/8621) — methods on `/jmap/api`, not
//! REST routes — so most verbs here name no route: they execute against the
//! same store those methods serve, and the coverage test holds the few
//! mail-side HTTP routes there are (`/contacts*`, the autoconfig file) to a
//! verb or a reason.
//!
//! The rules the old tool set was written around hold unchanged — each is a
//! way an agent's mail could otherwise go wrong:
//!
//! - **Nothing is ever sent by itself.** `draft_email` and `draft_reply`
//!   only ever land in the user's Drafts; `send_email` delivers a draft
//!   that ALREADY exists, only when the user clearly asked to send, and
//!   only once they approve — and it cannot be undone, which is why its
//!   preview says so.
//! - **A correspondent is answered from the correspondence.** A question
//!   about a person or company goes through `correspondence`, never a
//!   search snippet that merely mentions them; a message the result marks
//!   `"opened": false` was listed and NOT read.
//! - **Several matches are reported, never resolved.** Two people called
//!   Ben is the normal case, and a tool that picked one would put the
//!   wrong address in a message somebody then sends.
//!
//! What AC.4 adds is the mailbox as a *subject*: what waits unread and
//! where (`unread_summary`), one message's whole conversation
//! (`thread_lookup`), and who the asker's own mail went to lately
//! (`who_i_emailed`).

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const SOURCE: Arg = Arg::required(
    "source",
    "number",
    "the number [n] of that email in the numbered sources above",
);

/// The verbs.
pub const MAIL_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "correspondence",
        purpose: "The exchange with ONE person or company, both ways and newest first — who wrote, when, whether it came from them or from you, and whether the last word was theirs or yours. It reads; it changes nothing. The newest few messages come back with their text; the rest are listed with \"opened\": false — listed and NOT read. Use it for any question about being in contact with somebody, who replied last, or what has been said to them.",
        effect: Effect::Read,
        args: &[
            Arg::required("who", "text", "a name, an email address or a company"),
            Arg::optional(
                "about",
                "text",
                "narrows the exchange to the messages whose words match it",
            ),
            Arg::optional("limit", "number", "1-15, 8 when unsaid"),
        ],
        answers: &[
            "are we in contact with {who}",
            "who last replied to them",
            "what did we promise them about delivery",
        ],
        preview: None,
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "message_read",
        purpose: "The full text of ONE message, with everyone it was addressed to and what is attached to it. It reads; it changes nothing. Use it when a preview is not enough to say exactly what was said or promised. Never invent an id: only one that came back in another result will work.",
        effect: Effect::Read,
        args: &[Arg::required(
            "message",
            "text",
            "the \"id\" of a message in a result you have already been given",
        )],
        answers: &[
            "what exactly did they write",
            "open that message",
            "what is attached to it",
        ],
        preview: None,
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "unread_summary",
        purpose: "What waits unread, folder by folder — each folder by name with its unread and total counts, the Inbox always included even when it is empty. It reads; it changes nothing. Drafts, Sent and the internal folders are not waiting mail and are not counted.",
        effect: Effect::Read,
        args: &[],
        answers: &[
            "how much unread mail do I have",
            "what is waiting in my inbox",
            "is anything unread outside the inbox",
        ],
        preview: None,
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "thread_lookup",
        purpose: "ONE message's whole conversation, oldest first — every message of its thread with who wrote it, when, and whose side it came from — so a question about how an exchange went is answered from the thread rather than from one message of it. It reads; it changes nothing. Never invent an id: only one that came back in another result will work.",
        effect: Effect::Read,
        args: &[Arg::required(
            "message",
            "text",
            "the \"id\" of any message of the conversation, from a result you have already been given",
        )],
        answers: &[
            "how did that conversation go",
            "what came before this email",
            "did anyone reply to it",
        ],
        preview: None,
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "who_i_emailed",
        purpose: "Who this person's own mail went to lately — every address their sent mail was addressed to, most recent first, each with how many messages and the last subject. It reads; it changes nothing. An empty answer means nothing was sent in the period, which is an answer too.",
        effect: Effect::Read,
        args: &[Arg::optional(
            "days",
            "number",
            "how far back to look, 1-31 — a week when unsaid",
        )],
        answers: &[
            "who did I email last week",
            "who have I written to lately",
            "who have I been in touch with this month",
        ],
        preview: None,
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "find_contact",
        purpose: "Look somebody up in this person's OWN address book — a personal address book, never a company directory. It reads; it changes nothing. If more than one person matches, say so and name them rather than choosing one: two people with the same first name is ordinary, and picking the wrong one puts the wrong address in whatever is written next.",
        effect: Effect::Read,
        args: &[
            Arg::required(
                "query",
                "text",
                "the name, address or company the user said",
            ),
            Arg::optional("limit", "number", "at most 10"),
        ],
        answers: &[
            "what is the address for {query}",
            "who is {query}",
            "what is the number for {query}",
        ],
        preview: None,
        undo: None,
        routes: &["/contacts"],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "mark_read",
        purpose: "Mark ONE email read, or unread. It only proposes: nothing changes until the user approves.",
        effect: Effect::Write,
        args: &[
            SOURCE,
            Arg::optional(
                "read",
                "boolean",
                "false to mark it unread — read when unsaid",
            ),
        ],
        answers: &["mark that as read", "mark it unread again"],
        preview: Some(
            "Email [{source}] will be marked read — or back to unread, when that is what was asked.",
        ),
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "flag_email",
        purpose: "Flag (star) ONE email, or unflag it. It only proposes: nothing changes until the user approves.",
        effect: Effect::Write,
        args: &[
            SOURCE,
            Arg::optional(
                "flagged",
                "boolean",
                "false to take the flag off — flagged when unsaid",
            ),
        ],
        answers: &["flag that email", "star it so I find it back"],
        preview: Some(
            "Email [{source}] will be flagged — or unflagged, when that is what was asked.",
        ),
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "archive_email",
        purpose: "Move ONE email out of the inbox into Archive. It only proposes: nothing moves until the user approves.",
        effect: Effect::Write,
        args: &[SOURCE],
        answers: &["archive that", "get it out of my inbox"],
        preview: Some("Email [{source}] will move out of the inbox into Archive."),
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "trash_email",
        purpose: "Move ONE email to Trash — out of the inbox and the archive. It only proposes: nothing moves until the user approves.",
        effect: Effect::Write,
        args: &[SOURCE],
        answers: &["delete that email", "bin it"],
        preview: Some("Email [{source}] will move to Trash."),
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "snooze_email",
        purpose: "Hide ONE email from the inbox until a chosen time, when it returns to the inbox unread. It only proposes: nothing moves until the user approves.",
        effect: Effect::Write,
        args: &[
            SOURCE,
            Arg::required(
                "until",
                "text",
                "an RFC 3339 datetime in the future, e.g. \"2026-09-07T09:00:00Z\"",
            ),
        ],
        answers: &[
            "snooze it until Monday morning",
            "hide that until after the holidays",
        ],
        preview: Some("Email [{source}] will leave the inbox and return unread at {until}."),
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "draft_email",
        purpose: "Write a NEW email and save it to the user's Drafts for them to review and send — it is NEVER sent automatically. It only proposes: no draft is saved until the user approves. Compose the body from the request; do not invent facts. The sender is always the user's own address — never set it.",
        effect: Effect::Write,
        args: &[
            Arg::required("to", "text", "the recipient's email address"),
            Arg::optional("subject", "text", "the subject line"),
            Arg::required("body", "text", "the message, composed from the request"),
        ],
        answers: &[
            "draft an email to {to} about the delivery",
            "write to {to} asking for the invoice",
        ],
        preview: Some(
            "A draft to {to} will be saved in your Drafts — nothing is sent until you send it yourself.",
        ),
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "draft_reply",
        purpose: "Write a reply to an email in the sources and save it to the user's Drafts — NEVER sent automatically. It only proposes: no draft is saved until the user approves. The reply goes to that email's sender and keeps its subject thread; compose the body from the request, do not invent facts.",
        effect: Effect::Write,
        args: &[
            SOURCE,
            Arg::required("body", "text", "the reply, composed from the request"),
        ],
        answers: &[
            "reply that Friday works",
            "answer them that the price stands",
        ],
        preview: Some(
            "A reply to email [{source}] will be saved in your Drafts — nothing is sent until you send it yourself.",
        ),
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "send_email",
        purpose: "SEND a message that is ALREADY in the user's Drafts. This delivers it to its recipients and CANNOT be undone. Only propose this when the user clearly and explicitly asks to send, and only for a draft that already exists — if there is no draft yet, write one first with draft_email or draft_reply and let the user send it. The user still approves before anything is sent.",
        effect: Effect::Write,
        args: &[SOURCE],
        answers: &["send that draft", "yes, send it"],
        preview: Some(
            "Draft [{source}] \"{subject}\" will be SENT to everyone it is addressed to — sending cannot be undone.",
        ),
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "move_to_folder",
        purpose: "Move ONE email into one of the user's own mail folders. It only proposes: nothing moves until the user approves. Set \"folder\" to EXACTLY one of the folder names listed under \"Folders\" below — never invent a folder. If the user names a folder that is not in that list, ANSWER instead and say that folder does not exist. Prefer archive_email for Archive and trash_email for Trash.",
        effect: Effect::Write,
        args: &[
            SOURCE,
            Arg::required(
                "folder",
                "text",
                "one of the folder names listed under \"Folders\"",
            ),
        ],
        answers: &["file that under {folder}", "move it to the {folder} folder"],
        preview: Some("Email [{source}] will move into {folder}."),
        undo: None,
        routes: &[],
    },
];

/// The mail-side HTTP routes deliberately without a verb, each with its
/// reason. Mail's own app surface is JMAP, so this list is short — the
/// address book's bulk moves and the file a mail client fetches on setup.
pub const MAIL_EXCLUDED: &[Excluded] = &[
    Excluded {
        route: "/contacts/import",
        why: "Importing a vCard file is the screen's own upload — a file the person picks, not a sentence an agent should write.",
    },
    Excluded {
        route: "/contacts/export",
        why: "Exporting the address book is the person's own download of their own data; the agent answers questions about contacts, it does not hand the book around.",
    },
    Excluded {
        route: "/mail/config-v1.1.xml",
        why: "Serves a mail client its autoconfiguration file on setup; it is read by software, not asked for in a conversation.",
    },
];

/// The Mail paragraph of the agent's general instructions.
///
/// It carries the three rules the whole module rests on: an email action
/// names its source, a correspondent is answered from the correspondence,
/// and nothing is ever sent by itself — plus the address book's own rules,
/// because the address book is Mail's.
pub const MAIL_GUIDANCE: &str = "For any tool that acts on an email, set \"source\" to the number [n] of that email in the numbered sources above; only propose it when the relevant email is present in the sources. Answer a question about a person or a company from their correspondence, never from a source that merely mentions them: look the exchange up first, then say which message you are relying on by its subject and its date. A message a result marks \"opened\": false was listed and NOT read — you may say that it exists and when it arrived, and never what it says; read it with message_read if the question turns on its contents. Nothing is ever sent by itself: a draft waits in the user's Drafts, and send_email delivers only a draft that already exists, only when the user clearly asked to send, and only once they approve. For find_contact, pass the user's own words through as the query and never invent a surname, a company or an address to narrow it. It searches the user's PERSONAL address book, not a company directory: if nobody matches, say the address book has no such person rather than guessing at a colleague; if more than one person matches, name them all and ask rather than choosing. Never state a contact detail that did not come from a tool result.\n";

/// The module, as the registry reads it.
pub static MAIL: IntentModule = IntentModule {
    intents: MAIL_INTENTS,
    excluded: MAIL_EXCLUDED,
    guidance: MAIL_GUIDANCE,
};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The verbs whose surface is not an HTTP route of this module — Mail's
    /// app surface is JMAP (`/jmap/api` methods), so every mailbox verb
    /// executes against the store those methods serve. Named, so a new verb
    /// with an empty route list fails the test instead of joining them
    /// silently. `find_contact` is the one verb with a route: the address
    /// book's listing.
    const ROUTELESS: &[&str] = &[
        "correspondence",
        "message_read",
        "unread_summary",
        "thread_lookup",
        "who_i_emailed",
        "mark_read",
        "flag_email",
        "archive_email",
        "trash_email",
        "snooze_email",
        "draft_email",
        "draft_reply",
        "send_email",
        "move_to_folder",
    ];

    #[test]
    fn every_verb_has_a_route_a_purpose_and_a_question_it_answers() {
        for intent in MAIL_INTENTS {
            assert!(
                !intent.routes.is_empty() || ROUTELESS.contains(&intent.name),
                "{} names no route",
                intent.name
            );
            assert!(
                intent.purpose.ends_with('.'),
                "{} purpose is not a sentence",
                intent.name
            );
            assert!(
                !intent.answers.is_empty(),
                "{} answers nothing",
                intent.name
            );
            if intent.effect == Effect::Write {
                assert!(
                    intent.preview.is_some(),
                    "{} is a write without a preview",
                    intent.name
                );
            }
        }
    }

    #[test]
    fn names_are_unique_and_the_doc_lists_each_once() {
        let mut names: Vec<&str> = MAIL_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), MAIL_INTENTS.len());
        let doc = MAIL.doc();
        for intent in MAIL_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(MAIL_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn an_exclusion_is_a_sentence_not_a_shrug() {
        for excluded in MAIL_EXCLUDED {
            assert!(
                excluded.why.ends_with('.'),
                "{}: {}",
                excluded.route,
                excluded.why
            );
            assert!(
                !MAIL_INTENTS
                    .iter()
                    .any(|i| i.routes.contains(&excluded.route)),
                "{} is both a verb's route and excluded",
                excluded.route
            );
        }
    }

    /// Every read says, where the model reads it, that it changes nothing —
    /// so a turn never offers a button for an answer.
    #[test]
    fn the_reads_say_they_change_nothing() {
        for intent in MAIL_INTENTS.iter().filter(|i| i.effect == Effect::Read) {
            assert!(
                intent.purpose.contains("changes nothing"),
                "{} does not say it changes nothing",
                intent.name
            );
        }
    }

    /// The rule the whole module rests on: nothing leaves the server by
    /// itself. A draft is never sent automatically, and the one verb that
    /// sends is narrow — a draft that already exists, an explicit ask, an
    /// approval — and says it cannot be undone where the model reads it.
    #[test]
    fn nothing_is_ever_sent_by_itself() {
        for name in ["draft_email", "draft_reply"] {
            let draft = MAIL.find(name).unwrap();
            assert_eq!(draft.effect, Effect::Write);
            assert!(
                draft.purpose.contains("NEVER sent automatically"),
                "{name} does not say it never sends"
            );
        }
        let send = MAIL.find("send_email").unwrap();
        assert_eq!(send.effect, Effect::Write);
        assert!(send.purpose.contains("ALREADY in the user's Drafts"));
        assert!(send.purpose.contains("CANNOT be undone"));
        assert!(send.purpose.contains("clearly and explicitly"));
        // The queue's own words: a send is previewed with its recipients and
        // its subject — the subject fills from the resolved source, and the
        // recipients are the draft's own, said in so many words.
        let preview = send.preview.unwrap();
        assert!(preview.contains("{subject}"));
        assert!(preview.contains("everyone it is addressed to"));
        assert!(preview.contains("cannot be undone"));
        assert!(MAIL_GUIDANCE.contains("Nothing is ever sent by itself"));
    }

    /// The address book is read and never written: `find_contact` is the
    /// only contacts verb, several matches are reported rather than
    /// resolved, and the model is forbidden from inventing a detail.
    #[test]
    fn the_address_book_is_read_never_written_and_never_guessed() {
        let find = MAIL.find("find_contact").unwrap();
        assert_eq!(find.effect, Effect::Read);
        assert!(find.purpose.contains("rather than choosing one"));
        assert!(find.purpose.contains("never a company directory"));
        assert!(
            MAIL_INTENTS
                .iter()
                .filter(|i| i.name.contains("contact"))
                .all(|i| i.effect == Effect::Read),
            "a verb could write to the address book"
        );
        assert!(MAIL_GUIDANCE.contains("Never state a contact detail"));
        assert!(MAIL_GUIDANCE.contains("PERSONAL address book"));
    }

    /// An email action names its source, and a folder is never invented —
    /// the two rules that keep a proposal about the mail that is actually
    /// there.
    #[test]
    fn an_action_names_its_source_and_never_invents_a_folder() {
        assert!(MAIL_GUIDANCE.contains("set \"source\""));
        assert!(MAIL_GUIDANCE.contains("\"opened\": false"));
        let mover = MAIL.find("move_to_folder").unwrap();
        assert!(mover.purpose.contains("never invent a folder"));
        assert!(mover.purpose.contains("\"Folders\""));
        for intent in MAIL_INTENTS.iter().filter(|i| i.effect == Effect::Write) {
            let named_source = intent.args.iter().any(|arg| arg.name == "source");
            assert!(
                named_source || intent.name == "draft_email",
                "{} acts on an email without naming its source",
                intent.name
            );
        }
    }
}
