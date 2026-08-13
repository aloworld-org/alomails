//! The sweep that turns a paid domain purchase into a name answering for a
//! website (ADR 0036, S2.15c2).
//!
//! Runs as a background tick from `main.rs`, the same posture as the
//! scheduled-publish sweep, and only in a deployment that sells domains at all
//! ([`crate::sites_domain_purchases::SiteDomainCommerce::sells_domains`]).
//! [`alo_store::Store::claim_site_domain_registrations`] marks each paid
//! purchase `registering` in the statement that reads it — at-most-once under
//! concurrent sweepers — and hands over the order, including the registrant it
//! is the only reader of.
//!
//! **The money already moved.** That one fact shapes every decision here:
//!
//! - The registrar call carries the purchase id as its idempotency key, so a
//!   sweep that dies after the registry answered registers nothing the second
//!   time; the replay returns the same name.
//! - A fault that repeating could survive — a provider timeout, a registry
//!   briefly down, a deployment whose registrar was unwired under it — puts the
//!   row back in the queue ([`alo_store::Store::retry_site_domain_registration`]),
//!   bounded by `SITE_DOMAIN_PURCHASE_MAX_ATTEMPTS` so it ends visibly rather
//!   than circling forever.
//! - A refusal — the name went while the payment was in flight, the registry
//!   rejected the registrant — is terminal
//!   ([`alo_store::Store::fail_site_domain_registration`]) with the registrar's
//!   own sentence, because a person now has to refund or fix something and a
//!   retry loop would only delay them finding out.
//!
//! **A registered name attaches itself.** The whole point of buying inside alo
//! is that the website is live afterwards without a DNS lesson, so a successful
//! registration is followed immediately by
//! [`alo_store::AccountStore::configure_site_domain_purchase`], which writes the
//! custom-domain claim straight to `live` (alo registered the name, on alo's
//! nameservers — there is nothing left to prove by TXT record). That call is
//! made through the door of the person who approved the price, like the
//! publish sweep publishes through the scheduler's.
//!
//! An attachment that is refused leaves the purchase `registered`: the name is
//! genuinely held for this tenant, and saying `failed` because it could not be
//! pointed at this particular website would be a lie about somebody's money.
//!
//! Nothing that reaches a log here carries a domain, a registrant or any of a
//! person's data: only the coarse error and the purchase id (Law 1).

use std::sync::Arc;

use alo_store::{
    DomainRegistrar, DueSiteDomainRegistration, RegisteredDomain, RegistrarError,
    SiteDomainPurchaseKind, Store,
};

/// How many paid purchases one sweep round claims. Small: each one is a call to
/// a registrar, and a backlog of domain registrations is not a thing that
/// happens quietly at scale.
const BATCH: i64 = 10;

/// Registers every paid domain purchase and attaches what it registered.
/// Returns how many names ended up live on their website.
pub async fn run_due(store: &Store, registrar: &Arc<dyn DomainRegistrar>) -> usize {
    let mut configured = 0;
    loop {
        let due = match store.claim_site_domain_registrations(BATCH).await {
            Ok(due) => due,
            Err(error) => {
                tracing::warn!(%error, "domain registration sweep: claim failed");
                return configured;
            }
        };
        let batch_len = due.len();
        for item in due {
            if register_one(store, registrar, &item).await {
                configured += 1;
            }
        }
        if batch_len < BATCH as usize {
            return configured;
        }
    }
}

/// One claimed purchase, from the registrar call to the live name. `true` when
/// the website ended up serving from it.
async fn register_one(
    store: &Store,
    registrar: &Arc<dyn DomainRegistrar>,
    item: &DueSiteDomainRegistration,
) -> bool {
    let registered = match place(registrar, item).await {
        Ok(registered) => registered,
        Err(error) => {
            record_failure(store, item, &error).await;
            return false;
        }
    };
    if let Err(error) = store
        .complete_site_domain_registration(&item.tenant, &item.purchase, &registered)
        .await
    {
        // The name IS registered. Only the bookkeeping failed, so the claim is
        // left to go stale and be re-offered; the registrar call replays under
        // the same key and answers with the same name rather than buying a
        // second one.
        tracing::warn!(
            %error,
            purchase = item.purchase.as_str(),
            "domain registration sweep: could not record a registration"
        );
        return false;
    }
    attach(store, item).await
}

/// Places the order with the reseller: a registration buys a name, a renewal
/// extends one. Both replay under the purchase id.
async fn place(
    registrar: &Arc<dyn DomainRegistrar>,
    item: &DueSiteDomainRegistration,
) -> Result<RegisteredDomain, RegistrarError> {
    match item.kind {
        SiteDomainPurchaseKind::Registration => registrar.register(item.order.clone()).await,
        SiteDomainPurchaseKind::Renewal => {
            registrar
                .renew(
                    item.order.domain.clone(),
                    item.order.years,
                    item.order.idempotency_key.clone(),
                )
                .await
        }
    }
}

