// Buying one domain: who it is registered to, for how long, and the two
// numbers that have to be agreed before anything is charged (S2.15c3).
//
// The dialog is deliberately two steps, and the second one is not a summary.
//
//   * **Step one asks, it does not price.** The body sent to the server
//     carries no price at all: the seller states the price in that same
//     request. Nothing here can propose what a domain costs.
//   * **Step two is the agreement.** It shows the stored quote — the total for
//     the first term and what one year costs every year afterwards — and
//     approving echoes those exact six values back. The server refuses an
//     approval whose quote has drifted, so a price that moved between the
//     screen and the charge stops there instead of being re-quoted quietly.
//
// Closing after step one strands nothing: the purchase exists as `quoted`, in
// the list below, where its price can still be approved or called off.
import { useState } from "react";
import { Globe } from "lucide-react";

import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { formatPrice } from "./catalogPricing";
import { DialogFrame, Field } from "./parts";
import type {
  DomainEnding,
  DomainOffer,
  DomainQuote,
  SiteDomainPurchase,
} from "./types";
import styles from "./SitesModule.module.css";

/** Registrar prices are integer cents of a two-decimal currency; the server
 *  states the currency beside every one of them. */
const CENTS = 2;

/** The registrant fields as the form asks for them, in the order a person
 *  writes an address. `organisation` is the only optional one. */
interface RegistrantForm {
  name: string;
  organisation: string;
  email: string;
  street: string;
  postalCode: string;
  city: string;
  country: string;
  phone: string;
}

const EMPTY_REGISTRANT: RegistrantForm = {
  name: "",
  organisation: "",
  email: "",
  street: "",
  postalCode: "",
  city: "",
  country: "",
  phone: "",
};

/** What the registry demands of the registrant, said before anybody types an
 *  address rather than discovered at the registry afterwards. */
function requirementNote(ending: DomainEnding | null): string | null {
  if (ending === null) return null;
  switch (ending.requirement.kind) {
    case "none":
      return null;
    case "eea_presence":
      return strings.sitesDomainRequirementEea;
    case "country_presence":
      return strings.sitesDomainRequirementCountry(
        ending.requirement.country.toUpperCase(),
      );
  }
}

/** The years a term may run, from the ending's own limits. */
function termChoices(ending: DomainEnding | null): number[] {
  const min = ending?.minYears ?? 1;
  const max = Math.max(min, ending?.maxYears ?? 1);
  return Array.from({ length: max - min + 1 }, (_, index) => min + index);
}

