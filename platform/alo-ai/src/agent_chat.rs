//! The **Chat** reading tools of the agent (ADR 0034) — what alo Chat lends
//! the one agent.
//!
//! Chat already has agents *inside* rooms, mentioned by name and answering
//! there (`docs/design/chat-agents.md`). This is the other direction: the
//! workspace agent, asked from anywhere, being able to look at a room. "What
//! did I miss in the release channel?" is the question a workspace agent is
//! for, and until now it could only answer from search results — which is to
//! say, not from the conversation.
//!
//! Both tools read. There is deliberately no tool that posts:
//!
//! - **An agent that could post would be an agent that could speak for you.**
//!   Inside a room an agent posts *as itself* and acts *on behalf of* the
//!   person who asked, and the whole design rests on those being visibly
//!   different things. A workspace tool that dropped a message into a room
//!   would have neither property: it would arrive with the asker's authority
//!   and no room member watching the turn that produced it.
//! - **Reading is bounded by the reader.** Both tools run on the asker's own
//!   account door, so a room they are not in does not exist for them here —
//!   the same rule chat search already follows.

/// The Chat reading tools the agent may propose, by name.
pub const CHAT_TOOLS: &[&str] = &["catch_up_room", "find_in_chat"];

/// What each Chat tool takes, in the words the model reads.
pub const CHAT_TOOL_DOC: &str = "\
- catch_up_room: read the recent messages of one conversation so you can say what happened in it. It only READS; it changes nothing. args: {\"room\": string (the channel's name as the user says it, without the # , required), \"limit\": integer (optional, at most 50 messages)}. Propose this when the user asks what they missed, what was said, or what was decided in a named conversation. Summarise what you read and say who said what; never invent a message, and if the room has nothing recent, say that rather than reaching for the sources.\n\
- find_in_chat: search conversations for words somebody used. It only READS. args: {\"query\": string (the words to look for, required), \"room\": string (a channel name to look only in, optional), \"limit\": integer (optional, at most 25)}. Propose this when the user asks where something was discussed or who said something. It finds only conversations the user can already read.\n";

/// The rules that keep a Chat proposal honest, appended to the system prompt.
pub const CHAT_GUIDANCE: &str = "For a chat tool, pass the room's name through EXACTLY as the user said it, without the leading #, and never guess which room was meant — if they did not name one, ANSWER and ask. You cannot post, reply or react in a conversation and must not offer to; if the user asks you to tell a room something, say that sending it is theirs to do, or that they can mention an agent in the room itself. When you report what was said, attribute it to the person who said it and quote rather than paraphrase anything that sounds like a decision.\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_tool_is_described() {
        for name in CHAT_TOOLS {
            assert!(
                CHAT_TOOL_DOC.contains(&format!("- {name}:")),
                "{name} is offered to the model with no description"
            );
        }
    }

    /// The line this set must not cross. An agent that can post into a room is
    /// an agent that can speak as somebody, which is precisely what chat's own
    /// agent design separates.
    #[test]
    fn nothing_here_can_speak_in_a_room() {
        for name in CHAT_TOOLS {
            assert!(
                !name.contains("post")
                    && !name.contains("send")
                    && !name.contains("reply")
                    && !name.contains("react"),
                "{name} would let the workspace agent speak in somebody's room"
            );
        }
        assert!(CHAT_GUIDANCE.contains("cannot post"));
    }

    #[test]
    fn the_model_is_told_to_pass_room_names_through() {
        assert!(CHAT_GUIDANCE.contains("EXACTLY"));
        assert!(CHAT_GUIDANCE.contains("without the leading #"));
    }
}
