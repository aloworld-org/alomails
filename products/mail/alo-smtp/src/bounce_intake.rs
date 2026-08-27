//! The campaign return path (queue item M4.4, ADR 0044 §4): what happens to
//! a message after the MX accepted it for the configured bounce address.
//!
//! One message in, three possible acts:
//!
//! - an RFC 3464 report of a settled permanent failure suppresses the bounced
//!   address in every tenant whose campaign mail actually went to it — through
//!   [`suppress_campaign_address`](alo_store::TenantStore::suppress_campaign_address),
//!   the C1.3 seam, which is idempotent and keeps the first reason;
//! - a report of a transient condition is recorded and deliberately not acted
//!   on (soft failures retry; only a settled one suppresses — ADR 0044 §4);
//! - anything else is recorded whole, never crashed on, because the return
//!   path of a public address receives whatever the internet sends it.
//!
//! Ordering is the idempotence: suppress first, record the receipt last. A
//! fault anywhere defers the message (the sender retries), and the retry
//! re-runs the idempotent suppressions before writing the one receipt on the
//! attempt that gets answered `250`.
//!
//! Nothing here logs an address or a message byte — a bounced recipient is
//! personal data, and the receipt row is where the operator reads the story.

use alo_store::{
    BounceVerdict, CampaignBounceId, NewCampaignBounce, NewSuppression, Store, StoreError,
    SuppressionReason, normalise_address,
};

use crate::dsn_parse::{DsnVerdict, classify, parse_dsn};

/// Takes in one message addressed to the campaign return path: parses,
/// suppresses what must be suppressed, and records the receipt — whose id is
/// returned, so what one message did can be read back.
///
/// # Errors
/// [`StoreError`] on any store fault — the caller answers the sender with a
/// transient failure so nothing is lost, and the retry is safe (see the
/// module docs for the ordering that makes it so).
pub async fn intake_campaign_bounce(
    store: &Store,
    message: &[u8],
) -> Result<CampaignBounceId, StoreError> {
    let report = parse_dsn(message);
    let (verdict, recipient, status, suppressed) = match &report {
        None => (BounceVerdict::None, None, None, 0),
        Some(recipients) => act_on_report(store, recipients).await?,
    };
    let id = store
        .record_campaign_bounce(&NewCampaignBounce {
            verdict,
            recipient: recipient.as_deref(),
            status: status.as_deref(),
            suppressed,
            message,
        })
        .await?;
    tracing::info!(
        bounce = %id,
        verdict = verdict.as_str(),
        suppressed,
        "campaign return path took in a message"
    );
    Ok(id)
}

/// Applies a parsed report: one suppression per (hard-bounced address ×
/// tenant that mailed it). Returns what the receipt should say — the overall
/// verdict is the worst reported, and the recorded recipient/status are the
/// ones that verdict is about.
async fn act_on_report(
    store: &Store,
    recipients: &[crate::dsn_parse::DsnRecipient],
) -> Result<(BounceVerdict, Option<String>, Option<String>, i32), StoreError> {
    let mut verdict = BounceVerdict::None;
    let mut recorded: Option<(String, Option<String>)> = None;
    let mut suppressed = 0i32;
    for recipient in recipients {
        match classify(recipient) {
            DsnVerdict::Hard => {
                if verdict != BounceVerdict::Hard {
                    verdict = BounceVerdict::Hard;
                    recorded = Some((recipient.address.clone(), recipient.status.clone()));
                }
                suppressed += suppress_everywhere_it_was_mailed(store, recipient).await?;
            }
            DsnVerdict::Soft => {
                if verdict == BounceVerdict::None {
                    verdict = BounceVerdict::Soft;
                    recorded = Some((recipient.address.clone(), recipient.status.clone()));
                }
            }
            DsnVerdict::Ignore => {}
        }
    }
    // The receipt holds the address in the same fold the suppression list
    // uses; one the fold refuses is not stored (the raw bytes are).
    let (recipient, status) = match recorded {
        Some((address, status)) => (normalise_address(&address), status),
        None => (None, None),
    };
    Ok((verdict, recipient, status, suppressed))
}

/// Suppresses one hard-bounced address in every tenant whose campaign mail
/// went to it. A tenant that never mailed the address — or an address the
/// audience fold refuses — suppresses nowhere: the report is somebody else's
/// mail, and acting on it would let a fabricated DSN silence an arbitrary
/// address (RFC 3464 reports are unauthenticated by nature).
async fn suppress_everywhere_it_was_mailed(
    store: &Store,
    recipient: &crate::dsn_parse::DsnRecipient,
) -> Result<i32, StoreError> {
    let tenants = store
        .tenants_with_sent_campaign_recipient(&recipient.address)
        .await?;
    let source_ref = match &recipient.status {
        Some(status) => format!("dsn {status}"),
        None => "dsn".to_owned(),
    };
    let mut suppressed = 0;
    for tenant in tenants {
        store
            .for_tenant(tenant)
            .suppress_campaign_address(&NewSuppression {
                address: &recipient.address,
                reason: SuppressionReason::HardBounce,
                source_ref: Some(&source_ref),
                // Now, not the report's own date: an intake is handled the
                // moment it arrives, and a forged past date is worth nothing.
                occurred_at: None,
            })
            .await?;
        suppressed += 1;
    }
    Ok(suppressed)
}
