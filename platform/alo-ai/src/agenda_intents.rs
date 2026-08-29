//! alo Agenda's verbs (ADR 0058, queue item AB.5) — the whole of what the
//! Agenda agent may do, and the words a model reads about it.
//!
//! Nothing here reads or writes a diary: the executors live in `alo-jmap`
//! (`agenda_intents.rs`, with the kept executors in `agent_reads.rs`,
//! `agent_agenda.rs` and `agent_meeting.rs`), through the asker's
//! tenant-scoped store — a diary that was never shared with the asker is not
//! among the things that can be named.
//!
//! The rules the hand-written tool set learned, kept because each one is a
//! mistake it exists to prevent:
//!
//! - **A day is a date, never a phrase.** "Thursday" and "next week" are
//!   resolved by the model against today's date, which the prompt supplies,
//!   and arrive here as `YYYY-MM-DD`. A tool that accepted "next Thursday"
//!   would have to guess a week boundary and a timezone, and would guess
//!   differently from the person asking.
//! - **Busy is not the same as unavailable.** `am_i_free` reports what
//!   overlaps a window; it does not decide whether the person may be
//!   interrupted.
//! - **A diary you cannot see is never reported as free.** `find_a_time` and
//!   `colleague_free` look only at the diaries already shared with the person
//!   asking; a colleague whose calendar is not among them is a named refusal,
//!   never an empty (and therefore "free") day.
//! - **A meeting is named, and a day disambiguates it.** No verb that acts on
//!   one existing meeting takes an identifier: the model passes the words the
//!   user said, and a title that matches several sittings comes back listing
//!   their days rather than picking the next one.
//! - **Moving a meeting keeps its length**, and one sitting of a series moves
//!   on its own.
//! - **Cancelling and answering an invitation are the user's word, not the
//!   agent's guess.** AB.5 adds `cancel_event` and `respond_to_invitation`,
//!   and both wait for a tap: taking a meeting out of people's diaries, or
//!   telling an organizer yes or no, is exactly what a calendar agent must
//!   not do on a hunch.

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const MEETING_ARG: Arg = Arg::required("meeting", "text", "its title, in the user's own words");
const ON_ARG: Arg = Arg::optional(
    "on",
    "date",
    "\"YYYY-MM-DD\", which day's sitting — needed when the same meeting is in the diary more than once",
);

