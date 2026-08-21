// The client for the `/campaigns` HTTP surface (alo Campaigns, ADR 0044,
// wave C1).
//
// Its own small client, for the reason Billing and CRM each have one: this is a
// plain REST surface with none of JMAP's envelope, and it uses the same
// authenticated fetch, so there is one session and not two.
//
// It holds **no rule about who may be mailed**. Consent, suppression and the
// promise that a personal address book is never a source are properties of the
// server's SQL; this file sends a question and shows the answer. In particular
// it never filters a returned list — the people who will not be mailed come
// back deliberately, each carrying the reason, because a count whose exclusions
// were dropped in the browser would be a count nobody could audit.
import { useMemo } from "react";

import { useAuth } from "../auth";
import { strings } from "../i18n";
import { API_BASE } from "../platform/runtime";
import { RestError, problemDetail, restMessage } from "../platform/rest";
import type {
  AudienceMember,
  CampaignConsent,
  CampaignPreview,
  CampaignSegment,
  CampaignSummary,
  CampaignSuppression,
  CampaignTestDraft,
  PreviewAgainst,
  SegmentConditions,
  SegmentTally,
} from "./types";

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/** A failed campaigns request, carrying the server's own `Problem` detail. */
export class CampaignsError extends RestError {
  constructor(status: number, detail: string | null) {
    super(status, detail, "CampaignsError");
  }
}

/** What to show a user about a failed request: the server's sentence when it
 *  sent one, `fallback` otherwise. */
export function campaignsMessage(error: unknown, fallback: string): string {
  return restMessage(error, fallback);
}

/**
 * A question as query parameters.
 *
 * Countries are comma-separated because a repeated key would keep only the last
 * one and silently narrow the question. A period is only ever sent with its
 * condition — the server refuses a lone `withinDays` rather than answering the
 * wider question, and this keeps the client from asking for that refusal.
 */
export function conditionsQuery(conditions: SegmentConditions): URLSearchParams {
  const query = new URLSearchParams();
  if (conditions.countries.length > 0) query.set("countries", conditions.countries.join(","));
  if (conditions.purchase !== null) {
    query.set("purchase", conditions.purchase.condition);
    if (conditions.purchase.withinDays !== null) {
      query.set("withinDays", String(conditions.purchase.withinDays));
    }
  }
  return query;
}

/** The audience, the questions asked of it, and the two records that decide who
 *  may be mailed. One instance per auth context. */
export class CampaignsApi {
  readonly #fetch: AuthorizedFetch;

  constructor(authorizedFetch: AuthorizedFetch) {
    this.#fetch = authorizedFetch;
  }

