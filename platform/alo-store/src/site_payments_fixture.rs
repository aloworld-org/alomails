//! The payment provider that ships: in-memory, deterministic, and unable to
//! move money or open a socket (ADR 0041, item S3.04c).
//!
//! [`FixtureSitePayments`] is a complete implementation of
//! [`SitePaymentProvider`] — it mints payments, remembers them, replays
//! idempotency keys and answers status questions — and it **cannot charge
//! anybody**. That makes it usable in three places at once: the test suite,
//! local development, and the day a live Mollie or Adyen adapter is wired,
//! when the fixture's behaviour is the contract the new implementation is
//! held to.
//!
//! Deterministic on purpose: nothing here reads a clock or randomises
//! anything. A payment is [`SitePaymentStatus::Open`] from creation until a
//! test moves it with [`FixtureSitePayments::mark`] — the fixture's stand-in
//! for a buyer finishing, failing, cancelling or timing out on the hosted
//! page. Ids are `fixpay-<idempotency key>`: a pure function of the request,
//! so a transcript reads the same on every run, and — because the key is the
//! globally-unique order id — never a collision with what a previous test
//! run left in a shared database.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use crate::site_payments::{
    SitePaymentCreated, SitePaymentEnvironment, SitePaymentError, SitePaymentFuture,
    SitePaymentIdentity, SitePaymentProvider, SitePaymentRequest, SitePaymentResult,
    SitePaymentStatus,
};

/// A deterministic hosted-payment provider with no network and no money.
#[derive(Debug, Default)]
pub struct FixtureSitePayments {
    state: Mutex<FixtureState>,
}

#[derive(Debug, Default)]
struct FixtureState {
    /// Payment id → where it stands and the request that made it.
    payments: BTreeMap<String, StoredPayment>,
    /// Idempotency key → the fingerprint it was used with and the payment it
    /// produced. This is the whole of the replay contract.
    replays: BTreeMap<String, (String, String)>,
}

#[derive(Debug)]
struct StoredPayment {
    status: SitePaymentStatus,
    checkout_url: String,
}

/// One request reduced to the facts a replay must match on. Everything the
/// caller sent is part of the fingerprint: the same key with any field
/// changed is a conflict, never a quiet second charge.
fn fingerprint(request: &SitePaymentRequest) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        request.amount_cents,
        request.currency,
        request.description.trim(),
        request.redirect_url,
        request.webhook_url
    )
}

impl FixtureSitePayments {
    /// A fresh provider with no payments and its counter at zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves one payment to `status` — the fixture's stand-in for whatever
    /// the buyer did on the hosted page. Tests drive the world with this.
    ///
    /// # Errors
    /// [`SitePaymentError::Validation`] when the payment is not one this
    /// provider minted.
    pub fn mark(
        &self,
        provider_payment_id: &str,
        status: SitePaymentStatus,
    ) -> SitePaymentResult<()> {
        let mut state = self.lock();
        let payment = state
            .payments
            .get_mut(provider_payment_id)
            .ok_or_else(unknown_payment)?;
        payment.status = status;
        Ok(())
    }

    /// How many payments this provider has minted — a test's cheap proof
    /// that a replay did not create a second one.
    #[must_use]
    pub fn minted(&self) -> usize {
        self.lock().payments.len()
    }

    fn lock(&self) -> MutexGuard<'_, FixtureState> {
        match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn create(&self, request: &SitePaymentRequest) -> SitePaymentResult<SitePaymentCreated> {
        request.validate()?;
        let print = fingerprint(request);
        let mut state = self.lock();
        if let Some((seen, payment_id)) = state.replays.get(&request.idempotency_key) {
            if *seen != print {
                return Err(SitePaymentError::Conflict(
                    "that idempotency key was already used for a different payment".to_owned(),
                ));
            }
            let payment = state.payments.get(payment_id).ok_or_else(unknown_payment)?;
            return Ok(SitePaymentCreated {
                provider_payment_id: payment_id.clone(),
                checkout_url: payment.checkout_url.clone(),
            });
        }
        // The id is derived from the caller's key, not a counter: the
        // key is the order id, which is globally unique, so a payment id the
        // fixture mints today can never collide with one a previous test run
        // left in a shared database — the one-payment-one-order index is
        // global by design (it is the webhook's lookup key).
        let id = format!("fixpay-{}", request.idempotency_key);
        let checkout_url = format!("https://checkout.fixture.invalid/{id}");
        state.payments.insert(
            id.clone(),
            StoredPayment {
                status: SitePaymentStatus::Open,
                checkout_url: checkout_url.clone(),
            },
        );
        state
            .replays
            .insert(request.idempotency_key.clone(), (print, id.clone()));
        Ok(SitePaymentCreated {
            provider_payment_id: id,
            checkout_url,
        })
    }