/// The verbs.
pub const AGENDA_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "whats_on",
        purpose: "What is in the user's own calendar over a range of days. It changes nothing. Use it whenever the user asks what they have on, what their day or week looks like, or when something is — and never answer such a question from the sources: what is in the diary is in the diary, and a document that mentions a meeting is not evidence it is still happening. Every day arrives as a real date in \"YYYY-MM-DD\", never a phrase. A range covers at most 31 days.",
        effect: Effect::Read,
        args: &[
            Arg::required("from", "date", "\"YYYY-MM-DD\", the first day"),
            Arg::optional(
                "to",
                "date",
                "\"YYYY-MM-DD\", the last day, included — the same day as from when left out",
            ),
        ],
        answers: &["what have I got on Thursday", "what does my week look like"],
        preview: None,
        undo: None,
        routes: &["/calendar/events"],
    },
    IntentSpec {
        name: "am_i_free",
        purpose: "Whether anything already overlaps a specific span of the user's own time. It changes nothing. Use it BEFORE create_event whenever the user asks to book something at a particular time, so a new meeting is not proposed on top of one they already have. It reports what clashes; it does not decide whether they can be interrupted — a meeting they would happily leave and one they would not look identical in a diary.",
        effect: Effect::Read,
        args: &[
            Arg::required(
                "start",
                "text",
                "RFC 3339 datetime, e.g. \"2026-08-13T14:00:00Z\"",
            ),
            Arg::optional(
                "end",
                "text",
                "RFC 3339 — one hour after start when left out",
            ),
        ],
        answers: &["am I free tomorrow at two"],
        preview: None,
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "find_a_time",
        purpose: "The free slots several people share, over a range of days. It changes nothing and books nothing. It looks only at diaries already shared with the person asking: a colleague whose calendar is not shared comes back under couldNotCheck, and the slots are then free for the others only — say so, by name, rather than presenting them as free for everybody. The working window is UTC, so convert the user's working day into UTC yourself using the timezone given below. A range covers at most 31 days.",
        effect: Effect::Read,
        args: &[
            Arg::optional(
                "people",
                "array",
                "the colleagues to include, each named the way the user named them — a first name or an email address; just the user themselves when left out",
            ),
            Arg::required("from", "date", "\"YYYY-MM-DD\", the first day to look at"),
            Arg::optional(
                "to",
                "date",
                "\"YYYY-MM-DD\", the last day, included — the same day as from when left out",
            ),
            Arg::optional(
                "minutes",
                "integer",
                "how long the meeting needs to be, 30 by default, at most 480",
            ),
            Arg::optional("earliest", "text", "\"HH:MM\" UTC, 09:00 by default"),
            Arg::optional("latest", "text", "\"HH:MM\" UTC, 17:00 by default"),
        ],
        answers: &["when can Ben and I meet for an hour this week"],
        preview: None,
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "meeting_prep",
        purpose: "What a meeting already in the diary is about — the meeting itself, and the emails and attachments that go with it. It changes nothing. Use it before writing an agenda, a briefing or a set of talking points, and write them from what comes back rather than from the meeting's title. A title that matches several sittings comes back listing their days: ask which one, never guess.",
        effect: Effect::Read,
        args: &[MEETING_ARG, ON_ARG],
        answers: &["what do I need for the Delaunay review"],
        preview: None,
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "event_lookup",
        purpose: "ONE meeting in full, by its title — when it is, where, its notes, its guests and who of them has replied, whether it repeats and its reminder. It changes nothing. Use it when the user asks about one meeting's details; for a briefing with the mail that goes with it, meeting_prep is the read. A title that matches several sittings comes back listing their days: ask which, never guess.",
        effect: Effect::Read,
        args: &[MEETING_ARG, ON_ARG],
        answers: &[
            "when is the board review",
            "who has replied to the launch dinner",
        ],
        preview: None,
        undo: None,
        routes: &["/calendar/events/{id}"],
    },
    IntentSpec {
        name: "colleague_free",
        purpose: "Whether ONE colleague already has something over a specific span of time, from the diaries ALREADY shared with the user — a colleague whose diary is not shared is a named refusal, never reported free, and the refusal says nothing about whether that person exists. It changes nothing. Name them the way the user did — a first name or an email address.",
        effect: Effect::Read,
        args: &[
            Arg::required(
                "person",
                "text",
                "the colleague, named the way the user named them — a first name or an email address",
            ),
            Arg::required(
                "start",
                "text",
                "RFC 3339 datetime, e.g. \"2026-08-13T14:00:00Z\"",
            ),
            Arg::optional(
                "end",
                "text",
                "RFC 3339 — one hour after start when left out",
            ),
        ],
        answers: &[
            "is Ben free tomorrow at two",
            "what does Marta have on Friday morning",
        ],
        preview: None,
        undo: None,
        routes: &[],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "create_event",
        purpose: "Schedule a calendar event in the user's own diary. Check the time with am_i_free BEFORE proposing it, so it does not land on top of one they already have, and never say a meeting is booked until the user has approved it.",
        effect: Effect::Write,
        args: &[
            Arg::required("title", "text", "what the meeting is"),
            Arg::required(
                "start",
                "text",
                "RFC 3339 datetime, e.g. \"2026-08-07T14:00:00Z\"",
            ),
            Arg::optional(
                "end",
                "text",
                "RFC 3339 — one hour after start when left out",
            ),
            Arg::optional("location", "text", "where"),
            Arg::optional("notes", "text", "one or two sentences of context"),
        ],
        answers: &["book a review with legal on Thursday at 10"],
        preview: Some("\"{title}\" will be put in your diary, starting {start}."),
        undo: None,
        routes: &["/calendar/events"],
    },
    IntentSpec {
        name: "reschedule_event",
        purpose: "MOVE a meeting that is already in a diary to a new time. It changes nothing else about it — not its title, its guests, its place or its notes — and it cannot cancel one. The meeting keeps its current length when no end is given; one sitting of a repeating meeting is moved on its own and the rest of the series stays where it is. A title that matches several sittings comes back listing their days: ask which, never guess. Check the new time with find_a_time or am_i_free BEFORE proposing it, and never say a meeting has been moved until the user has approved it.",
        effect: Effect::Write,
        args: &[
            MEETING_ARG,
            ON_ARG,
            Arg::required("start", "text", "the new start, RFC 3339"),
            Arg::optional(
                "end",
                "text",
                "RFC 3339 — the meeting keeps its current length when left out",
            ),
        ],
        answers: &["move the Delaunay review to Thursday at 2"],
        preview: Some(
            "\"{meeting}\" will be moved to start at {start}; nothing else about it changes.",
        ),
        undo: None,
        routes: &["/calendar/events/{id}"],
    },
    IntentSpec {
        name: "cancel_event",
        purpose: "CANCEL a meeting that is in the user's own diary: it is taken out, and every guest is emailed a cancellation. Propose it only when the user SAID to cancel — a meeting that looks abandoned is not one. One sitting of a repeating meeting is cancelled on its own and the rest of the series stays; a title that matches several sittings comes back listing their days: ask which, never guess. There is no undo — a cancelled meeting is re-created, not restored — so the preview has to name the right one.",
        effect: Effect::Write,
        args: &[MEETING_ARG, ON_ARG],
        answers: &["cancel Friday's standup", "call off the launch dinner"],
        preview: Some(
            "\"{meeting}\" will be cancelled — taken out of the diary, and every guest told.",
        ),
        undo: None,
        routes: &["/calendar/events/{id}"],
    },
    IntentSpec {
        name: "respond_to_invitation",
        purpose: "Answer an invitation that arrived in the user's mail: their reply is emailed to the organizer, and the meeting lands in their diary unless they declined. Pass the answer the user actually gave — accepted, declined or tentative — and never choose one for them. An invitation is named by the meeting's title; one that matches several comes back listing them: ask which, never guess.",
        effect: Effect::Write,
        args: &[
            MEETING_ARG,
            Arg::required(
                "response",
                "text",
                "one of \"accepted\", \"declined\", \"tentative\" — the user's own answer",
            ),
        ],
        answers: &[
            "accept the invitation to the sales kickoff",
            "decline the vendor demo",
        ],
        preview: Some(
            "Your answer — {response} — will be sent to the organizer of \"{meeting}\", and the meeting goes in your diary unless you declined.",
        ),
        undo: None,
        routes: &["/calendar/rsvp"],
    },
];

