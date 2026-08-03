//! IDLE (RFC 2177). After `+ idling`, we watch the account's own state
//! cursor (the per-account modseq — never a co-tenant's, migration 0005)
//! and, on change, resync the selected mailbox: untagged EXPUNGE / EXISTS
//! / FETCH-flags. The client ends the idle with `DONE`.
//!
//! Push is poll-driven off the account change cursor (sub-second
//! LISTEN/NOTIFY is a follow-up); the cursor read is account-scoped, so an
//! idle stream is provably silent about another account's activity.

use std::time::Duration;

use alo_store::AccountStore;

use super::Session;
use crate::parser::{Parser, ReadOutcome, read_command};

/// Poll cadence for the account change cursor while idling.
const POLL: Duration = Duration::from_millis(1000);

impl Session {
    pub(super) async fn cmd_idle(&mut self, tag: &str, acc: &AccountStore) -> std::io::Result<()> {
        self.send(b"+ idling\r\n").await?;
        let mut ticker = tokio::time::interval(POLL);
        ticker.tick().await; // consume the immediate first tick

        loop {
            let mut poll = false;
            tokio::select! {
                outcome = read_command(&mut self.reader, self.cfg.max_line, self.cfg.max_literal) => {
                    match outcome? {
                        ReadOutcome::Line(segs) => {
                            let mut p = Parser::new(segs);
                            if p.read_atom().eq_ignore_ascii_case("DONE") {
                                break;
                            }
                            // Anything else during IDLE is ignored (we keep
                            // idling); a well-behaved client only sends DONE.
                        }
                        ReadOutcome::Eof => return Ok(()),
                        _ => {
                            let _ = self.send_line("* BAD Expected DONE").await;
                        }
                    }
                }
                _ = ticker.tick() => {
                    poll = true;
                }
            }
            if poll && self.selected.is_some() {
                let synced = self
                    .selected
                    .as_ref()
                    .map(|s| s.synced_state.clone())
                    .unwrap_or_default();
                let current = acc.state().await.unwrap_or_default();
                if current != synced {
                    self.resync(acc).await?;
                }
            }
        }
        self.tagged(tag, "OK", "IDLE terminated").await
    }
}