export function DomainPurchaseDialog({
  siteId,
  offer,
  ending,
  onClose,
  onApproved,
}: {
  siteId: string;
  /** The searched name and the price it came back with. */
  offer: DomainOffer;
  /** Its ending's catalog entry, for the term limits and the registry's
   *  demands. Null when the search answered for an ending the catalog no
   *  longer lists; the term then stays at one year. */
  ending: DomainEnding | null;
  onClose: () => void;
  /** The approved purchase, so the list behind the dialog can show it. */
  onApproved: (purchase: SiteDomainPurchase) => void;
}) {
  const api = useSitesApi();
  const terms = termChoices(ending);
  const [registrant, setRegistrant] = useState<RegistrantForm>(EMPTY_REGISTRANT);
  const [years, setYears] = useState<number>(terms[0] ?? 1);
  const [autoRenew, setAutoRenew] = useState(true);
  const [quoted, setQuoted] = useState<SiteDomainPurchase | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // One replay token for this dialog: a network wobble that hides a successful
  // create answers with the same purchase instead of buying a second name.
  const [requestKey] = useState(() => crypto.randomUUID());

  const requirement = requirementNote(ending);
  const complete =
    registrant.name.trim() !== "" &&
    registrant.email.trim() !== "" &&
    registrant.street.trim() !== "" &&
    registrant.postalCode.trim() !== "" &&
    registrant.city.trim() !== "" &&
    registrant.country.trim() !== "" &&
    registrant.phone.trim() !== "";

  function edit(field: keyof RegistrantForm, value: string) {
    setRegistrant((current) => ({ ...current, [field]: value }));
  }

  async function askThePrice() {
    setBusy(true);
    setError(null);
    try {
      setQuoted(
        await api.startDomainPurchase(siteId, {
          domain: offer.domain,
          years,
          autoRenew,
          requestKey,
          registrant: {
            name: registrant.name.trim(),
            // A blank company is no company, not an empty one.
            organisation:
              registrant.organisation.trim() === ""
                ? null
                : registrant.organisation.trim(),
            email: registrant.email.trim(),
            street: registrant.street.trim(),
            postalCode: registrant.postalCode.trim(),
            city: registrant.city.trim(),
            // The registry's code is lowercase; typing NL is not a mistake
            // worth a refusal.
            country: registrant.country.trim().toLowerCase(),
            phone: registrant.phone.trim(),
          },
        }),
      );
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesDomainQuoteFailed));
    } finally {
      setBusy(false);
    }
  }

  async function approve(purchase: SiteDomainPurchase) {
    setBusy(true);
    setError(null);
    try {
      // The six numbers exactly as this screen has them up.
      const agreed: DomainQuote = {
        domain: purchase.domain,
        termYears: purchase.termYears,
        currency: purchase.currency,
        firstTermCents: purchase.firstTermCents,
        renewalCentsPerYear: purchase.renewalCentsPerYear,
        premium: purchase.premium,
      };
      onApproved(await api.approveDomainPurchase(siteId, purchase.id, agreed));
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesDomainApproveFailed));
    } finally {
      setBusy(false);
    }
  }

  if (quoted !== null) {
    const today = formatPrice(quoted.firstTermCents, quoted.currency, CENTS);
    const renewal = formatPrice(quoted.renewalCentsPerYear, quoted.currency, CENTS);
    return (
      <DialogFrame
        Icon={Globe}
        title={strings.sitesDomainApproveTitle}
        subtitle={strings.sitesDomainApproveSubtitle(quoted.domain)}
        error={error}
        busy={busy}
        canSubmit
        submitLabel={strings.sitesDomainApproveAction(today)}
        onClose={onClose}
        onSubmit={() => void approve(quoted)}
      >
        <dl className={styles.domainQuote}>
          <div>
            <dt>{strings.sitesDomainQuoteName}</dt>
            <dd className={styles.mono}>{quoted.domain}</dd>
          </div>
          <div>
            <dt>{strings.sitesDomainQuoteTerm}</dt>
            <dd>{strings.sitesDomainYearsOption(quoted.termYears)}</dd>
          </div>
          <div>
            <dt>{strings.sitesDomainQuoteToday}</dt>
            <dd>
              <strong>{today}</strong>
            </dd>
          </div>
          <div>
            <dt>{strings.sitesDomainQuoteRenewal}</dt>
            <dd>{renewal}</dd>
          </div>
        </dl>
        {quoted.premium && <p className={styles.hint}>{strings.sitesDomainPremiumHint}</p>}
        <p className={styles.hint}>
          {quoted.autoRenew
            ? strings.sitesDomainAutoRenewOn
            : strings.sitesDomainAutoRenewOff}
        </p>
        <p className={styles.hint}>{strings.sitesDomainApproveHint}</p>
      </DialogFrame>
    );
  }

  return (
    <DialogFrame
      Icon={Globe}
      title={strings.sitesDomainPurchaseTitle(offer.domain)}
      subtitle={strings.sitesDomainPurchaseSubtitle}
      error={error}
      busy={busy}
      canSubmit={complete}
      submitLabel={strings.sitesDomainSeePrice}
      onClose={onClose}
      onSubmit={() => void askThePrice()}
    >
      {offer.quote !== null && (
        <p className={styles.domainOfferPrice}>
          {strings.sitesDomainPriceLine(
            formatPrice(offer.quote.firstTermCents, offer.quote.currency, CENTS),
            formatPrice(
              offer.quote.renewalCentsPerYear,
              offer.quote.currency,
              CENTS,
            ),
          )}
        </p>
      )}

      <Field label={strings.sitesDomainYears} hint={strings.sitesDomainYearsHint}>
        <select
          className={styles.input}
          value={years}
          disabled={busy}
          onChange={(event) => setYears(Number(event.target.value))}
        >
          {terms.map((term) => (
            <option key={term} value={term}>
              {strings.sitesDomainYearsOption(term)}
            </option>
          ))}
        </select>
      </Field>

      <label className={styles.domainRenewToggle}>
        <input
          type="checkbox"
          checked={autoRenew}
          disabled={busy}
          onChange={(event) => setAutoRenew(event.target.checked)}
        />
        <span>{strings.sitesDomainAutoRenew}</span>
      </label>
      <p className={styles.hint}>{strings.sitesDomainAutoRenewHint}</p>

      <h3 className={styles.domainFormHeading}>{strings.sitesDomainRegistrant}</h3>
      <p className={styles.hint}>{strings.sitesDomainRegistrantHint}</p>
      {requirement !== null && <p className={styles.hint}>{requirement}</p>}

      <Field label={strings.sitesDomainRegistrantName}>
        <input
          className={styles.input}
          value={registrant.name}
          autoComplete="name"
          disabled={busy}
          onChange={(event) => edit("name", event.target.value)}
        />
      </Field>
      <Field label={strings.sitesDomainRegistrantOrganisation}>
        <input
          className={styles.input}
          value={registrant.organisation}
          autoComplete="organization"
          disabled={busy}
          onChange={(event) => edit("organisation", event.target.value)}
        />
      </Field>
      <Field
        label={strings.sitesDomainRegistrantEmail}
        hint={strings.sitesDomainRegistrantEmailHint}
      >
        <input
          className={styles.input}
          type="email"
          value={registrant.email}
          autoComplete="email"
          disabled={busy}
          onChange={(event) => edit("email", event.target.value)}
        />
      </Field>
      <Field label={strings.sitesDomainRegistrantStreet}>
        <input
          className={styles.input}
          value={registrant.street}
          autoComplete="street-address"
          disabled={busy}
          onChange={(event) => edit("street", event.target.value)}
        />
      </Field>
      <Field label={strings.sitesDomainRegistrantPostalCode}>
        <input
          className={styles.input}
          value={registrant.postalCode}
          autoComplete="postal-code"
          disabled={busy}
          onChange={(event) => edit("postalCode", event.target.value)}
        />
      </Field>
      <Field label={strings.sitesDomainRegistrantCity}>
        <input
          className={styles.input}
          value={registrant.city}
          autoComplete="address-level2"
          disabled={busy}
          onChange={(event) => edit("city", event.target.value)}
        />
      </Field>
      <Field
        label={strings.sitesDomainRegistrantCountry}
        hint={strings.sitesDomainRegistrantCountryHint}
      >
        <input
          className={styles.input}
          value={registrant.country}
          autoComplete="country"
          maxLength={2}
          disabled={busy}
          onChange={(event) => edit("country", event.target.value)}
        />
      </Field>
      <Field
        label={strings.sitesDomainRegistrantPhone}
        hint={strings.sitesDomainRegistrantPhoneHint}
      >
        <input
          className={styles.input}
          type="tel"
          value={registrant.phone}
          autoComplete="tel"
          disabled={busy}
          onChange={(event) => edit("phone", event.target.value)}
        />
      </Field>
    </DialogFrame>
  );
}
