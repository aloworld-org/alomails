//! alo Chat's verbs (ADR 0058) — the one command layer over the rooms.
//!
//! This is the whole of what the Chat agent may do, and the words a model
//! reads about it. Nothing here reads or writes a record: the executors live
//! beside Chat's routes in `alo-jmap` (`chat_intents.rs`), through the asker's
//! tenant-scoped store, and they answer with the same record views the routes
//! serve.
//!
//! Two rules shape this set:
//!
//! - **Reading is bounded by the reader.** Every read runs on the asker's own
//!   account door, so a room they are not in does not exist here — the same
//!   rule chat search already follows, and the reason a missing room and a
//!   private room answer identically.
//! - **A message is the asker's, never the agent's.** The old tool set had no
//!   write at all, because a tool that silently dropped words into a room
//!   would have spoken *for* somebody. Under intents the objection is
//!   answered rather than dodged: `post_message` carries the asker's words
//!   exactly, shows them a preview, waits for their approval, and posts in
//!   their own name — the room watches the proposal and the tap that ran it
//!   (ADR 0047). Nothing posts on the model's say-so.

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const ROOM_REQ: Arg = Arg::required(
    "room",
    "text",
    "the room's name exactly as the user says it, without the leading #",
);
const LIMIT_OPT: Arg = Arg::optional("limit", "integer", "at most this many messages");

/// The verbs.
pub const CHAT_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "my_rooms",
        purpose: "The conversations the asker is in — named rooms and one-to-ones — liveliest first, each with its unread count, who it is with, and the last thing said.",
        effect: Effect::Read,
        args: &[],
        answers: &[
            "what rooms am I in",
            "what conversations do I have",
            "what is happening in chat",
        ],
        preview: None,
        undo: None,
        routes: &["/chat/channels"],
    },
    IntentSpec {
        name: "unread_rooms",
        purpose: "Only the conversations with something unread — how many new messages each holds and how many name the asker — so what was missed is counted from the record, never guessed.",
        effect: Effect::Read,
        args: &[],
        answers: &[
            "what did I miss",
            "where do I have unread messages",
            "am I mentioned anywhere",
        ],
        preview: None,
        undo: None,
        routes: &["/chat/channels"],
    },
    IntentSpec {
        name: "room_members",
        purpose: "Who is in one room — each member with their address and their role — for a room the asker can see. A room that is not theirs to see is reported as not found, never as forbidden.",
        effect: Effect::Read,
        args: &[ROOM_REQ],
        answers: &[
            "who is in the release room",
            "who can read this channel",
            "who am I talking to in here",
        ],
        preview: None,
        undo: None,
        routes: &["/chat/channels/{id}"],
    },
    IntentSpec {
        name: "catch_up_room",
        purpose: "The recent messages of one conversation, oldest first, so what happened in it can be retold — who said what, attributed and quotable. If the room has nothing recent, it says so.",
        effect: Effect::Read,
        args: &[ROOM_REQ, LIMIT_OPT],
        answers: &[
            "what did I miss in the release room",
            "what was said in the launch channel",
            "what was decided in ops",
        ],
        preview: None,
        undo: None,
        routes: &["/chat/channels/{id}/messages"],
    },
    IntentSpec {
        name: "find_in_chat",
        purpose: "The messages matching words somebody used, across the conversations the asker can already read, or in one named room.",
        effect: Effect::Read,
        args: &[
            Arg::required("query", "text", "the words to look for"),
            Arg::optional(
                "room",
                "text",
                "a room's name to look only in, without the leading #",
            ),
            LIMIT_OPT,
        ],
        answers: &[
            "where did we discuss the rollout",
            "who said the demo was Friday",
            "find the message about the outage",
        ],
        preview: None,
        undo: None,
        routes: &["/chat/search"],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "post_message",
        purpose: "Post a message to a room the asker is a member of, in the asker's own name, with their words passed through EXACTLY as they gave them — never reworded, never expanded.",
        effect: Effect::Write,
        args: &[
            ROOM_REQ,
            Arg::required(
                "message",
                "text",
                "the words to post, exactly as the user gave them",
            ),
        ],
        answers: &[
            "tell the team the deploy is done",
            "post in the release room that we shipped",
            "say in ops that the incident is closed",
        ],
        preview: Some("Your message will be posted to #{room} in your own name."),
        undo: None,
        routes: &["/chat/channels/{id}/messages"],
    },
    IntentSpec {
        name: "create_room",
        purpose: "Create a named room with the asker as its owner — public unless the user says private. Nobody else is added: inviting people is the owner's own act in the app.",
        effect: Effect::Write,
        args: &[
            Arg::required("name", "text", "the room's name, without the leading #"),
            Arg::optional("topic", "text", "one line saying what the room is for"),
            Arg::optional("visibility", "text", "public or private; default public"),
        ],
        answers: &[
            "create a room for the launch",
            "make a private room for the audit",
            "set up a channel for the offsite",
        ],
        preview: Some("Room #{name} will be created with you as its owner."),
        undo: None,
        routes: &["/chat/channels"],
    },
];

