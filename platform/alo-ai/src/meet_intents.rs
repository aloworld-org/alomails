//! alo Meet's verbs (ADR 0058) — a meeting before it runs and after it is over.
//!
//! This is the whole of what the Meet agent may do, and the words a model
//! reads about it. The executors live beside Meet's routes in `alo-jmap`
//! (`meet_intents.rs`), through the asker's tenant-scoped store.
//!
//! **The agent is still not a live participant.** It joins nothing, hears
//! nothing and says nothing while a meeting is running — the in-call agent is
//! a media path nobody has decided on, and every excluded route below that
//! touches a running call says so. What this set adds to the old tool set is
//! the *before*: the meetings ahead in the asker's own diary, one of them
//! looked up with its notes, and a new one scheduled.
//!
//! Two of the old rules hold unchanged, and one is deliberately retired:
//!
//! - **A meeting is named, never identified.** The model passes the title the
//!   user said and, when it must, the day; a title that ran twice comes back
//!   listing the days rather than picking one — minutes written into the
//!   wrong room publish them to the wrong people, and a moved meeting moves
//!   the wrong Tuesday.
//! - **Minutes are written from the meeting, not from its title,** and they
//!   are the asker's own message: previewed, approved, posted in their name.
//! - **"No calendar entry" is retired**, as AC.2 orders. A3.2 refused a
//!   scheduling tool because it would have been a second mechanism beside the
//!   Agenda agent's; under intents `schedule_meeting` runs the Agenda
//!   module's own calendar write, as the asker, previewed and approved — one
//!   mechanism, reached by the agent whose subject matter meetings are. The
//!   actions *inside* minutes still become work only through the Tasks and
//!   Agenda agents' own proposals, accepted one at a time (ADR 0023).

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const MEETING_REQ: Arg = Arg::required(
    "meeting",
    "text",
    "the meeting's title, in the user's own words — there is no identifier for a meeting that you could know",
);
const DAY_OPT: Arg = Arg::optional(
    "day",
    "date",
    "the day it runs or ran, YYYY-MM-DD — needed only when the title matches more than once",
);

