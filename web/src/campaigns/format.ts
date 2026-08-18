// Turning the campaigns wire vocabulary into sentences a colleague reads.
//
// One rule shapes all of it: **an exclusion is named, never counted silently.**
// The server sends `no_consent` or `suppressed:<reason>` and this file gives
// each one a sentence, because "88 excluded" tells somebody a number they
// cannot act on, while "62 never agreed, 26 unsubscribed" tells them which half
// is a job for the shop counter and which half is closed for good.
//
// It is presentation only. Nothing here decides who may be mailed, and nothing
// here recomputes a count — a browser that worked out its own total would
// eventually work out a different one from the server's.
import { strings } from "../i18n";
import type { AudienceSourceKind, PreviewAgainst } from "./types";

/** What one exclusion reason is called on screen. Unknown tokens fall back to
 *  the raw token rather than to a blank: a bucket this build cannot name is a
 *  real group of people, and hiding it would make the count stop adding up. */
export function exclusionLabel(token: string): string {
  switch (token) {
    case "no_consent":
      return strings.campaignsReasonNoConsent;
    case "suppressed:unsubscribe":
      return strings.campaignsReasonUnsubscribe;
    case "suppressed:hard_bounce":
      return strings.campaignsReasonHardBounce;
    case "suppressed:complaint":
      return strings.campaignsReasonComplaint;
    case "suppressed:manual":
      return strings.campaignsReasonManual;
    default:
      return token;
  }
}

/** Which kind of record holds somebody. Never a personal address book — no
 *  campaign query reads one, which is a property of the server's SQL and the
 *  reason there is no token for it here. */
export function sourceLabel(source: AudienceSourceKind): string {
  switch (source) {
    case "billing_customer":
      return strings.campaignsSourceBillingCustomer;
    case "crm_deal":
      return strings.campaignsSourceCrmDeal;
    case "site_form":
      return strings.campaignsSourceSiteForm;
    default:
      return source;
  }
}

/** How a person is introduced: their name when a record carries one, and their
 *  address otherwise. Never a name invented from the address — "Hi j.dupont" is
 *  worse than no greeting at all. */
export function personLabel(name: string | null, address: string): string {
  return name !== null && name.trim() !== "" ? name : address;
}

/** What one merge field is called on screen.
 *
 *  The names are the server's (they travel on the wire and go in the letter);
 *  the words are ours, in three languages. A name this build has no words for
 *  falls through to itself rather than to a blank — a field added on the server
 *  is a field a writer can already use, and hiding it would be worse than
 *  showing it untranslated. */
export function mergeFieldLabel(field: string): string {
  switch (field) {
    case "first_name":
      return strings.campaignsFieldFirstName;
    case "name":
      return strings.campaignsFieldName;
    case "email":
      return strings.campaignsFieldEmail;
    case "country":
      return strings.campaignsFieldCountry;
    default:
      return field;
  }
}

/** Whose copy of the letter is on screen, said in one sentence.
 *
 *  The two "nobody" cases are kept apart on purpose. *You asked for it* is a
 *  choice a writer made; *you have nobody to mail yet* is a fact about the
 *  workspace, and a screen that said the first when the second was true would
 *  be claiming a preview is representative when it is only all there is. */
export function previewAgainstLabel(against: PreviewAgainst): string {
  if (against.kind === "recipient") {
    return strings.campaignsAgainstRecipient(personLabel(against.name, against.address));
  }
  return against.reason === "nobody_to_mail_yet"
    ? strings.campaignsAgainstNobodyYet
    : strings.campaignsAgainstFallbacks;
}

/** The country codes a colleague typed, split and trimmed. Whether `XX` is a
 *  country is the server's rule, and its refusal is shown verbatim — this only
 *  turns one text box into a list. */
export function splitCountries(raw: string): string[] {
  return raw
    .split(",")
    .map((code) => code.trim())
    .filter((code) => code !== "");
}
