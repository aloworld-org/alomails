//! What a tool *does* — the one bit that decides whether its result lands in
//! the room now or waits for a tap (ADR 0047).
//!
//! Before this existed, the distinction lived eleven times **in English**, in
//! the sentence "It only READS; it changes nothing." that each reading tool's
//! description carries. Nothing in the code could tell `stock_answer` from
//! `send_email`, so every tool came back as a proposal and the agent answered
//! "is the X100 in stock?" with a button.
//!
//! The bit is declared **beside the name, in the same const list the prompt is
//! generated from**, so a tool cannot be added to a product without answering
//! the question. It is never re-derived at a call site: not from the name, not
//! from the description, and never from the model's own envelope — the model is
//! the untrusted party here, and an injected turn would call a write a read.

/// Whether a tool changes anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// It answers a question and writes nothing. Runs **inside the turn**; its
    /// result grounds the answer that lands in the room.
    Read,
    /// It changes something. Runs only from an approval the asker themselves
    /// gave — never from inside a turn.
    Write,
}

impl Effect {
    /// The token used in the audit record and on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

/// One tool an agent may use: the name the model calls it by, and what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentTool {
    /// The name in the model's envelope, and in the prompt's `- name:` line.
    pub name: &'static str,
    /// Read or write — see [`Effect`].
    pub effect: Effect,
}

impl AgentTool {
    /// A tool that only answers.
    #[must_use]
    pub const fn read(name: &'static str) -> Self {
        Self {
            name,
            effect: Effect::Read,
        }
    }

    /// A tool that changes something.
    #[must_use]
    pub const fn write(name: &'static str) -> Self {
        Self {
            name,
            effect: Effect::Write,
        }
    }

    /// Whether this one may run without anybody approving it.
    #[must_use]
    pub const fn is_read(self) -> bool {
        matches!(self.effect, Effect::Read)
    }
}

/// The tool called `name` in `list`, if it is there.
///
/// A free function rather than a method because the registry is a slice of
/// consts, one per product, and every caller asks the same question of it.
#[must_use]
pub fn find_tool<'a>(list: &'a [AgentTool], name: &str) -> Option<&'a AgentTool> {
    list.iter().find(|tool| tool.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_effect_is_carried_by_the_declaration_not_the_name() {
        // A name that looks like a read is a write if it was declared one:
        // there is no naming convention here, on purpose. The first tool that
        // broke such a convention would execute a write with no approval, and
        // the failure would be silent.
        assert_eq!(AgentTool::write("find_and_delete").effect, Effect::Write);
        assert!(!AgentTool::write("find_and_delete").is_read());
        assert!(AgentTool::read("stock_answer").is_read());
        assert_eq!(Effect::Read.as_str(), "read");
        assert_eq!(Effect::Write.as_str(), "write");
    }

    #[test]
    fn lookup_matches_the_whole_name_only() {
        let list = [AgentTool::read("stock_answer"), AgentTool::write("send")];
        assert_eq!(
            find_tool(&list, "stock_answer").map(|t| t.effect),
            Some(Effect::Read)
        );
        assert_eq!(
            find_tool(&list, "send").map(|t| t.effect),
            Some(Effect::Write)
        );
        // A prefix, a suffix, and a stray space are all strangers.
        assert!(find_tool(&list, "stock").is_none());
        assert!(find_tool(&list, "stock_answer ").is_none());
        assert!(find_tool(&list, "").is_none());
    }
}