/// The verbs.
pub const MEET_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "meetings_recent",
        purpose: "The meetings this person was in that have already ENDED — the title, when each one ran, its day, and whether it came out of a conversation. It changes nothing. Use it whenever the user says \"the last meeting\" or names one you have not seen.",
        effect: Effect::Read,
        args: &[Arg::optional(
            "limit",
            "integer",
            "how many, 10 by default, at most 25",
        )],
        answers: &[
            "what meetings did I have",
            "when did the review last run",
            "what was my last meeting",
        ],
        preview: None,
        undo: None,
        routes: &["/meet/history"],
    },
    IntentSpec {
        name: "meeting_record",
        purpose: "ONE ended meeting in full — who attended, what was said in it (the live transcript and the messages typed during it), and what has been posted in its conversation SINCE it finished (postedSince). It changes nothing. Read it before writing anything about a meeting: a title is not evidence of what was decided, and minutes already in postedSince must not be written a second time.",
        effect: Effect::Read,
        args: &[MEETING_REQ, DAY_OPT],
        answers: &[
            "what happened in the budget review",
            "who was in yesterday's standup",
            "what was decided in the retro",
        ],
        preview: None,
        undo: None,
        routes: &[
            "/meet/{id}",
            "/meet/{id}/participants",
            "/meet/{id}/transcript",
            "/meet/{id}/messages",
        ],
    },
    IntentSpec {
        name: "upcoming_meetings",
        purpose: "The meetings ahead in the asker's own diary — each with its title, day, time, place and whether it repeats — so \"what do I have coming up\" is counted from the calendar, never guessed. It changes nothing.",
        effect: Effect::Read,
        args: &[Arg::optional(
            "days",
            "integer",
            "how far ahead to look, 14 by default, at most 60",
        )],
        answers: &[
            "what meetings do I have coming up",
            "what is in my diary this week",
            "when is my next meeting",
        ],
        preview: None,
        undo: None,
        // The diary's own listing — no `/meet/` route serves it, which is why
        // this verb adapts none.
        routes: &[],
    },
    IntentSpec {
        name: "meeting_lookup",
        purpose: "ONE meeting in the asker's diary, by its title — when it runs, where, who is invited, and the notes on the invitation — plus whether a sitting of it has already happened and left a record. It changes nothing. A title in the diary more than once is a question that lists the days, never a guess.",
        effect: Effect::Read,
        args: &[MEETING_REQ, DAY_OPT],
        answers: &[
            "when is the budget review",
            "what are the notes on Friday's kickoff",
            "where is the board meeting",
        ],
        preview: None,
        undo: None,
        routes: &["/meet/events/{id}"],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "meeting_minutes",
        purpose: "Post the minutes of ONE ended meeting into the conversation the meeting came out of — a summary, the decisions, and the actions people agreed. The message appears under the asker's OWN name, so write it as they would, in the language of the meeting. Every line comes from meeting_record: never write down a decision nobody made, an owner nobody volunteered or a deadline nobody gave, and minutes already posted must not be written a second time. This posts the minutes and NOTHING else — it creates no tasks and no calendar entries; to turn the actions into to-dos, ask the Tasks agent.",
        effect: Effect::Write,
        args: &[
            MEETING_REQ,
            DAY_OPT,
            Arg::required("summary", "text", "a short paragraph of what happened"),
            Arg::optional(
                "decisions",
                "array",
                "sentences, at most 20 — only decisions the record contains",
            ),
            Arg::optional(
                "actions",
                "array",
                "each {\"what\": text, \"owner\": text (whoever agreed to it, optional), \"due\": YYYY-MM-DD (optional)}, at most 20",
            ),
        ],
        answers: &[
            "write up the budget review",
            "post the minutes of yesterday's retro",
            "put what we agreed in the room",
        ],
        preview: Some(
            "The minutes of {meeting} will be posted into its conversation in your own name.",
        ),
        undo: None,
        // The minutes land in the meeting's chat conversation — no `/meet/`
        // route serves that post, which is why this verb adapts none.
        routes: &[],
    },
    IntentSpec {
        name: "schedule_meeting",
        purpose: "Put a meeting in the asker's OWN diary — the Agenda module's own calendar write, run as the asker once they approve. It invites nobody by itself: guests are added in the calendar.",
        effect: Effect::Write,
        args: &[
            Arg::required("title", "text", "what the meeting is called"),
            Arg::required(
                "start",
                "text",
                "when it starts, as an RFC 3339 datetime — resolve today/tomorrow/Monday against today's date first, never pass a phrase",
            ),
            Arg::optional(
                "end",
                "text",
                "when it ends, RFC 3339; one hour after start when left out",
            ),
            Arg::optional("location", "text", "where it happens"),
            Arg::optional("notes", "text", "the notes on the invitation"),
        ],
        answers: &[
            "schedule a review for Friday at 10",
            "set up a kickoff meeting tomorrow",
            "put a retro in my diary next week",
        ],
        preview: Some("{title} will be put in your diary at {start}."),
        undo: None,
        // The diary's write — no `/meet/` route serves it, which is why this
        // verb adapts none.
        routes: &[],
    },
];