/// Attaches a registered name to its website, through the door of whoever
/// approved the price. `true` when the site is now serving from it.
async fn attach(store: &Store, item: &DueSiteDomainRegistration) -> bool {
    let approver = match store
        .site_domain_purchase_approver(&item.tenant, &item.purchase)
        .await
    {
        Ok(approver) => approver,
        Err(error) => {
            tracing::warn!(
                %error,
                purchase = item.purchase.as_str(),
                "domain registration sweep: registered, but nobody to attach it as"
            );
            return false;
        }
    };
    match store
        .for_account(item.tenant.clone(), approver)
        .configure_site_domain_purchase(&item.purchase)
        .await
    {
        Ok(_) => true,
        Err(error) => {
            // Left `registered`: the tenant holds the name, and the editor's
            // domain screen can point it at a website. Failing the purchase
            // here would say the money bought nothing.
            tracing::warn!(
                %error,
                purchase = item.purchase.as_str(),
                "domain registration sweep: registered, not attached"
            );
            false
        }
    }
}

/// Writes a registrar refusal onto the purchase — as "ask again" or as "a
/// person has to look at this", which is the only distinction that matters
/// once somebody has been charged.
async fn record_failure(store: &Store, item: &DueSiteDomainRegistration, error: &RegistrarError) {
    let reason = failure_sentence(error);
    let outcome = if retryable(error) {
        store
            .retry_site_domain_registration(&item.tenant, &item.purchase, &reason)
            .await
            .map(|state| state.as_str())
    } else {
        store
            .fail_site_domain_registration(&item.tenant, &item.purchase, &reason)
            .await
            .map(|()| "failed")
    };
    match outcome {
        Ok(state) => tracing::warn!(
            purchase = item.purchase.as_str(),
            attempts = item.attempts,
            state,
            "domain registration sweep: the registrar refused"
        ),
        Err(error) => tracing::warn!(
            %error,
            purchase = item.purchase.as_str(),
            "domain registration sweep: could not record a refusal"
        ),
    }
}

/// Whether repeating this exact order could succeed.
///
/// [`RegistrarError::Unconfigured`] counts: a deployment that lost its
/// registrar under a paid purchase will have it back, and writing the purchase
/// off for a configuration mistake would turn an operator's afternoon into a
/// refund. It is bounded like every other retry, so it still ends in a
/// sentence somebody reads.
fn retryable(error: &RegistrarError) -> bool {
    match error {
        RegistrarError::Provider { retryable, .. } => *retryable,
        RegistrarError::Unconfigured => true,
        _ => false,
    }
}

/// The sentence stored on the purchase for the tenant to read.
///
/// [`RegistrarError`]'s own contract is that no variant carries registrant data
/// or a provider's raw response, so its `Display` is safe to store and show;
/// the two that would read as machine vocabulary get the buyer's words instead.
fn failure_sentence(error: &RegistrarError) -> String {
    match error {
        RegistrarError::Unavailable => {
            "that name was taken while the payment was going through; alo support can \
             refund it or buy another"
                .to_owned()
        }
        RegistrarError::Unconfigured => {
            "registering domains is not configured on this alo deployment right now; \
             this purchase will be tried again"
                .to_owned()
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_fault_that_could_pass_is_retried() {
        assert!(retryable(&RegistrarError::Provider {
            retryable: true,
            message: "timeout".to_owned(),
        }));
        assert!(retryable(&RegistrarError::Unconfigured));
        assert!(!retryable(&RegistrarError::Provider {
            retryable: false,
            message: "rejected".to_owned(),
        }));
        assert!(!retryable(&RegistrarError::Unavailable));
        assert!(!retryable(&RegistrarError::Validation(
            "the registrant telephone must be in international form".to_owned()
        )));
        assert!(!retryable(&RegistrarError::Conflict(
            "that idempotency key was already used for a different order".to_owned()
        )));
    }

    #[test]
    fn a_failure_reads_as_something_the_buyer_can_act_on() {
        let taken = failure_sentence(&RegistrarError::Unavailable);
        assert!(taken.contains("refund"), "{taken}");
        // Never a domain, never a person: the sentence is about the situation.
        assert!(!taken.contains('@'), "{taken}");
        assert_eq!(
            failure_sentence(&RegistrarError::Validation(
                "the registrant city must be 1-120 characters".to_owned()
            )),
            "invalid input: the registrant city must be 1-120 characters"
        );
    }
}
