// What a domain purchase's state means to the person who started it, and
// nothing else (S2.15c3).
//
// The server owns the state machine — `quoted → approved → awaiting_payment →
// paid → registering → registered → configured`, with `failed` and `cancelled`
// as the other two ends. This module never advances it, never guesses the next
// one and never decides what may happen: it turns the state the server sent
// into the sentence a person reads, and answers the one question a screen has
// to ask before it draws a button — whether calling this purchase off is still
// possible, which is the server's own `moneyMoved`/`open` pair rather than a
// second list of states kept here.
import { strings } from "../i18n";
import type { SiteDomainPurchase, SiteDomainPurchaseState } from "./types";

/** The short name of a state, for a chip. */
export function purchaseStateLabel(state: SiteDomainPurchaseState): string {
  switch (state) {
    case "quoted":
      return strings.sitesDomainStateQuoted;
    case "approved":
      return strings.sitesDomainStateApproved;
    case "awaiting_payment":
      return strings.sitesDomainStateAwaitingPayment;
    case "paid":
      return strings.sitesDomainStatePaid;
    case "registering":
      return strings.sitesDomainStateRegistering;
    case "registered":
      return strings.sitesDomainStateRegistered;
    case "configured":
      return strings.sitesDomainStateConfigured;
    case "failed":
      return strings.sitesDomainStateFailed;
    case "cancelled":
      return strings.sitesDomainStateCancelled;
  }
}

/** What is happening, and what happens next — the sentence under the chip.
 *
 *  A failure shows the server's own words when it sent any: it names what the
 *  registrar refused and what it means for the money, which is exactly what a
 *  person needs and is not something this module could reconstruct. */
export function purchaseProgress(purchase: SiteDomainPurchase): string {
  switch (purchase.state) {
    case "quoted":
      return strings.sitesDomainStepQuoted;
    case "approved":
      return strings.sitesDomainStepApproved;
    case "awaiting_payment":
      return strings.sitesDomainStepAwaitingPayment;
    case "paid":
      return strings.sitesDomainStepPaid;
    case "registering":
      return strings.sitesDomainStepRegistering;
    case "registered":
      return strings.sitesDomainStepRegistered(purchase.domain);
    case "configured":
      return strings.sitesDomainStepConfigured(purchase.domain);
    case "failed":
      return purchase.failure ?? strings.sitesDomainStepFailed;
    case "cancelled":
      return strings.sitesDomainStepCancelled;
  }
}

/** Whether a purchase can still be called off from a button.
 *
 *  Both halves come from the server: `open` means it is still on its way
 *  somewhere, and `moneyMoved` is the line cancellation may not cross — past
 *  it, calling off is a refund conversation, and offering a button for it
 *  would promise something no route performs. */
export function canCancelPurchase(purchase: SiteDomainPurchase): boolean {
  return purchase.open && !purchase.moneyMoved;
}

/** Whether a purchase is still travelling under its own steam, so the screen
 *  offers a refresh rather than leaving somebody watching a stale row. The
 *  three machine states are the ones that change without anybody clicking. */
export function purchaseIsMoving(purchase: SiteDomainPurchase): boolean {
  return (
    purchase.state === "paid" ||
    purchase.state === "registering" ||
    purchase.state === "registered"
  );
}