/// The routes deliberately kept from the agent, each with its reason — the
/// other half of the coverage test in `alo-jmap`'s `agenda_intents.rs`.
pub const AGENDA_EXCLUDED: &[Excluded] = &[
    Excluded {
        route: "/calendar/cancel",
        why: "Applies an organizer's emailed cancellation to the diary; that is the mail client's own tap on the message it arrived in.",
    },
    Excluded {
        route: "/calendar/apply-reply",
        why: "Records a guest's emailed reply on the organizer's event; the mail client applies it when the reply is opened.",
    },
    Excluded {
        route: "/calendar/calendars",
        why: "Diaries themselves are made and named by people; the agent works in the ones that exist.",
    },
    Excluded {
        route: "/calendar/calendars/{id}",
        why: "Renaming a diary is a person's own act, and removing one takes every meeting in it — nothing here deletes a diary.",
    },
    Excluded {
        route: "/calendar/calendars/{id}/grants",
        why: "Sharing a diary is a person's own act of trust; nothing here widens who can see whose day.",
    },
    Excluded {
        route: "/calendar/groups",
        why: "Lists the tenant's groups for the share dialog; sharing stays a person's own act.",
    },
    Excluded {
        route: "/calendar/freebusy",
        why: "The scheduling row's tenant-wide busy spans; the agent looks across diaries only where they are shared, through find_a_time and colleague_free.",
    },
    Excluded {
        route: "/calendar/working-hours",
        why: "A person's working schedule — days, hours, zone — is theirs to set in Agenda's settings; scheduling already reads its effect through free/busy.",
    },
];

