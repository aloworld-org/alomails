//! Push (RFC 8620 §7.3): a per-tenant broadcast fan-out and the
//! `text/event-stream` EventSource endpoint. Each connection is
//! authenticated to one tenant and subscribes to **that tenant's**
//! channel only, so a tenant's stream is structurally silent about other
//! tenants (an isolation surface — tested).

use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use alo_store::{TenantId, UserId};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use serde_json::{Map, json};
use tokio::sync::broadcast;

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The synthetic change type signalling that a user's *set of delegated
/// mailboxes* changed (a grant was added or revoked) — distinct from a data
/// change to a mailbox. Their live stream re-subscribes on it and their client
/// re-lists shared mailboxes, so a new grant goes live with no refresh.
pub const TYPE_DELEGATION: &str = "Delegation";

/// The signal that something in alo Chat changed — a message arrived, words
/// were changed or withdrawn, a room appeared, or membership moved.
///
/// One type is deliberately enough (`docs/design/chat.md`): the client
/// refetches `/chat/channels`, which carries every room's unread count and
/// last sequence, and pulls messages only for the room it has open. Chat needs
/// no second transport — it rides the stream the workspace already keeps open
/// for mail (ADR 0038).
pub const TYPE_CHAT: &str = "Chat";

/// Publishes a [`TYPE_CHAT`] signal to each of `users`, on their own streams.
///
/// Best-effort by design: a chat write never fails because a notification
/// could not be delivered — the client would refetch on its next poll anyway.
pub async fn notify_chat(state: &AppState, tenant: &TenantId, users: &[UserId]) {
    let mut seen: Vec<&str> = Vec::with_capacity(users.len());
    for user in users {
        if seen.contains(&user.as_str()) {
            continue;
        }
        seen.push(user.as_str());
        let account_state = state
            .store
            .for_account(tenant.clone(), user.clone())
            .state()
            .await
            .unwrap_or_default();
        state.push.publish(
            tenant.as_str(),
            StateChangeMsg {
                account_id: user.as_str().to_owned(),
                types: vec![TYPE_CHAT],
                state: account_state,
            },
        );
    }
}

/// Publishes a [`TYPE_DELEGATION`] signal to `delegate_id`'s own stream after
/// their grants change, so it takes effect immediately (ADR 0017).
pub async fn notify_delegation_change(state: &AppState, tenant: &TenantId, delegate_id: &str) {
    let account_state = state
        .store
        .for_account(tenant.clone(), UserId::new(delegate_id))
        .state()
        .await
        .unwrap_or_default();
    state.push.publish(
        tenant.as_str(),
        StateChangeMsg {
            account_id: delegate_id.to_owned(),
            types: vec![TYPE_DELEGATION],
            state: account_state,
        },
    );
}

/// The delegated-mailbox account ids a `delegate` currently listens for on their
/// stream: their own account plus every mailbox they hold a grant on.
async fn listen_set(
    state: &AppState,
    tenant: &TenantId,
    own_id: &str,
    delegate: &UserId,
) -> HashSet<String> {
    let mut ids: HashSet<String> = HashSet::from([own_id.to_owned()]);
    if let Ok(dels) = state
        .store
        .for_tenant(tenant.clone())
        .delegations_for(delegate)
        .await
    {
        for (owner_id, _email, _can_write, _send_mode) in dels {
            ids.insert(owner_id);
        }
    }
    ids
}

/// A state-change notification for one account.
#[derive(Debug, Clone)]
pub struct StateChangeMsg {
    /// The account (user id) whose data changed.
    pub account_id: String,
    /// The JMAP types that changed (`Mailbox`/`Email`/`Thread`).
    pub types: Vec<&'static str>,
    /// The new opaque state string (shared tenant modseq).
    pub state: String,
}

/// A per-tenant broadcast hub. Channels are created lazily on first
/// subscribe; publishing to a tenant with no subscribers is a no-op.
#[derive(Clone, Default)]
pub struct PushHub {
    inner: Arc<Mutex<HashMap<String, broadcast::Sender<StateChangeMsg>>>>,
    /// The Web Push tap (mail M5.3): every published change is also handed
    /// to the dispatcher that wakes CLOSED apps, so it deliberately does not
    /// depend on anyone holding an EventSource open — the broadcast half
    /// serves the connected, this half serves the absent.
    tap: Arc<Mutex<Option<TapSender>>>,
}

/// The Web Push tap's feed: a tenant id and the change published to it.
pub type TapSender = tokio::sync::mpsc::UnboundedSender<(String, StateChangeMsg)>;

impl PushHub {
    /// A fresh hub.
    pub fn new() -> Self {
        Self::default()
    }

