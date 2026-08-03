//! # alo-sieve — the Sieve engine (RFC 5228 + vacation/subaddress/imap4flags)
//!
//! Compiles user filter scripts to an AST (enforcing `require` and hard
//! parse limits) and evaluates them against a message at delivery time,
//! returning an [`Outcome`] of [`Action`]s. **Pure and protocol-agnostic:**
//! no I/O, no store, no knowledge of JMAP/ManageSieve — the store performs
//! the store-side actions and the delivery bridge performs the outbound
//! ones (redirect, vacation), each applying its own safety budget.
//!
//! Sieve scripts are **user-supplied programs run on the server**, so every
//! limit here is a security control, and no script failure ever loses mail:
//! a compile error on the active script, a runtime budget overrun, or an
//! unperformable action all fall back to implicit keep. See
//! `docs/design/sieve-filtering.md`.

pub mod action;
pub mod ast;
pub mod error;
pub mod eval;
pub mod message;
pub mod parser;

pub use action::{Action, EvalError, Outcome, VacationReply};
pub use error::CompileError;
pub use eval::{EvalContext, evaluate};
pub use message::{Address, Message};
pub use parser::{Limits, Script, compile};