/// The Meet routes deliberately without a verb, each with its reason — and
/// most of them are one reason worn many ways: nothing here touches a call
/// while it is running.
pub const MEET_EXCLUDED: &[Excluded] = &[
    Excluded {
        route: "/meet",
        why: "Starting a call and listing the live ones is a person walking into a room; the media path is deliberately not an agent's.",
    },
    Excluded {
        route: "/meet/{id}/join",
        why: "A join token puts somebody in the call; the agent joins nothing — the in-call agent is a media path nobody has decided on.",
    },
    Excluded {
        route: "/meet/{id}/end",
        why: "Ending a meeting ends it for everyone in it; a person does that in the room.",
    },
    Excluded {
        route: "/meet/{id}/moderate",
        why: "Muting or removing somebody is the host's own act on people, never delegated.",
    },
    Excluded {
        route: "/meet/{id}/workspace",
        why: "The in-call workspace is an opaque client state the screens sync; the agent's record is the transcript and the messages.",
    },
    Excluded {
        route: "/meet/{id}/workspace/vote",
        why: "A vote is each person's own voice; cast by an agent it would be the asker seeming to decide.",
    },
    Excluded {
        route: "/meet/{id}/recordings",
        why: "Recordings are consent-gated media the screen plays; the readable record is the transcript.",
    },
    Excluded {
        route: "/meet/{id}/recordings/{recording}/consent",
        why: "Consent to being recorded is each person's own to give and nobody else's.",
    },
    Excluded {
        route: "/meet/{id}/recordings/{recording}/start",
        why: "Recording people is a deliberate act in the call, done by somebody in it.",
    },
    Excluded {
        route: "/meet/{id}/recordings/{recording}/stop",
        why: "Stopping a recording is the same in-call act as starting one, and stays a person's.",
    },
    Excluded {
        route: "/meet/{id}/messages/{message}/attachments",
        why: "Serves the screen's file chips; a file's text is read through the Drive agent's own tool.",
    },
    Excluded {
        route: "/meet/{id}/messages/{message}/attachments/{attachment}",
        why: "Serves one attachment's bytes to the screen; the agent reads the words, not the files.",
    },
    Excluded {
        route: "/meet/{id}/messages/{message}/reactions",
        why: "Reacting is a person's own gesture; made by an agent it would be the asker seeming to feel something.",
    },
    Excluded {
        route: "/meet/channels/{id}",
        why: "The room's screen asking which meeting is live in it; what a sitting left behind is read through meeting_record.",
    },
];

/// The Meet paragraph of the agent's general instructions.
pub const MEET_GUIDANCE: &str = "For a Meet verb, read the meeting before you say anything about it: what happened in a sitting is in its transcript and its messages, and its title is not evidence of anything. Name a meeting the way the user did and add the day when a title matches more than once; never invent an identifier. Resolve every relative day (today, yesterday, Monday) against today's date given below and pass a real calendar date or datetime — never a phrase. You can see only the meetings this person is allowed to see, so an answer about what was decided is about those and has to say so rather than sounding like the whole company. A meeting that is still running has no minutes yet: say so, and offer to write them once it has ended. Minutes are a record, not a summary of the title — quote what people actually agreed, attribute a decision only to somebody who made it, and leave out anything the record does not contain. The actions in minutes become to-dos and diary entries through the Tasks and Agenda agents, whose proposals the user accepts one at a time: say that is what you would do next, and never claim a task or a reminder has been created here. Both writes wait for the asker's approval: never say the minutes have been posted or a meeting has been scheduled until they approve.\n";

