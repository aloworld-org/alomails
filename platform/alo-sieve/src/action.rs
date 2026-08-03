//! The outcome of evaluating a script: the ordered actions to perform and
//! any warnings. The engine decides *what* should happen; the store and
//! delivery bridge decide *how* (and apply the safety budgets). Implicit
//! keep (RFC 5228 §2.10.2) is already resolved into the action list.

/// A single delivery action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Keep the message in the Inbox (explicit or implicit), with these
    /// flags applied.
    Keep { flags: Vec<String> },
    /// File the message into a named mailbox, with these flags.
    FileInto { mailbox: String, flags: Vec<String> },
    /// Redirect a copy to an address (bounded by the caller).
    Redirect { address: String },
    /// Send a vacation auto-reply (the caller checks suppression and the
    /// RFC 3834 return-path rules that need store state before sending).
    Vacation(VacationReply),
}

/// A vacation auto-reply the engine judged permissible from the message
/// alone (RFC 3834 header guards passed). The caller still checks
/// per-correspondent `:days` suppression before actually sending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VacationReply {
    /// The correspondent to reply to (the triggering message's return path).
    pub to: String,
    /// `:subject`, if given.
    pub subject: Option<String>,
    /// `:from`, if given.
    pub from: Option<String>,
    /// `:handle` — scopes suppression independently of the reason.
    pub handle: Option<String>,
    /// `:days` suppression window (caller applies its default if `None`).
    pub days: Option<u32>,
    /// The reply body.
    pub reason: String,
}

/// The full result of evaluating a script.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    /// Actions to perform, in order (implicit keep already resolved in).
    pub actions: Vec<Action>,
    /// Non-fatal warnings (e.g. fileinto to a missing folder degraded to
    /// keep) — logged, never shown to a remote sender.
    pub warnings: Vec<String>,
}

impl Outcome {
    /// Whether any action files or keeps the message somewhere (used to
    /// sanity-check that mail is never silently lost).
    pub fn files_somewhere(&self) -> bool {
        self.actions
            .iter()
            .any(|a| matches!(a, Action::Keep { .. } | Action::FileInto { .. }))
    }
}

/// An evaluation failure — the caller responds with implicit keep.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvalError {
    /// The instruction budget was exhausted (runaway script).
    #[error("evaluation exceeded the instruction budget")]
    BudgetExceeded,
}
