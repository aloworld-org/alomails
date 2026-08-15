//! The Mail agent's tool set (ADR 0034).
//!
//! These nine lived in `crate::agent` as part of one undifferentiated "core"
//! list, beside `create_task` and `create_event`, because there was one agent
//! and the question "whose tools are these?" had no place to be answered. A1.2
//! gives every agent a product, so the question has an answer and the tools
//! move to the module that owns them: this is what makes the Mail agent the
//! agent *of* mail rather than a general assistant under a mail-shaped name.
//!
//! The address book is Mail's too — see [`crate::agent_contacts`], whose one
//! tool is offered alongside these.

use crate::agent_tool::AgentTool;

/// What the Mail agent may do, each tool declaring whether it reads or writes
/// (ADR 0047 §1).
///
/// Every one of them writes. Mail's *answers* — "are we in contact with X",
/// "who last replied" — are grounded in retrieval today and get their own
/// reading tools in A2.8; until then this agent proposes and does not look up.
pub const MAIL_TOOLS: &[AgentTool] = &[
    AgentTool::write("mark_read"),
    AgentTool::write("flag_email"),
    AgentTool::write("archive_email"),
    AgentTool::write("trash_email"),
    AgentTool::write("snooze_email"),
    AgentTool::write("draft_email"),
    AgentTool::write("draft_reply"),
    AgentTool::write("send_email"),
    AgentTool::write("move_to_folder"),
];

/// What each Mail tool takes, in the words the model reads.
///
/// Read-versus-write is declared in [`MAIL_TOOLS`] and rendered into the prompt
/// from there (ADR 0047 §1), never restated here.
pub const MAIL_TOOL_DOC: &str = "\
- mark_read: mark an email read or unread. args: {\"source\": number, \"read\": boolean}.\n\
- flag_email: flag (star) or unflag an email. args: {\"source\": number, \"flagged\": boolean}.\n\
- archive_email: move an email out of the inbox into Archive. args: {\"source\": number}.\n\
- trash_email: move an email to Trash (delete it from the inbox and archive). args: {\"source\": number}.\n\
- snooze_email: hide an email from the inbox until a chosen time, when it returns to the inbox. args: {\"source\": number, \"until\": string RFC 3339 datetime e.g. \"2026-08-07T09:00:00Z\" (required)}.\n\
- draft_email: write a NEW email and save it to the user's Drafts for them to review and send — it is NEVER sent automatically. args: {\"to\": string email address (required), \"subject\": string (optional), \"body\": string (required)}. Compose the body from the request; do not invent facts. The sender is always the user's own address — never set it.\n\
- draft_reply: write a reply to an email in the sources and save it to the user's Drafts — NEVER sent automatically. args: {\"source\": number (the email to reply to, required), \"body\": string (required)}. The reply goes to that email's sender and keeps its subject thread; compose the body from the request, do not invent facts.\n\
- send_email: SEND a message that is ALREADY in the user's Drafts. This delivers it to its recipients and CANNOT be undone. args: {\"source\": number (a draft in the sources, required)}. Only propose this when the user clearly and explicitly asks to send, and only for a draft that already exists — if there is no draft yet, write one first with draft_email or draft_reply and let the user send it. The user still approves before anything is sent.\n\
- move_to_folder: move an email into one of the user's own mail folders. args: {\"source\": number, \"folder\": string}. Set \"folder\" to EXACTLY one of the folder names listed under \"Folders\" below — never invent a folder. If the user names a folder that is not in that list, ANSWER instead and say that folder does not exist. Prefer the dedicated tools for Archive (archive_email) and Trash (trash_email).\n";

/// The rules that keep a Mail proposal honest, appended to the system prompt.
///
/// The sentence about `source` numbers used to sit in the prompt's general
/// rules, where every agent read it — including the ones with no tool that
/// takes a `source`. It belongs to whoever has such a tool.
pub const MAIL_GUIDANCE: &str = "For any tool that acts on an email, set \"source\" to the number [n] of that email in the numbered sources above; only propose it when the relevant email is present in the sources.\n";
