//! Turns an agent is taking right now, and stopping one (ADR 0034:
//! "a **Stop** control on any multi-step run").
//!
//! Two things are missing without this. The first is worse than the one the
//! ADR names: when someone mentions an agent, nothing happens on screen for
//! however long the model takes, so a slow answer is indistinguishable from a
//! broken one. The second is the Stop itself — an agent that has misread the
//! question should be interruptible rather than waited out.
//!
//! Deliberately **in memory, not a table**. A turn in flight is not a fact
//! about the workspace; it is a fact about one process for a few seconds. If
//! the process dies the turn dies with it, and a row saying otherwise would be
//! a lie that outlives its subject — the same reason nobody persists "is
//! typing".
//!
//! Two consequences, both stated rather than discovered later:
//!
//! * With more than one API process, a Stop only reaches turns running on the
//!   process that receives it. Acceptable while a turn is a single call of a
//!   few seconds; the thing to revisit before turns become long-running.
//! * Stopping does not abort the model call already in flight — it declines to
//!   post its result. That is what someone pressing Stop actually wants (those
//!   words not appearing in the room), and it costs no new dependency to do.
//!   A true abort belongs with the multi-step turns that do not exist yet.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use time::OffsetDateTime;

use alo_store::{ChatChannelId, TenantId};

/// One agent turn currently running.
#[derive(Clone)]
pub struct Turn {
    /// Opaque handle a client stops by.
    pub id: String,
    /// The agent answering, so the room can say who is thinking.
    pub agent: String,
    /// Its handle, for "alo is thinking…".
    pub handle: String,
    /// Who asked — the only person who may stop it, for the same reason only
    /// the asker may approve what it proposes: the turn is running with their
    /// access, not the room's.
    pub asked_by: String,
    pub started_at: OffsetDateTime,
    stopped: Arc<AtomicBool>,
}

/// A room, identified across tenants: `(tenant, channel)`.
type Room = (String, String);

/// Every turn in flight on this process, keyed by room.
#[derive(Clone, Default)]
pub struct Turns {
    live: Arc<Mutex<HashMap<Room, Vec<Turn>>>>,
}

impl Turns {
    /// Register a turn about to start; the token it carries is what `stop`
    /// trips.
    #[must_use]
    pub fn begin(
        &self,
        tenant: &TenantId,
        channel: &ChatChannelId,
        agent: &str,
        handle: &str,
        asked_by: &str,
    ) -> (String, Arc<AtomicBool>) {
        let id = format!(
            "{}-{}",
            channel.as_str(),
            OffsetDateTime::now_utc().unix_timestamp_nanos()
        );
        let stopped = Arc::new(AtomicBool::new(false));
        let turn = Turn {
            id: id.clone(),
            agent: agent.to_owned(),
            handle: handle.to_owned(),
            asked_by: asked_by.to_owned(),
            started_at: OffsetDateTime::now_utc(),
            stopped: Arc::clone(&stopped),
        };
        if let Ok(mut live) = self.live.lock() {
            live.entry((tenant.as_str().to_owned(), channel.as_str().to_owned()))
                .or_default()
                .push(turn);
        }
        (id, stopped)
    }

    /// Forget a turn that has finished, however it finished.
    pub fn end(&self, tenant: &TenantId, channel: &ChatChannelId, id: &str) {
        if let Ok(mut live) = self.live.lock() {
            let key = (tenant.as_str().to_owned(), channel.as_str().to_owned());
            if let Some(turns) = live.get_mut(&key) {
                turns.retain(|t| t.id != id);
                if turns.is_empty() {
                    live.remove(&key);
                }
            }
        }
    }

    /// What is running in a room right now.
    #[must_use]
    pub fn in_room(&self, tenant: &TenantId, channel: &ChatChannelId) -> Vec<Turn> {
        self.live
            .lock()
            .ok()
            .and_then(|live| {
                live.get(&(tenant.as_str().to_owned(), channel.as_str().to_owned()))
                    .cloned()
            })
            .unwrap_or_default()
    }

    /// Stop a turn, if the caller is the person who asked for it.
    ///
    /// Returns `false` when there is no such turn here — already finished, or
    /// running on another process. A stop that finds nothing is not an error:
    /// the outcome the caller wanted (that turn not continuing) is true either
    /// way.
    #[must_use]
    pub fn stop(&self, tenant: &TenantId, channel: &ChatChannelId, id: &str, by: &str) -> bool {
        let Ok(live) = self.live.lock() else {
            return false;
        };
        let Some(turns) = live.get(&(tenant.as_str().to_owned(), channel.as_str().to_owned()))
        else {
            return false;
        };
        let Some(turn) = turns.iter().find(|t| t.id == id) else {
            return false;
        };
        if turn.asked_by != by {
            return false;
        }
        turn.stopped.store(true, Ordering::SeqCst);
        true
    }
}

/// The wire shape of a running turn. No `askedBy` beyond what the room can
/// already see — the point is to say something is happening, not to publish
/// who is waiting on what.
#[must_use]
pub fn turn_json(t: &Turn, me: &str) -> Value {
    json!({
        "id": t.id,
        "agent": t.agent,
        "handle": t.handle,
        // Whether this reader may stop it, so the client does not have to
        // reimplement the rule.
        "mine": t.asked_by == me,
    })
}