/// The module, as the registry reads it.
pub static MEET: IntentModule = IntentModule {
    intents: MEET_INTENTS,
    excluded: MEET_EXCLUDED,
    guidance: MEET_GUIDANCE,
};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The verbs whose surface is another module's record — the diary, the
    /// meeting's chat conversation — and which therefore adapt no `/meet/`
    /// route. Named, so a new verb with an empty route list fails the test
    /// instead of joining them silently.
    const ROUTELESS: &[&str] = &["upcoming_meetings", "meeting_minutes", "schedule_meeting"];

    #[test]
    fn every_verb_has_a_route_a_purpose_and_a_question_it_answers() {
        for intent in MEET_INTENTS {
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
        let mut names: Vec<&str> = MEET_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), MEET_INTENTS.len());
        let doc = MEET.doc();
        for intent in MEET_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(MEET_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn an_exclusion_is_a_sentence_not_a_shrug() {
        for excluded in MEET_EXCLUDED {
            assert!(
                excluded.why.ends_with('.'),
                "{}: {}",
                excluded.route,
                excluded.why
            );
            assert!(
                !MEET_INTENTS
                    .iter()
                    .any(|i| i.routes.contains(&excluded.route)),
                "{} is both a verb's route and excluded",
                excluded.route
            );
        }
    }

    /// Every read says, where the model reads it, that it changes nothing — so
    /// a turn never offers a button for an answer.
    #[test]
    fn the_reads_say_they_change_nothing() {
        for intent in MEET_INTENTS.iter().filter(|i| i.effect == Effect::Read) {
            assert!(
                intent.purpose.contains("changes nothing"),
                "{} does not say it changes nothing",
                intent.name
            );
        }
    }

    /// A meeting is named and dated, and a title that matches twice is a
    /// question — the rule that keeps minutes out of the wrong room and a
    /// lookup off the wrong Tuesday.
    #[test]
    fn a_meeting_is_named_and_never_identified() {
        for name in ["meeting_record", "meeting_lookup", "meeting_minutes"] {
            let intent = MEET.find(name).unwrap_or_else(|| panic!("{name} exists"));
            assert!(
                intent
                    .args
                    .iter()
                    .any(|arg| arg.name == "meeting" && arg.purpose.contains("no identifier")),
                "{name} does not take the title in the user's own words"
            );
            assert!(
                intent.args.iter().any(|arg| arg.name == "day"),
                "{name} cannot be told apart by its day"
            );
        }
        assert!(MEET_GUIDANCE.contains("never invent an identifier"));
    }

    /// The A3.2 doctrine that still holds: minutes come out of the record, are
    /// never written twice, are the asker's own message, and their actions
    /// become work only through the Tasks and Agenda agents.
    #[test]
    fn minutes_come_out_of_the_record_and_stay_the_askers_own() {
        let minutes = MEET.find("meeting_minutes").unwrap();
        assert!(
            minutes
                .purpose
                .contains("Every line comes from meeting_record")
        );
        assert!(minutes.purpose.contains("second time"));
        assert!(minutes.purpose.contains("OWN name"));
        assert!(
            minutes
                .purpose
                .contains("creates no tasks and no calendar entries")
        );
        assert!(minutes.purpose.contains("ask the Tasks agent"));
        let record = MEET.find("meeting_record").unwrap();
        assert!(record.purpose.contains("postedSince"));
        assert!(MEET_GUIDANCE.contains("never say the minutes have been posted"));
    }

    /// The doctrine AC.2 retires, in its new form: scheduling exists here, it
    /// is the Agenda module's own write run as the asker, and nothing happens
    /// until they approve — the second-mechanism objection is answered by
    /// sharing the one mechanism, not by refusing the verb.
    #[test]
    fn scheduling_is_the_agenda_write_as_the_asker_and_waits_for_approval() {
        let schedule = MEET.find("schedule_meeting").unwrap();
        assert_eq!(schedule.effect, Effect::Write);
        assert!(schedule.preview.is_some());
        assert!(schedule.purpose.contains("OWN diary"));
        assert!(
            schedule
                .purpose
                .contains("Agenda module's own calendar write")
        );
        assert!(schedule.purpose.contains("invites nobody"));
        assert!(MEET_GUIDANCE.contains("a meeting has been scheduled until they approve"));
    }

    /// Nothing in this set can touch a running call, and the exclusions say so
    /// route by route: every route that joins, ends, moderates, records or
    /// votes is excluded with a sentence.
    #[test]
    fn nothing_touches_a_call_while_it_is_running() {
        for route in [
            "/meet/{id}/join",
            "/meet/{id}/end",
            "/meet/{id}/moderate",
            "/meet/{id}/recordings/{recording}/start",
            "/meet/{id}/workspace/vote",
        ] {
            assert!(
                MEET_EXCLUDED.iter().any(|e| e.route == route),
                "{route} is reachable"
            );
        }
    }
}