/// The Chat routes deliberately without a verb, each with its reason.
pub const CHAT_EXCLUDED: &[Excluded] = &[
    Excluded {
        route: "/chat/channels/joinable",
        why: "Browsing rooms to join is the person's own wandering in the app; the agent is asked about rooms the asker already has.",
    },
    Excluded {
        route: "/chat/channels/{id}/archive",
        why: "Archiving a room ends a conversation for everyone in it; a person does that deliberately in the app.",
    },
    Excluded {
        route: "/chat/channels/{id}/join",
        why: "Joining a room is the asker's own step in the app; a later intent set.",
    },
    Excluded {
        route: "/chat/channels/{id}/members",
        why: "Adding someone to a room changes who can read it; a person does that deliberately in the app.",
    },
    Excluded {
        route: "/chat/channels/{id}/members/{user}",
        why: "Removing someone from a room changes who can read it; a person does that deliberately in the app.",
    },
    Excluded {
        route: "/chat/channels/{id}/threads/{seq}",
        why: "A thread's replies come back through catch_up_room's reading of the conversation; per-thread paging is the screen's.",
    },
    Excluded {
        route: "/chat/channels/{id}/read",
        why: "Marking a room read is the reader's own act; an agent must not clear what a person has not actually seen.",
    },
    Excluded {
        route: "/chat/reactions",
        why: "Serves the screen's reaction chips; an agent reads the messages themselves.",
    },
    Excluded {
        route: "/chat/people",
        why: "Serves the mention picker; finding a person is the Mail agent's find_contact, in the address book.",
    },
    Excluded {
        route: "/chat/channels/{id}/turns",
        why: "The agent runtime's own surface; a verb here would be the agent watching itself run.",
    },
    Excluded {
        route: "/chat/channels/{id}/memory",
        why: "The room's memory switch (ADR 0057 §6) is the members' own control over what an agent retains; an agent must not toggle what is remembered about a room.",
    },
    Excluded {
        route: "/chat/channels/{id}/turns/{turn}/stop",
        why: "Stopping a run is the person's brake on an agent; it must never be a tool an agent can reach.",
    },
    Excluded {
        route: "/chat/channels/{id}/goals",
        why: "The goal card (ADR 0058 §7) is the runtime's own progress surface; a verb here would be the agent watching itself run.",
    },
    Excluded {
        route: "/chat/agents",
        why: "Registering agents is the workspace's configuration, kept by a person.",
    },
    Excluded {
        route: "/chat/agents/{id}/dm",
        why: "Opening a one-to-one with an agent is the person's own doorway to it.",
    },
    Excluded {
        route: "/chat/agents/directory",
        why: "The directory describes agents to people; an agent's account of itself is its prompt.",
    },
    Excluded {
        route: "/chat/agents/{id}/directory",
        why: "The directory describes agents to people; an agent's account of itself is its prompt.",
    },
    Excluded {
        route: "/chat/channels/{id}/agents",
        why: "Inviting an agent into a room decides who is listening; a person does that in the app.",
    },
    Excluded {
        route: "/chat/channels/{id}/agents/{agent}",
        why: "Removing an agent from a room decides who is listening; a person does that in the app.",
    },
    Excluded {
        route: "/chat/proposals/{id}",
        why: "The approval surface itself: a verb that decided proposals would let a turn approve its own writes.",
    },
    Excluded {
        route: "/chat/messages/{id}",
        why: "Editing or deleting what somebody said is a person's own act on their own words.",
    },
    Excluded {
        route: "/chat/messages/{id}/reactions",
        why: "Reacting is a person's own gesture; made by an agent it would be the asker seeming to feel something.",
    },
    Excluded {
        route: "/chat/channels/{id}/agents/{agent}/memories",
        why: "The What-I-remember panel is the members' window into an agent; an agent's own memories reach its turns as grounding, not through a verb.",
    },
    Excluded {
        route: "/chat/memories/{id}",
        why: "Forgetting a remembered fact is a person's withdrawal of consent; an agent must not curate what is remembered about a room.",
    },
    Excluded {
        route: "/chat/channels/{id}/instructions",
        why: "A standing instruction (ADR 0057 §7) is a person's advance ask; an agent must not commission itself, and the card list is the members' window.",
    },
    Excluded {
        route: "/chat/instructions/{id}",
        why: "Cancel is the author's and the room owner's brake on a standing instruction; it must never be a tool an agent can reach.",
    },
    Excluded {
        route: "/chat/proposals/{id}/hand",
        why: "Handing an open proposal to an agent (A8.2) is the asker's decision on the approval surface; an agent that could hand work to an agent would be approving writes.",
    },
    Excluded {
        route: "/chat/agents/{id}/tasks",
        why: "Assigning a task to an agent (A8.2) commissions a standing instruction, and an agent must not commission itself or another.",
    },
];