    fn sender(&self, tenant: &str) -> broadcast::Sender<StateChangeMsg> {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        map.entry(tenant.to_owned())
            .or_insert_with(|| broadcast::channel(256).0)
            .clone()
    }

    /// Routes every future publish into `tx` as well (the Web Push
    /// dispatcher's feed). One tap: wiring a second replaces the first.
    pub fn set_tap(&self, tx: TapSender) {
        let mut tap = self.tap.lock().unwrap_or_else(|p| p.into_inner());
        *tap = Some(tx);
    }

    /// Publishes a change to a tenant's channel (no-op if nobody listens)
    /// and to the Web Push tap when one is wired.
    pub fn publish(&self, tenant: &str, msg: StateChangeMsg) {
        {
            let tap = self.tap.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(tx) = tap.as_ref() {
                let _ = tx.send((tenant.to_owned(), msg.clone()));
            }
        }
        let map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(tx) = map.get(tenant) {
            let _ = tx.send(msg);
        }
    }

    /// Subscribes to a tenant's channel.
    pub fn subscribe(&self, tenant: &str) -> broadcast::Receiver<StateChangeMsg> {
        self.sender(tenant).subscribe()
    }
}

/// `GET {eventSourceUrl}` — a `text/event-stream` emitting `StateChange`
/// events for this account, with keep-alive heartbeats.
pub async fn event_source(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    // The accounts this connection listens for: the user's own, plus any shared
    // mailboxes they were delegated (ADR 0017), so a change made by another
    // delegate reaches this client live. Re-evaluated in-stream whenever a
    // TYPE_DELEGATION signal arrives, so a grant added mid-connection goes live
    // immediately — no reconnect.
    let own_id = account.account_id().to_owned();
    let account_ids = listen_set(&state, &account.tenant, &own_id, &account.user).await;
    let rx = state.push.subscribe(account.tenant.as_str());

    let seed = (rx, account_ids, state, account.tenant, account.user, own_id);
    let stream = futures::stream::unfold(
        seed,
        move |(mut rx, mut account_ids, state, tenant, user, own_id)| async move {
            loop {
                match rx.recv().await {
                    // A grant of the signed-in user changed — rebuild the listen
                    // set before forwarding, so a newly-granted mailbox's changes
                    // start passing the filter right away.
                    Ok(msg) if msg.account_id == own_id && msg.types.contains(&TYPE_DELEGATION) => {
                        account_ids = listen_set(&state, &tenant, &own_id, &user).await;
                        let event = Event::default()
                            .event("state")
                            .id(msg.state.clone())
                            .data(state_change_json(&msg).to_string());
                        return Some((
                            Ok::<_, Infallible>(event),
                            (rx, account_ids, state, tenant, user, own_id),
                        ));
                    }
                    Ok(msg) if account_ids.contains(&msg.account_id) => {
                        let event = Event::default()
                            .event("state")
                            .id(msg.state.clone())
                            .data(state_change_json(&msg).to_string());
                        return Some((
                            Ok::<_, Infallible>(event),
                            (rx, account_ids, state, tenant, user, own_id),
                        ));
                    }
                    // Another account in the same tenant, or a lag skip.
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    );

    Ok(Sse::new(Box::pin(stream))
        .keep_alive(KeepAlive::default())
        .into_response())
}

/// The RFC 8620 §7.1 `StateChange` object for one account. Shared with the
/// Web Push dispatcher (mail M5.3), whose payloads are this object and
/// nothing more — type names, an account id and an opaque state string,
/// never message content.
pub(crate) fn state_change_json(msg: &StateChangeMsg) -> serde_json::Value {
    let mut types = Map::new();
    for t in &msg.types {
        types.insert((*t).to_owned(), json!(msg.state));
    }
    let mut changed = Map::new();
    changed.insert(msg.account_id.clone(), serde_json::Value::Object(types));
    json!({ "@type": "StateChange", "changed": changed })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use tokio::sync::broadcast::error::TryRecvError;

    #[tokio::test]
    async fn push_is_per_tenant_isolated() {
        let hub = PushHub::new();
        let mut a = hub.subscribe("tenant-a");
        let mut b = hub.subscribe("tenant-b");
        hub.publish(
            "tenant-a",
            StateChangeMsg {
                account_id: "user-a".to_owned(),
                types: vec!["Email"],
                state: "7".to_owned(),
            },
        );
        // Tenant A's stream receives it; tenant B's is silent.
        let got = a.try_recv().unwrap();
        assert_eq!(got.account_id, "user-a");
        assert!(matches!(b.try_recv(), Err(TryRecvError::Empty)));
    }
}