    fn status(&self, provider_payment_id: &str) -> SitePaymentResult<SitePaymentStatus> {
        let state = self.lock();
        state
            .payments
            .get(provider_payment_id)
            .map(|payment| payment.status)
            .ok_or_else(unknown_payment)
    }
}

fn unknown_payment() -> SitePaymentError {
    SitePaymentError::Validation("that payment is not known to this provider".to_owned())
}

impl SitePaymentProvider for FixtureSitePayments {
    fn identity(&self) -> SitePaymentIdentity {
        SitePaymentIdentity {
            name: "fixture".to_owned(),
            country: "nl".to_owned(),
            environment: SitePaymentEnvironment::Fixture,
        }
    }

    fn create_payment(
        &self,
        request: SitePaymentRequest,
    ) -> SitePaymentFuture<'_, SitePaymentCreated> {
        Box::pin(async move { self.create(&request) })
    }

    fn payment_status(
        &self,
        provider_payment_id: String,
    ) -> SitePaymentFuture<'_, SitePaymentStatus> {
        Box::pin(async move { self.status(&provider_payment_id) })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn request(key: &str) -> SitePaymentRequest {
        SitePaymentRequest {
            idempotency_key: key.to_owned(),
            amount_cents: 8_500,
            currency: "EUR".to_owned(),
            description: "1 × Letterpress workshop".to_owned(),
            redirect_url: "https://axon.alosites.com/tickets/thanks".to_owned(),
            webhook_url: "https://axon.alosites.com/pay/webhook".to_owned(),
        }
    }

    #[tokio::test]
    async fn payments_are_minted_from_their_keys_and_start_open() {
        let provider = FixtureSitePayments::new();
        let first = provider
            .create_payment(request("order-0001"))
            .await
            .unwrap();
        let second = provider
            .create_payment(request("order-0002"))
            .await
            .unwrap();
        assert_eq!(first.provider_payment_id, "fixpay-order-0001");
        assert_eq!(second.provider_payment_id, "fixpay-order-0002");
        assert_eq!(
            first.checkout_url,
            "https://checkout.fixture.invalid/fixpay-order-0001"
        );
        assert_eq!(
            provider
                .payment_status("fixpay-order-0001".to_owned())
                .await
                .unwrap(),
            SitePaymentStatus::Open
        );
    }

    #[tokio::test]
    async fn the_replay_contract_holds() {
        let provider = FixtureSitePayments::new();
        let first = provider
            .create_payment(request("order-0001"))
            .await
            .unwrap();
        // Same key, same request: the payment already made, nothing minted.
        let again = provider
            .create_payment(request("order-0001"))
            .await
            .unwrap();
        assert_eq!(first, again);
        assert_eq!(provider.minted(), 1);
        // Same key, different request: refused, never a second charge.
        let mut moved = request("order-0001");
        moved.amount_cents = 9_999;
        assert!(matches!(
            provider.create_payment(moved).await,
            Err(SitePaymentError::Conflict(_))
        ));
        assert_eq!(provider.minted(), 1);
    }

    #[tokio::test]
    async fn marking_moves_a_payment_and_unknown_ids_refuse() {
        let provider = FixtureSitePayments::new();
        let created = provider
            .create_payment(request("order-0001"))
            .await
            .unwrap();
        provider
            .mark(&created.provider_payment_id, SitePaymentStatus::Paid)
            .unwrap();
        assert_eq!(
            provider
                .payment_status(created.provider_payment_id)
                .await
                .unwrap(),
            SitePaymentStatus::Paid
        );
        assert!(matches!(
            provider.mark("fixpay-99", SitePaymentStatus::Paid),
            Err(SitePaymentError::Validation(_))
        ));
        assert!(matches!(
            provider.payment_status("fixpay-99".to_owned()).await,
            Err(SitePaymentError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn a_malformed_request_is_refused_before_anything_is_minted() {
        let provider = FixtureSitePayments::new();
        let mut bad = request("order-0001");
        bad.redirect_url = "http://insecure.example".to_owned();
        assert!(matches!(
            provider.create_payment(bad).await,
            Err(SitePaymentError::Validation(_))
        ));
        assert_eq!(provider.minted(), 0);
    }

    #[test]
    fn the_fixture_cannot_spend_money() {
        assert!(
            !FixtureSitePayments::new()
                .identity()
                .environment
                .spends_money()
        );
    }
}