  /**
   * One page of the people a question selects — **everybody** it selects,
   * mailable or not.
   *
   * `after` is the last address of the previous page: keyset, not an offset,
   * because the audience is a live query over three moving tables and an offset
   * would silently skip somebody when a form is submitted mid-walk.
   */
  audience(
    conditions: SegmentConditions,
    page: { after?: string; limit?: number } = {},
  ): Promise<AudienceMember[]> {
    const query = conditionsQuery(conditions);
    if (page.after !== undefined && page.after !== "") query.set("after", page.after);
    if (page.limit !== undefined) query.set("limit", String(page.limit));
    return this.#read<{ people?: AudienceMember[] }>(`/campaigns/audience?${query.toString()}`).then(
      (r) => r.people ?? [],
    );
  }

  /** How many people the question reaches, and who it leaves out with the
   *  reason. Asked for conditions rather than for a saved id, so the number can
   *  move while the question is still being typed. */
  tally(conditions: SegmentConditions): Promise<SegmentTally> {
    return this.#read<{ tally: SegmentTally }>(
      `/campaigns/audience/tally?${conditionsQuery(conditions).toString()}`,
    ).then((r) => r.tally);
  }

  /** One person's provenance, freshest first. An empty list is a complete
   *  answer: this tenant holds no evidence, which is why they are not a
   *  recipient. */
  consentFor(address: string): Promise<CampaignConsent[]> {
    return this.#read<{ consent?: CampaignConsent[] }>(
      `/campaigns/consent/${encodeURIComponent(address)}`,
    ).then((r) => r.consent ?? []);
  }

  /** Records that somebody agreed, with the provenance that makes it evidence.
   *  Append-only: a second record joins the first rather than replacing it. */
  recordConsent(consent: {
    address: string;
    source: string;
    sourceRef?: string;
    statement: string;
    occurredAt?: string;
  }): Promise<CampaignConsent> {
    return this.#write<{ consent: CampaignConsent }>("POST", "/campaigns/consent", consent).then(
      (r) => r.consent,
    );
  }

  /** Everybody this tenant will not mail again, freshest first. */
  suppressions(): Promise<CampaignSuppression[]> {
    return this.#read<{ suppressions?: CampaignSuppression[] }>("/campaigns/suppressions").then(
      (r) => r.suppressions ?? [],
    );
  }

  /**
   * Suppresses an address for the whole tenant.
   *
   * There is deliberately no method to undo it, here or on the server: ADR 0044
   * §2 makes suppression absolute, and a client that could lift one is a client
   * a bulk import is eventually driven through.
   */
  suppress(suppression: {
    address: string;
    reason: string;
    sourceRef?: string;
    occurredAt?: string;
  }): Promise<CampaignSuppression> {
    return this.#write<{ suppression: CampaignSuppression }>(
      "POST",
      "/campaigns/suppressions",
      suppression,
    ).then((r) => r.suppression);
  }

  /** The tenant's saved questions, by name. */
  segments(): Promise<CampaignSegment[]> {
    return this.#read<{ segments?: CampaignSegment[] }>("/campaigns/segments").then(
      (r) => r.segments ?? [],
    );
  }

  /** Saves a question under a name a colleague will recognise later. A
   *  duplicate name is a `409`: "the Belgian customers" must name one thing. */
  createSegment(segment: {
    name: string;
    conditions: SegmentConditions;
  }): Promise<CampaignSegment> {
    return this.#write<{ segment: CampaignSegment }>("POST", "/campaigns/segments", segment).then(
      (r) => r.segment,
    );
  }

  /** Renames a saved question, or rewrites it. Conditions are replaced whole
   *  when stated — a segment is one sentence, and merging half of one produces
   *  a question nobody wrote. */
  updateSegment(
    id: string,
    segment: { name?: string; conditions?: SegmentConditions },
  ): Promise<CampaignSegment> {
    return this.#write<{ segment: CampaignSegment }>(
      "PATCH",
      `/campaigns/segments/${encodeURIComponent(id)}`,
      segment,
    ).then((r) => r.segment);
  }

  /** The letters this workspace has written, newest first, without their
   *  bodies. */
  campaigns(): Promise<CampaignSummary[]> {
    return this.#read<{ campaigns?: CampaignSummary[] }>("/campaigns/campaigns").then(
      (r) => r.campaigns ?? [],
    );
  }

  /**
   * The vocabulary a letter can personalise with, read from the server.
   *
   * Deliberately not a constant in this file: a merge field added to the store
   * has to appear in the composer without a web release, and a client-side copy
   * of the list is a client-side copy that goes stale. The server answers names
   * only — the words describing each one are ours, in three languages.
   */
  mergeFields(): Promise<string[]> {
    return this.#read<{ fields?: string[] }>("/campaigns/merge-fields").then((r) => r.fields ?? []);
  }

  /**
   * One letter as one person will receive it.
   *
   * `against` is an address, or `PREVIEW_AS_FALLBACKS`, or omitted for the
   * first person this workspace may actually mail. A `404` covers all three of
   * "no such letter", "not yours" and "not somebody you may mail" — the server
   * will not say which, and neither does this method.
   */
  preview(id: string, against?: string): Promise<CampaignPreview> {
    // The words of the unsubscribe footer travel with the request. The server
    // renders the letter and holds no translations of its own, so a preview
    // that did not send them would show an English footer to a Dutch reader —
    // and the footer is the one control a recipient looks for when they have
    // decided they want the mail to stop. The URL is the server's; only the
    // words are ours to give.
    const params = new URLSearchParams({ unsubscribeText: strings.campaignUnsubscribeLinkText });
    if (against !== undefined && against !== "") params.set("as", against);
    return this.#read<{ preview: CampaignPreview }>(
      `/campaigns/campaigns/${encodeURIComponent(id)}/preview?${params.toString()}`,
    ).then((r) => r.preview);
  }

  /**
   * Writes a copy of the letter into the caller's own Drafts.
   *
   * **This sends nothing.** It is the same path every server-composed message
   * in the product takes: the draft lands where the person who asked can read
   * it in their own mail client, change it, and send it themselves. There is no
   * recipient parameter, and that is the point — a field naming who a rendered
   * campaign goes to is the first half of a sending API.
   */
  testDraft(
    id: string,
    against?: string,
  ): Promise<{ draft: CampaignTestDraft; against: PreviewAgainst }> {
    // The seed test renders the same letter a recipient would receive, so it
    // needs the same footer words the preview does — see `preview` above. A
    // draft written without them would be a rehearsal of a different message,
    // in the one place a recipient looks when they want the mail to stop.
    const params = new URLSearchParams({ unsubscribeText: strings.campaignUnsubscribeLinkText });
    if (against !== undefined && against !== "") params.set("as", against);
    return this.#write<{ draft: CampaignTestDraft; against: PreviewAgainst }>(
      "POST",
      `/campaigns/campaigns/${encodeURIComponent(id)}/test?${params.toString()}`,
      {},
    );
  }

  /** Forgets a saved question. Never the evidence: consent records and
   *  suppressions are separate and are untouched. */
  async deleteSegment(id: string): Promise<void> {
    await this.#json<unknown>(
      await this.#send(`/campaigns/segments/${encodeURIComponent(id)}`, { method: "DELETE" }),
    );
  }

  async #read<T>(path: string): Promise<T> {
    return this.#json<T>(await this.#send(path, { method: "GET" }));
  }

  async #write<T>(method: string, path: string, body: unknown): Promise<T> {
    return this.#json<T>(
      await this.#send(path, {
        method,
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      }),
    );
  }

  async #send(path: string, init: RequestInit): Promise<Response> {
    try {
      return await this.#fetch(`${API_BASE}/api${path}`, init);
    } catch {
      // A dropped connection is not a status code; give it one the UI can treat
      // like any other failure rather than an unhandled rejection.
      throw new CampaignsError(0, null);
    }
  }

  async #json<T>(res: Response): Promise<T> {
    if (!res.ok) throw new CampaignsError(res.status, await problemDetail(res));
    return (await res.json()) as T;
  }
}

/** The campaigns client bound to the current session. Memoized per auth
 *  context, so a re-render never re-creates it and effects keyed on it do not
 *  loop. */
export function useCampaignsApi(): CampaignsApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new CampaignsApi(authorizedFetch), [authorizedFetch]);
}