/// The Agenda paragraph of the agent's general instructions.
pub const AGENDA_GUIDANCE: &str = "For an Agenda verb, resolve every relative day (today, tomorrow, Thursday, next week) against today's date given below and pass a real calendar date — never a phrase, and never a day you were not given enough to work out. If the user's meaning is ambiguous (\"Friday\" when it is already Friday), ANSWER and ask which they mean rather than choosing. Ask what is on the calendar with whats_on rather than inferring it from the sources; a document that mentions a meeting is not proof it is still in the diary. Name a meeting and a colleague the way the user did — there is no identifier for either that you could know, so never invent one. You can see only the diaries that have been shared with this person: a colleague find_a_time could not check is NOT free, and an answer that leaves them out has to say whose diary was not read. Propose cancel_event only when the user said to cancel, and respond_to_invitation only with the answer they gave — never either on your own reading of the diary. Never say a meeting has been booked, moved, cancelled or answered, or that everyone is free, until the tool has actually said so.\n";

/// The module, as the registry reads it.
pub static AGENDA: IntentModule = IntentModule {
    intents: AGENDA_INTENTS,
    excluded: AGENDA_EXCLUDED,
    guidance: AGENDA_GUIDANCE,
};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_has_a_purpose_a_question_and_a_write_its_preview() {
        for intent in AGENDA_INTENTS {
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
        for excluded in AGENDA_EXCLUDED {
            assert!(
                excluded.route.starts_with("/calendar"),
                "{} is not a calendar route",
                excluded.route
            );
            assert!(
                excluded.why.ends_with('.'),
                "{} has no sentence for a reason",
                excluded.route
            );
        }
    }

    #[test]
    fn names_are_unique_and_the_doc_lists_each_once() {
        let mut names: Vec<&str> = AGENDA_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), AGENDA_INTENTS.len());
        let doc = AGENDA.doc();
        for intent in AGENDA_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(AGENDA_GUIDANCE.ends_with('\n'));
    }

    /// The six reads answer inside the turn and say so; the four writes wait.
    /// Named explicitly rather than counted, so a fifth write slipped into the
    /// list fails this test instead of passing it (the rule the old tool set
    /// carried as `nothing_but_creating_and_moving_a_meeting_can_change_a_diary`).
    #[test]
    fn the_reads_answer_and_every_write_waits_for_a_tap() {
        let find = |name: &str| AGENDA.find(name).unwrap_or_else(|| panic!("{name}"));
        for read in [
            "whats_on",
            "am_i_free",
            "find_a_time",
            "meeting_prep",
            "event_lookup",
            "colleague_free",
        ] {
            assert_eq!(find(read).effect, Effect::Read, "{read}");
            assert!(find(read).purpose.contains("changes nothing"), "{read}");
        }
        for intent in AGENDA_INTENTS {
            assert_eq!(
                intent.effect == Effect::Write,
                matches!(
                    intent.name,
                    "create_event" | "reschedule_event" | "cancel_event" | "respond_to_invitation"
                ),
                "{} is on the wrong side of the read/write split",
                intent.name
            );
        }
        assert!(AGENDA_GUIDANCE.contains("Never say a meeting has been booked"));
    }

    /// A2.6's first rule, in the words the model reads it in: a diary that
    /// could not be read is never a free one, in both verbs that look past the
    /// asker's own — and the guidance forbids passing off a partial answer as
    /// a whole.
    #[test]
    fn a_diary_that_could_not_be_read_is_never_reported_as_free() {
        let find_a_time = AGENDA.find("find_a_time").unwrap().purpose;
        assert!(find_a_time.contains("couldNotCheck"), "{find_a_time}");
        assert!(
            find_a_time.contains("only at diaries already shared"),
            "{find_a_time}"
        );
        let colleague = AGENDA.find("colleague_free").unwrap().purpose;
        assert!(colleague.contains("ALREADY shared"), "{colleague}");
        assert!(colleague.contains("never reported free"), "{colleague}");
        assert!(
            colleague.contains("nothing about whether that person exists"),
            "{colleague}"
        );
        assert!(AGENDA_GUIDANCE.contains("could not check is NOT free"));
    }

    /// Moving a meeting is not re-creating one: the description says what it
    /// leaves alone, and says the length survives a start-only move.
    #[test]
    fn moving_a_meeting_changes_its_time_and_nothing_else() {
        let purpose = AGENDA.find("reschedule_event").unwrap().purpose;
        assert!(purpose.contains("changes nothing else"), "{purpose}");
        assert!(purpose.contains("cannot cancel"), "{purpose}");
        assert!(purpose.contains("keeps its current length"), "{purpose}");
        // One sitting moves; the series does not follow it.
        assert!(purpose.contains("rest of the series stays"), "{purpose}");
    }

    /// The verbs that act on one existing meeting take the user's own words
    /// and a day, never an id — and a title that matches several sittings is a
    /// question rather than a guess.
    #[test]
    fn a_meeting_is_named_and_a_day_disambiguates_it() {
        for verb in [
            "meeting_prep",
            "event_lookup",
            "reschedule_event",
            "cancel_event",
        ] {
            let args = AGENDA.find(verb).unwrap().args;
            let meeting = args.iter().find(|arg| arg.name == "meeting").unwrap();
            assert!(
                meeting.purpose.contains("in the user's own words"),
                "{verb}"
            );
            assert!(
                args.iter().any(|arg| arg.name == "on"),
                "{verb} has no day to disambiguate with"
            );
            assert!(
                AGENDA.find(verb).unwrap().purpose.contains("never guess"),
                "{verb}"
            );
        }
        assert!(AGENDA_GUIDANCE.contains("never invent one"));
    }

    /// Prep is written from what the verb returns, not from the meeting's
    /// title — the same rule the Drive agent's `file_read` carries.
    #[test]
    fn a_briefing_is_written_from_the_meeting_and_not_from_its_name() {
        assert!(
            AGENDA
                .find("meeting_prep")
                .unwrap()
                .purpose
                .contains("rather than from the meeting's title")
        );
    }

    /// The whole reason the verbs take a date rather than a word.
    #[test]
    fn the_model_is_told_to_resolve_days_itself() {
        assert!(AGENDA_GUIDANCE.contains("never a phrase"));
        assert!(
            AGENDA
                .find("whats_on")
                .unwrap()
                .purpose
                .contains("YYYY-MM-DD")
        );
    }

    /// Booking over an existing meeting is the failure the old set existed to
    /// prevent; the model is still told when to check.
    #[test]
    fn the_model_is_told_to_check_before_booking() {
        assert!(
            AGENDA
                .find("create_event")
                .unwrap()
                .purpose
                .contains("am_i_free BEFORE")
        );
    }

    /// AB.5's two writes, held to their own sentences: cancelling is the
    /// user's word, tells the guests, and is not undoable by a verb; an
    /// invitation is answered with the user's own answer, never the agent's.
    #[test]
    fn cancelling_and_answering_are_the_users_word() {
        let cancel = AGENDA.find("cancel_event").unwrap();
        assert!(
            cancel.purpose.contains("user SAID to cancel"),
            "{}",
            cancel.purpose
        );
        assert!(
            cancel.purpose.contains("every guest is emailed"),
            "{}",
            cancel.purpose
        );
        assert!(cancel.purpose.contains("no undo"), "{}", cancel.purpose);
        assert!(cancel.undo.is_none());
        let respond = AGENDA.find("respond_to_invitation").unwrap().purpose;
        assert!(
            respond.contains("the answer the user actually gave"),
            "{respond}"
        );
        assert!(respond.contains("never choose one for them"), "{respond}");
        assert!(AGENDA_GUIDANCE.contains("only with the answer they gave"));
        // …and the two mail-side taps stay the person's own: applying an
        // organizer's cancellation or a guest's reply is excluded, not a verb.
        for route in ["/calendar/cancel", "/calendar/apply-reply"] {
            assert!(
                AGENDA_EXCLUDED
                    .iter()
                    .any(|excluded| excluded.route == route),
                "{route} must stay the person's own tap"
            );
        }
    }

    /// Sharing a diary — and unsharing one — is a person's own act of trust,
    /// and the tenant-wide busy row is not the agent's way around the
    /// shared-diaries-only reach.
    #[test]
    fn sharing_and_the_tenant_wide_busy_row_stay_out_of_reach() {
        for route in [
            "/calendar/calendars/{id}/grants",
            "/calendar/groups",
            "/calendar/freebusy",
        ] {
            assert!(
                AGENDA_EXCLUDED
                    .iter()
                    .any(|excluded| excluded.route == route),
                "{route} is neither a verb's nor excused"
            );
        }
    }
}
