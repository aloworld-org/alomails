//! The sweep that makes "go live on Monday at 09:00" actually happen
//! (ADR 0036, S2.05b).
//!
//! Runs as a background tick from `main.rs`, the same posture as the
//! form-notification sweep: [`alo_store::Store::claim_due_site_publishes`]
//! marks each due intention `publishing` in the statement that reads it
//! (at-most-once under concurrent sweepers — see that module's doc), and each
//! claim is then published **through the scheduling user's own account door**,
//! so a scheduled publish is the same operation, with the same tenant scope
//! and the same authorship, as the button in the editor.
//!
//! Nothing new can be published here that could not be published there: this
//! module adds a *moment*, never a second way to freeze a site.
//!
//! Two failure modes, deliberately answered differently:
//!
//! - A **refusal** (the site has no home page, a collection no longer
//!   resolves) is terminal. It is a statement about the site's content that
//!   retrying in ten minutes cannot change, so the reason is recorded verbatim
//!   for the owner to read and act on.
//! - An **infrastructure failure** (the database, a blob backend) leaves the
//!   claim standing. The claim goes stale and is re-offered, exactly like a
//!   worker that died mid-publish, and after
//!   [`alo_store::SITE_PUBLISH_MAX_ATTEMPTS`] the row is written off visibly
//!   as `failed` rather than left "publishing" forever.
//!
//! Nothing that reaches a log here carries site content or a person's data:
//! only the coarse error (Law 1).

use alo_store::{Store, StoreError};

/// How many due schedules one sweep round claims. A round that claims the full
/// batch is immediately followed by another in the same tick, so a backlog
/// (a deployment that was down over a scheduled moment) drains fast without an
/// unbounded single query.
const BATCH: i64 = 50;

/// Publishes every website whose chosen moment has arrived. Returns how many
/// went live.
pub async fn run_due(store: &Store) -> usize {
    let mut published = 0;
    loop {
        let due = match store.claim_due_site_publishes(BATCH).await {
            Ok(due) => due,
            Err(error) => {
                tracing::warn!(%error, "scheduled publish sweep: claim failed");
                return published;
            }
        };
        let batch_len = due.len();
        for item in due {
            // The scheduling user's door: the version records them as its
            // author, and a collaborator who has since lost their grant
            // publishes nothing.
            let account = store.for_account(item.tenant.clone(), item.requested_by.clone());
            match account.publish_site(&item.site).await {
                Ok(publish) => {
                    if let Err(error) = store
                        .finish_site_publish_schedule(&item.tenant, &item.schedule, &publish)
                        .await
                    {
                        // The site *is* live; only the bookkeeping failed. The
                        // stale claim is re-offered, and a second publish of an
                        // unchanged site is a harmless extra version.
                        tracing::warn!(%error, "scheduled publish sweep: could not record success");
                    } else {
                        published += 1;
                    }
                }
                Err(error) if is_infrastructure(&error) => {
                    // Leave the row claimed: the stale-claim path retries it,
                    // and gives up visibly once the attempts are used up.
                    tracing::warn!(
                        %error,
                        attempts = item.attempts,
                        "scheduled publish sweep: publish failed, will retry"
                    );
                }
                Err(error) => {
                    let reason = refusal_reason(&error);
                    if let Err(error) = store
                        .fail_site_publish_schedule(&item.tenant, &item.schedule, &reason)
                        .await
                    {
                        tracing::warn!(%error, "scheduled publish sweep: could not record refusal");
                    }
                }
            }
        }
        if batch_len < BATCH as usize {
            return published;
        }
    }
}

/// Whether the failure is about the machine rather than about the website. An
/// infrastructure failure is worth retrying; a refusal is not.
fn is_infrastructure(error: &StoreError) -> bool {
    matches!(
        error,
        StoreError::Db(_) | StoreError::Blob(_) | StoreError::Migrate(_) | StoreError::Crypto
    )
}

/// The sentence stored on the schedule for the owner to act on. A store
/// refusal already names the violated rule in words, without echoing another
/// tenant's data, so it is passed through as-is (without the error type's
/// `conflict:` prefix, which is machine vocabulary); the two failures that
/// would otherwise read as machine words get the owner's words instead.
fn refusal_reason(error: &StoreError) -> String {
    match error {
        StoreError::NotFound => "this website no longer exists".to_owned(),
        StoreError::Forbidden => {
            "the person who scheduled this publish can no longer edit this website".to_owned()
        }
        StoreError::Conflict(message) | StoreError::Validation(message) => message.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_failures_are_retried_and_refusals_are_not() {
        assert!(is_infrastructure(&StoreError::Db(sqlx::Error::PoolClosed)));
        assert!(!is_infrastructure(&StoreError::Conflict(
            "site has no home page".to_owned()
        )));
        assert!(!is_infrastructure(&StoreError::NotFound));
    }

    #[test]
    fn a_refusal_reads_as_a_sentence_the_owner_can_act_on() {
        assert_eq!(
            refusal_reason(&StoreError::Conflict("site has no home page".to_owned())),
            "site has no home page"
        );
        assert_eq!(
            refusal_reason(&StoreError::NotFound),
            "this website no longer exists"
        );
    }
}
