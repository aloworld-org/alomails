//! # alo-auth-mail — email authentication for alo
//!
//! Owns SPF (RFC 7208), DKIM (RFC 6376 + Ed25519 per RFC 8463), DMARC
//! (RFC 7489), ARC (RFC 8617), MTA-STS (RFC 8461), and the one
//! [`Authentication-Results`](authres) builder (RFC 8601) that records
//! every verdict. `alo-smtp` calls this crate at DATA time
//! (inbound verdicts) and at submission (DKIM signing).
//!
//! Security invariants (see `docs/design/email-authentication-trust-stack.md`): DNS is
//! hostile input handled behind one [`resolver`] with timeouts and
//! caps; private keys are permission-checked, never logged, and held
//! in zeroizing buffers; malformed input yields a *fail verdict*,
//! never a panic.

pub mod arc;
pub mod authres;
pub mod dkim;
pub mod dmarc;
pub mod mta_sts;
pub mod resolver;
pub mod spf;
