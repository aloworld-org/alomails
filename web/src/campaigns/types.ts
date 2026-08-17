// The `/campaigns` surface as the screens see it (alo Campaigns, ADR 0044,
// wave C1).
//
// One thing here is a decision rather than a shape: a **segment is conditions,
// never people**. There is no member list on a saved segment and no cached
// count, because consent and suppression have to apply at the moment of asking
// rather than at the moment of saving — a stored list is how somebody who
// unsubscribed on Monday is mailed on Tuesday. So the editor holds
// `SegmentConditions`, the count is asked for those conditions, and a saved
// segment is the same conditions read back.

/** Which kind of record holds a person. Never a personal address book. */
export type AudienceSourceKind = "billing_customer" | "crm_deal" | "site_form";

/** How a tenant came to hold somebody's agreement. Wider than the audience's
 *  three, because an imported list is the dangerous path and has to be nameable
 *  as itself. */
export type ConsentSourceKind = AudienceSourceKind | "import" | "manual";

/** Why somebody may never be mailed again. */
export type SuppressionReasonKind = "unsubscribe" | "hard_bounce" | "complaint" | "manual";

/** The consent behind a person as the audience carries it: the record to read,
 *  not the statement itself. */
export interface ConsentEvidence {
  recordId: string;
  source: ConsentSourceKind;
  occurredAt: string;
}

/** Why somebody is out, as the audience carries it. */
export interface SuppressionEvidence {
  recordId: string;
  reason: SuppressionReasonKind;
  occurredAt: string;
}

/**
 * One person the tenant could reach — or could not, and the screen says which.
 *
 * `exclusionReason` is `null` exactly when `mailable` is true. Both come from
 * the server so the list and the count cannot disagree about one person; a
 * screen that re-derived either would eventually derive it differently.
 */
export interface AudienceMember {
  address: string;
  name: string | null;
  /** ISO 3166-1 alpha-2, and only a billing customer has one. `null` is
   *  *unknown*, which is why a country segment excludes rather than assumes. */
  country: string | null;
  sources: AudienceSourceKind[];
  firstSeenAt: string;
  lastSeenAt: string;
  consent: ConsentEvidence | null;
  suppression: SuppressionEvidence | null;
  mailable: boolean;
  /** `"no_consent"`, or `"suppressed:<reason>"`, or `null` when they will be
   *  mailed. */
  exclusionReason: string | null;
}

/** How many people one reason kept out of a send. */
export interface SegmentExclusion {
  reason: string;
  people: number;
}

/** What a question answers: who it may mail, and who it may not, with reasons.
 *  `matched - mailable` is exactly `excluded` summed — the arithmetic that
 *  makes the number auditable rather than merely large. */
export interface SegmentTally {
  mailable: number;
  matched: number;
  excluded: SegmentExclusion[];
}

/** Whether the segment wants people who have bought, or people who have not. */
export type PurchaseConditionKind = "bought" | "not_bought";

/** A purchase condition and the period it looks back over. `withinDays` of
 *  `null` is "ever", which is a real question and not a missing value. */
export interface PurchaseWindow {
  condition: PurchaseConditionKind;
  withinDays: number | null;
}

/** What a segment asks. Empty means everybody in the audience. */
export interface SegmentConditions {
  countries: string[];
  purchase: PurchaseWindow | null;
}

/** A saved question. `createdBy` is who to ask what it meant — never a claim
 *  that anybody it selects agreed to anything. */
export interface CampaignSegment {
  id: string;
  name: string;
  conditions: SegmentConditions;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

/** One recorded act of consent, whole — this is the answer to "how do we
 *  know", so it carries the statement the tenant wrote. */
export interface CampaignConsent {
  id: string;
  address: string;
  source: ConsentSourceKind;
  sourceRef: string | null;
  statement: string;
  recordedBy: string;
  occurredAt: string;
  recordedAt: string;
}

/** One address this tenant will not mail again. */
export interface CampaignSuppression {
  id: string;
  address: string;
  reason: SuppressionReasonKind;
  sourceRef: string | null;
  /** Whether a person decided this, or a mailbox did. A bounce says nothing
   *  about whether the mail was wanted. */
  personsDecision: boolean;
  occurredAt: string;
  recordedAt: string;
}

/** The empty question — everybody in the audience. */
export const NO_CONDITIONS: SegmentConditions = { countries: [], purchase: null };