/// The Chat paragraph of the agent's general instructions.
pub const CHAT_GUIDANCE: &str = "For a chat verb, pass the room's name through EXACTLY as the user said it, without the leading #, and never guess which room was meant — if they did not name one, ANSWER and ask. To say what is happening or what was missed, USE a reading verb first and answer from what it returned; attribute what was said to the person who said it and quote rather than paraphrase anything that sounds like a decision. A message is posted only through post_message, in the asker's own name, with their words passed through exactly — never reworded — and it waits for their approval, as does create_room. You cannot edit, delete, react to or mark anything read, and must not offer to.\n";

/// The module, as the registry reads it.
pub static CHAT: IntentModule = IntentModule {
    intents: CHAT_INTENTS,
    excluded: CHAT_EXCLUDED,
    guidance: CHAT_GUIDANCE,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_has_a_route_a_purpose_and_a_question_it_answers() {
        for intent in CHAT_INTENTS {
            assert!(!intent.routes.is_empty(), "{} names no route", intent.name);
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
        let mut names: Vec<&str> = CHAT_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CHAT_INTENTS.len());
        let doc = CHAT.doc();
        for intent in CHAT_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(CHAT_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn an_exclusion_is_a_sentence_not_a_shrug() {
        for excluded in CHAT_EXCLUDED {
            assert!(
                excluded.why.ends_with('.'),
                "{}: {}",
                excluded.route,
                excluded.why
            );
            assert!(
                !CHAT_INTENTS
                    .iter()
                    .any(|i| i.routes.contains(&excluded.route)),
                "{} is both a verb's route and excluded",
                excluded.route
            );
        }
    }

    /// The line the old tool set drew, kept where it still holds: a message
    /// reaches a room only as a previewed, approved proposal in the asker's
    /// own name — so both writes carry a preview, and the guidance says whose
    /// words they are.
    #[test]
    fn nothing_speaks_without_the_askers_approval() {
        for intent in CHAT_INTENTS {
            if intent.effect == Effect::Write {
                assert!(
                    intent.preview.is_some(),
                    "{} would run without the asker seeing what changes",
                    intent.name
                );
            }
        }
        assert!(CHAT_GUIDANCE.contains("EXACTLY"));
        assert!(CHAT_GUIDANCE.contains("without the leading #"));
        assert!(CHAT_GUIDANCE.contains("in the asker's own name"));
    }
}
