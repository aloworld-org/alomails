//! The Tasks agent's tool set (ADR 0034).
//!
//! One tool today. It is a module of its own rather than a line left in the
//! "core" list because a product's tools are what a product's agent *is*
//! (A1.2), and A2.7 — what is on my plate, prioritise, chase an overdue owner —
//! adds the rest of them here.

use crate::agent_tool::AgentTool;

/// What the Tasks agent may do (ADR 0047 §1 declares the effect beside the
/// name).
pub const TASKS_TOOLS: &[AgentTool] = &[AgentTool::write("create_task")];

/// What each Tasks tool takes, in the words the model reads.
pub const TASKS_TOOL_DOC: &str = "\
- create_task: create a to-do for the user. args: {\"title\": string (required), \"due\": string in \"YYYY-MM-DD\" (optional), \"notes\": string (optional)}.\n";

/// The rules that keep a Tasks proposal honest, appended to the system prompt.
pub const TASKS_GUIDANCE: &str = "For create_task, write the title in the user's own words and never invent a due date they did not give you — a task with a deadline nobody set is worse than one with none.\n";
