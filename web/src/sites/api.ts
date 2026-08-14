// The client for the authenticated `/sites/*` HTTP surface (alo Sites,
// ADR 0036, wave S1) — the edit half of the two-service boundary in
// `docs/design/sites.md`.
//
// Its own small client rather than more methods on `JmapClient`, for the same
// reason billing's is: a plain REST surface with none of JMAP's envelope, and
// it changes for different reasons than mail does. It uses the same
// authenticated fetch (bearer + refresh handled by the auth layer), so there
// is one session, not two.
//
// It holds NO validation. Subdomain syntax, reserved words, slug rules and
// the home-page invariant are all ruled on by the store; a second, weaker
// copy of those rules here is how two doors end up disagreeing. The form's
// job is to send what was typed and show what came back. Methods are added
// with their consumers — the section, theme, and publish calls land with the
// screens that make them (S1.12+), never speculatively.
import { useMemo } from "react";

import { useAuth } from "../auth";
import { getLocale } from "../i18n";
import { API_BASE } from "../platform/runtime";
import { readPalette } from "./palette";
import type { PaletteTile } from "./palette";
import type { Section, SectionsEnvelope } from "./sections";
import type {
  DomainCatalog,
  DomainQuote,
  DomainSearchResult,
  PageDraft,
  PostDraft,
  PostUpdate,
  Site,
  SiteDomain,
  SiteDomainPurchase,
  SiteDomainPurchaseDraft,
  SiteAnalyticsReport,
  SiteAttributionReport,
  SiteAvailabilitySource,
  SiteBooking,
  SiteBookingDraft,
  SiteCrmBoard,
  SiteCrmColumn,
  SiteLeadHandoff,
  SiteLeadLink,
  SiteDetail,
  SiteDraft,
  SiteHeatmapReport,
  GeneratedSiteDraft,
  SiteEditEnvelope,
  ProposedSiteEdit,
  SiteCopyRequest,
  SitePage,
  SitePageDetail,
  SitePageProtection,
  LocalizedSitePageDetail,
  SiteTranslationReadiness,
  SiteTranslationEnvelope,
  SitePost,
  SitePublishComparison,
  SitePublishPage,
  SitePublishRestore,
  SitePublishSchedule,
  SitePublishVersion,
  SiteSubmission,
  SiteTemplate,
  TemplateSiteDraft,
  SiteCatalog,
  SiteCatalogCategory,
  SiteCatalogDetail,
  SiteCatalogDraft,
  SiteCatalogItem,
  SiteCatalogItemDraft,
  SiteOrder,
  SiteOrderStatus,
  SiteCollection,
  SiteCollectionDraft,
  SiteCollectionPreview,
  SiteCollectionSource,
  SiteCollaborator,
  SiteCollaboratorInvite,
  SiteInvitation,
  SitesConfig,
  SubdomainCheck,
  ThemeEnvelope,
  ThemePreset,
} from "./types";

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/**
 * A failed sites request. `detail` is the server's own sentence when it sent
 * one — the store authors those messages to name the rule that was broken
 * (never another tenant's data), so they are safe to put in front of a user.
 * `status` lets a caller tell "that breaks a rule" (422) from "that record is
 * gone" (404) without parsing prose.
 */
export class SitesError extends Error {
  readonly status: number;
  readonly detail: string | null;
  readonly reason: string | null;

  constructor(status: number, detail: string | null, reason: string | null = null) {
    super(detail ?? `sites request failed (${status})`);
    this.name = "SitesError";
    this.status = status;
    this.detail = detail;
    this.reason = reason;
  }
}

/** What to show a user about a failed request: the server's own sentence when
 *  it sent one, and `fallback` otherwise (a dropped connection, or a failure
 *  whose reason is not the user's business). */
export function sitesMessage(error: unknown, fallback: string): string {
  return error instanceof SitesError && error.detail !== null ? error.detail : fallback;
}

/** The protection answer as the server shapes it: `{"protected": false}`
 *  carries nothing else, so every field but the flag is optional on the wire. */
interface RawPageProtection {
  protected?: unknown;
  pageId?: unknown;
  createdAt?: unknown;
  updatedAt?: unknown;
}

/** Normalizes one protection answer into the shape screens branch on, so a
 *  public page and a protected one differ by field VALUES rather than by
 *  which fields exist. */
function pageProtection(raw: RawPageProtection): SitePageProtection {
  const text = (value: unknown) => (typeof value === "string" ? value : null);
  return {
    protected: raw.protected === true,
    pageId: text(raw.pageId),
    createdAt: text(raw.createdAt),
    updatedAt: text(raw.updatedAt),
  };
}

/** The tenant's websites, their pages, and each page's section stack.
 *  One instance per auth context. */
export class SitesApi {
  readonly #fetch: AuthorizedFetch;

  constructor(authorizedFetch: AuthorizedFetch) {
    this.#fetch = authorizedFetch;
  }

  /** The tenant's sites. */
  sites(): Promise<Site[]> {
    return this.#read<{ sites?: Site[] }>("/sites").then((r) => r.sites ?? []);
  }

  /** Creates a site, claiming the subdomain in the global namespace; answers
   *  the STORED record. A claim that collides is a `422` saying taken/free
   *  only, never who holds it. */
  createSite(draft: SiteDraft): Promise<Site> {
    return this.#write<Site>("POST", "/sites", draft);
  }

  /** Turns a plain-language business description into one complete private
   *  site draft. Publishing remains a separate, explicit owner action. */
  generateSite(description: string): Promise<GeneratedSiteDraft> {
    return this.#write<GeneratedSiteDraft>("POST", "/sites/generate", { description });
  }

  /** The templates this build ships, in gallery order. The catalog is the
   *  same for every workspace — it carries no tenant data — but the route
   *  still authenticates, so a failure is treated like any other. */
  siteTemplates(): Promise<SiteTemplate[]> {
    return this.#read<{ templates?: SiteTemplate[] }>("/sites/templates").then(
      (r) => r.templates ?? [],
    );
  }

  /** One template page rendered by the server as a complete, self-contained
   *  HTML document — what the gallery shows. Answers text, not JSON; the
   *  caller puts it in a sandboxed iframe via `srcdoc`. Without `page` the
   *  template's home page is rendered. */
  async templatePreview(templateId: string, page?: string): Promise<string> {
    const path = `/sites/templates/${encodeURIComponent(templateId)}/preview`;
    const res = await this.#send(
      page === undefined ? path : `${path}?page=${encodeURIComponent(page)}`,
      { method: "GET" },
    );
    await SitesApi.#rejectFailed(res);
    return res.text();
  }

  /** Creates a draft site holding the template's pages, theme and contact
   *  form, in one server transaction. Nothing is published: making a template
   *  live stays an explicit act, exactly as for a generated site. */
  createSiteFromTemplate(templateId: string, draft: SiteDraft): Promise<TemplateSiteDraft> {
    return this.#write<TemplateSiteDraft>(
      "POST",
      `/sites/templates/${encodeURIComponent(templateId)}`,
      draft,
    );
  }

  /** One site with its current publish (`null` while unpublished). */
  site(id: string): Promise<SiteDetail> {
    return this.#read<SiteDetail>(`/sites/${encodeURIComponent(id)}`);
  }

  /** The restricted collaborators of one site. This route never exposes the
   * workspace user directory. */
  collaborators(siteId: string): Promise<SiteCollaborator[]> {
    return this.#read<{ collaborators?: SiteCollaborator[] }>(
      `/sites/${encodeURIComponent(siteId)}/collaborators`,
    ).then((response) => response.collaborators ?? []);
  }

  inviteCollaborator(siteId: string, email: string): Promise<SiteCollaboratorInvite> {
    return this.#write<SiteCollaboratorInvite>(
      "POST",
      `/sites/${encodeURIComponent(siteId)}/collaborators`,
      { email },
    );
  }

  async revokeCollaborator(siteId: string, userId: string): Promise<void> {
    await this.#write<{ status?: string }>(
      "DELETE",
      `/sites/${encodeURIComponent(siteId)}/collaborators/${encodeURIComponent(userId)}`,
      undefined,
    );
  }

  /** Replaces the visible language set; normalization and validation live on the server. */
  async setSiteLocales(
    siteId: string,
    defaultLocale: string,
    enabledLocales: string[],
  ): Promise<void> {
    await this.#write<{ status?: string }>("PUT", `/sites/${encodeURIComponent(siteId)}`, {
      defaultLocale,
      enabledLocales,
    });
  }

  /** Exact per-language page coverage used beside the Publish action. */
  translationReadiness(siteId: string): Promise<SiteTranslationReadiness> {
    return this.#read<SiteTranslationReadiness>(
      `/sites/${encodeURIComponent(siteId)}/translation-readiness`,
    );
  }

  proposeSiteTranslation(
    siteId: string,
    sourceLocale: string,
    targetLocale: string,
  ): Promise<SiteTranslationEnvelope> {
    return this.#write<{ proposal: SiteTranslationEnvelope }>(
      "POST",
      `/sites/${encodeURIComponent(siteId)}/translation-proposals`,
      { sourceLocale, targetLocale },
    ).then((response) => response.proposal);
  }

  async applySiteTranslation(
    siteId: string,
    proposal: SiteTranslationEnvelope,
  ): Promise<void> {
    await this.#write(
      "PUT",
      `/sites/${encodeURIComponent(siteId)}/translation-proposals`,
      { proposal },
    );
  }

  /** The live taken/free answer for a well-formed label; a syntactically
   *  invalid or reserved one is a `422` naming the rule. */
  checkSubdomain(subdomain: string): Promise<SubdomainCheck> {
    return this.#read<SubdomainCheck>(
      `/sites/subdomain-check?subdomain=${encodeURIComponent(subdomain)}`,
    );
  }

  /** The deployment-wide sites config (the domain published sites serve
   *  under). Callers treat a failure as "domain unknown" and degrade — the
   *  copy that needs it simply stays off. */
  config(): Promise<SitesConfig> {
    return this.#read<SitesConfig>("/sites/config");
  }

  /** Freezes the current pages + theme into an immutable published set and
   *  puts the site live. A site with no pages or no home page is a `422`
   *  naming the precondition. */
  async publishSite(siteId: string): Promise<void> {
    await this.#write<{ publishId?: string }>(
      "POST",
      `/sites/${encodeURIComponent(siteId)}/publish`,
      {},
    );
  }

  /** Takes the site off the air (history is retained on the server).
   *  Idempotent. */
  async unpublishSite(siteId: string): Promise<void> {
    await this.#write<{ status?: string }>(
      "POST",
      `/sites/${encodeURIComponent(siteId)}/unpublish`,
      {},
    );
  }

  /** Every version this site has ever published, newest first, with the id of
   *  the one on the internet (`null` while the site is offline). */
  publishes(siteId: string): Promise<{
    publishes: SitePublishVersion[];
    current: string | null;
  }> {
    return this.#read<{ publishes?: SitePublishVersion[]; current?: string | null }>(
      `/sites/${encodeURIComponent(siteId)}/publishes`,
    ).then((r) => ({ publishes: r.publishes ?? [], current: r.current ?? null }));
  }

  /** The pages one version froze — one entry per page and language. */
  publishPages(siteId: string, publishId: string): Promise<SitePublishPage[]> {
    return this.#read<{ pages?: SitePublishPage[] }>(
      `${this.#publishPath(siteId, publishId)}/pages`,
    ).then((r) => r.pages ?? []);
  }

  /** What a visitor would see differently between two versions (metadata). */
  comparePublishes(
    siteId: string,
    from: string,
    to: string,
  ): Promise<SitePublishComparison> {
    return this.#read<SitePublishComparison>(
      `/sites/${encodeURIComponent(siteId)}/publishes/compare?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`,
    );
  }

  /** One frozen page rendered by the server as a complete, self-contained
   *  HTML document — the history preview. Answers text, not JSON; the caller
   *  puts it in a sandboxed iframe via `srcdoc`. */
  async publishPreview(
    siteId: string,
    publishId: string,
    pageId: string,
    locale?: string,
  ): Promise<string> {
    const path = `${this.#publishPath(siteId, publishId)}/pages/${encodeURIComponent(pageId)}/preview`;
    const res = await this.#send(
      locale === undefined ? path : `${path}?locale=${encodeURIComponent(locale)}`,
      { method: "GET" },
    );
    await SitesApi.#rejectFailed(res);
    return res.text();
  }

  /** Puts an earlier version back on the internet as a NEW version holding a
   *  copy of it. History is never rewritten, and the draft is never touched. */
  restorePublish(siteId: string, publishId: string): Promise<SitePublishRestore> {
    return this.#write<SitePublishRestore>(
      "POST",
      `${this.#publishPath(siteId, publishId)}/restore`,
      {},
    );
  }

  /** When this website is going to publish itself (`null` when nothing is
   *  waiting), and what previous intentions did. */
  publishSchedule(siteId: string): Promise<{
    schedule: SitePublishSchedule | null;
    history: SitePublishSchedule[];
  }> {
    return this.#read<{
      schedule?: SitePublishSchedule | null;
      history?: SitePublishSchedule[];
    }>(`${this.#schedulePath(siteId)}`).then((r) => ({
      schedule: r.schedule ?? null,
      history: r.history ?? [],
    }));
  }

  /** Chooses the moment this website goes live, or moves the moment already
   *  chosen (the schedule keeps its id). `publishAt` is an RFC 3339 instant —
   *  the caller sends a real moment, never a wall-clock string. */
  scheduleSitePublish(siteId: string, publishAt: string): Promise<SitePublishSchedule> {
    return this.#write<SitePublishSchedule>("POST", this.#schedulePath(siteId), {
      publishAt,
    });
  }

  /** Calls off a scheduled publish. The site is not touched; the row survives
   *  as `cancelled` so the screen can say what happened to it. */
  cancelSitePublish(siteId: string, scheduleId: string): Promise<SitePublishSchedule> {
    return this.#write<SitePublishSchedule>(
      "DELETE",
      `${this.#schedulePath(siteId)}/${encodeURIComponent(scheduleId)}`,
      {},
    );
  }

  /** Every visitor message sent through this site's contact forms, newest first. */
  submissions(siteId: string): Promise<SiteSubmission[]> {
    return this.#read<{ submissions?: SiteSubmission[] }>(
      `/sites/${encodeURIComponent(siteId)}/submissions`,
    ).then((r) => r.submissions ?? []);
  }

  /** The same visitor inbox as a spreadsheet-safe CSV rendered by the server. */
  async submissionsCsv(siteId: string): Promise<string> {
    const response = await this.#send(
      `/sites/${encodeURIComponent(siteId)}/submissions.csv`,
      { method: "GET" },
    );
    await SitesApi.#rejectFailed(response);
    return response.text();
  }

  /** Moves one message between the open and handled queues. */
  async setSubmissionHandled(
    siteId: string,
    formId: string,
    submissionId: string,
    handled: boolean,
  ): Promise<void> {
    await this.#write<{ status?: string }>(
      "PUT",
      `/sites/${encodeURIComponent(siteId)}/forms/${encodeURIComponent(formId)}/submissions/${encodeURIComponent(submissionId)}`,
      { handled },
    );
  }

  /** Anonymous daily traffic totals and actionable rankings for one site. */
  analytics(siteId: string, days: number): Promise<SiteAnalyticsReport> {
    return this.#read<SiteAnalyticsReport>(
      `/sites/${encodeURIComponent(siteId)}/analytics?days=${encodeURIComponent(String(days))}`,
    );
  }

  /** One page's aggregate click-and-depth heatmap, plus the menu of pages that
   *  have any. Called without `path` it answers the menu alone (`page` is
   *  `null`), so the screen can offer the menu before anything is chosen. */
  heatmap(
    siteId: string,
    days: number,
    path?: string,
  ): Promise<SiteHeatmapReport> {
    const query = new URLSearchParams({ days: String(days) });
    if (path !== undefined) query.set("path", path);
    return this.#read<SiteHeatmapReport>(
      `/sites/${encodeURIComponent(siteId)}/heatmap?${query.toString()}`,
    ).then((report) => ({ ...report, paths: report.paths ?? [] }));
  }

  /** This site's funnel from page views to invoices over `days`, per
   *  conversion point and for the site. A reader who may not see the business
   *  behind the website (a site editor, or someone alo CRM is switched off
   *  for) gets a `403` carrying the server's own sentence. */
  attribution(siteId: string, days: number): Promise<SiteAttributionReport> {
    return this.#read<SiteAttributionReport>(
      `/sites/${encodeURIComponent(siteId)}/attribution?days=${encodeURIComponent(String(days))}`,
    ).then((report) => ({ ...report, sources: report.sources ?? [] }));
  }

  /** Every enquiry of this site that a person handed to the sales board,
   *  newest first, each with its opportunity as it stands now. */
  siteLeads(siteId: string): Promise<SiteLeadLink[]> {
    return this.#read<{ leads?: SiteLeadLink[] }>(
      `/sites/${encodeURIComponent(siteId)}/leads`,
    ).then((response) => response.leads ?? []);
  }

  /** Hands one enquiry to the sales board: raises the opportunity in the
   *  chosen board and column and links it to the submission, in one server
   *  transaction. The enquirer's name and address travel from the stored
   *  submission — they are not in this body and are never re-typed. */
  createSiteLead(
    siteId: string,
    submissionId: string,
    handoff: SiteLeadHandoff,
  ): Promise<SiteLeadLink> {
    return this.#write<SiteLeadLink>(
      "POST",
      `/sites/${encodeURIComponent(siteId)}/submissions/${encodeURIComponent(submissionId)}/lead`,
      handoff,
    );
  }

  /** Unclaims an opportunity for this website. The opportunity itself belongs
   *  to CRM and is left exactly as it is. */
  async deleteSiteLead(siteId: string, linkId: string): Promise<void> {
    // Answers `204` with no body, so this one cannot go through `#write`,
    // which reads the answer as JSON.
    const response = await this.#send(
      `/sites/${encodeURIComponent(siteId)}/leads/${encodeURIComponent(linkId)}`,
      { method: "DELETE" },
    );
    await SitesApi.#rejectFailed(response);
  }

  /** The tenant's sales boards, read through CRM's own route — Sites keeps no
   *  copy of a board and invents no second permission vocabulary. The `lang`
   *  parameter is CRM's: a tenant that has never opened CRM has its first
   *  board seeded in the language of the person looking at it. */
  crmBoards(): Promise<SiteCrmBoard[]> {
    return this.#read<{ pipelines?: SiteCrmBoard[] }>(
      `/crm/pipelines?lang=${encodeURIComponent(getLocale())}`,
    ).then((response) => (response.pipelines ?? []).filter((board) => !board.archived));
  }

  /** The columns of one sales board, left to right. */
  crmColumns(boardId: string): Promise<SiteCrmColumn[]> {
    return this.#read<{ stages?: SiteCrmColumn[] }>(
      `/crm/pipelines/${encodeURIComponent(boardId)}/stages`,
    ).then((response) => (response.stages ?? []).filter((column) => !column.archived));
  }

  /** The shipped theme presets, in picker order (the first is the default). */
  themePresets(): Promise<ThemePreset[]> {
    return this.#read<{ presets?: ThemePreset[] }>("/sites/theme-presets").then(
      (r) => r.presets ?? [],
    );
  }

  /** Stores the site's theme (the full envelope, through the server's theme
   *  gate — an unknown preset or malformed blob ref is a 422 naming the rule). */
  async setTheme(siteId: string, theme: ThemeEnvelope): Promise<void> {
    await this.#write<{ status?: string }>(
      "PUT",
      `/sites/${encodeURIComponent(siteId)}/theme`,
      theme,
    );
  }

  /** The site's pages in navigation order. */
  pages(siteId: string): Promise<SitePage[]> {
    return this.#read<{ pages?: SitePage[] }>(
      `/sites/${encodeURIComponent(siteId)}/pages`,
    ).then((r) => r.pages ?? []);
  }

  collections(siteId: string): Promise<SiteCollection[]> {
    return this.#read<{ collections?: SiteCollection[] }>(
      `/sites/${encodeURIComponent(siteId)}/collections`,
    ).then((response) => response.collections ?? []);
  }

  createCollection(siteId: string, draft: SiteCollectionDraft): Promise<SiteCollection> {
    return this.#write<SiteCollection>(
      "POST",
      `/sites/${encodeURIComponent(siteId)}/collections`,
      draft,
    );
  }

  updateCollection(
    siteId: string,
    collectionId: string,
    draft: SiteCollectionDraft,
  ): Promise<SiteCollection> {
    return this.#write<SiteCollection>(
      "PUT",
      `/sites/${encodeURIComponent(siteId)}/collections/${encodeURIComponent(collectionId)}`,
      draft,
    );
  }

  async disconnectCollection(siteId: string, collectionId: string): Promise<void> {
    await this.#write<{ status?: string }>(
      "DELETE",
      `/sites/${encodeURIComponent(siteId)}/collections/${encodeURIComponent(collectionId)}`,
      undefined,
    );
  }

  /** The site's catalogs, without their items: a site with four catalogs must
   *  not pay for four hundred items to draw a list. */
  catalogs(siteId: string): Promise<SiteCatalog[]> {
    return this.#read<{ catalogs?: SiteCatalog[] }>(
      `/sites/${encodeURIComponent(siteId)}/catalogs`,
    ).then((response) => response.catalogs ?? []);
  }

  /** One catalog with its categories and every item, hidden ones included —
   *  the editor's whole screen in one read. */
  catalog(siteId: string, catalogId: string): Promise<SiteCatalogDetail> {
    return this.#read<SiteCatalogDetail>(this.#catalogPath(siteId, catalogId));
  }

  createCatalog(siteId: string, draft: SiteCatalogDraft): Promise<SiteCatalog> {
    return this.#write<SiteCatalog>(
      "POST",
      `/sites/${encodeURIComponent(siteId)}/catalogs`,
      draft,
    );
  }

  updateCatalog(
    siteId: string,
    catalogId: string,
    draft: SiteCatalogDraft,
  ): Promise<SiteCatalog> {
    return this.#write<SiteCatalog>("PUT", this.#catalogPath(siteId, catalogId), draft);
  }

  deleteCatalog(siteId: string, catalogId: string): Promise<void> {
    return this.#discard("DELETE", this.#catalogPath(siteId, catalogId));
  }

  createCatalogCategory(
    siteId: string,
    catalogId: string,
    name: string,
  ): Promise<SiteCatalogCategory> {
    return this.#write<SiteCatalogCategory>(
      "POST",
      `${this.#catalogPath(siteId, catalogId)}/categories`,
      { name },
    );
  }

  updateCatalogCategory(
    siteId: string,
    catalogId: string,
    categoryId: string,
    name: string,
  ): Promise<SiteCatalogCategory> {
    return this.#write<SiteCatalogCategory>(
      "PUT",
      `${this.#catalogPath(siteId, catalogId)}/categories/${encodeURIComponent(categoryId)}`,
      { name },
    );
  }

  /** Removes the grouping. The items it grouped stay, uncategorised. */
  deleteCatalogCategory(
    siteId: string,
    catalogId: string,
    categoryId: string,
  ): Promise<void> {
    return this.#discard(
      "DELETE",
      `${this.#catalogPath(siteId, catalogId)}/categories/${encodeURIComponent(categoryId)}`,
    );
  }

  createCatalogItem(
    siteId: string,
    catalogId: string,
    draft: SiteCatalogItemDraft,
  ): Promise<SiteCatalogItem> {
    return this.#write<SiteCatalogItem>(
      "POST",
      `${this.#catalogPath(siteId, catalogId)}/items`,
      draft,
    );
  }

  /** Replaces the item whole — every field the form shows is sent every time,
   *  so no combination of screens can leave a published card half-written. */
  updateCatalogItem(
    siteId: string,
    catalogId: string,
    itemId: string,
    draft: SiteCatalogItemDraft,
  ): Promise<SiteCatalogItem> {
    return this.#write<SiteCatalogItem>(
      "PUT",
      `${this.#catalogPath(siteId, catalogId)}/items/${encodeURIComponent(itemId)}`,
      draft,
    );
  }

  deleteCatalogItem(siteId: string, catalogId: string, itemId: string): Promise<void> {
    return this.#discard(
      "DELETE",
      `${this.#catalogPath(siteId, catalogId)}/items/${encodeURIComponent(itemId)}`,
    );
  }

  /** The Agenda calendars a booking service of this site may be attached to,
   *  read-only shares included and marked as such (S2.13a). An empty list is
   *  the honest dependency state: nothing here can be booked until the account
   *  has a calendar to book into. */
  bookingSources(siteId: string): Promise<SiteAvailabilitySource[]> {
    return this.#read<{ sources?: SiteAvailabilitySource[] }>(
      `/sites/${encodeURIComponent(siteId)}/booking-sources`,
    ).then((response) => response.sources ?? []);
  }

  /** Every bookable service of the site, in creation order, each with its
   *  calendar resolved (`calendar: null` when it can no longer be reached). */
  bookings(siteId: string): Promise<SiteBooking[]> {
    return this.#read<{ bookings?: SiteBooking[] }>(
      `/sites/${encodeURIComponent(siteId)}/bookings`,
    ).then((response) => response.bookings ?? []);
  }

  createBooking(siteId: string, draft: SiteBookingDraft): Promise<SiteBooking> {
    return this.#write<SiteBooking>(
      "POST",
      `/sites/${encodeURIComponent(siteId)}/bookings`,
      draft,
    );
  }

  /** Replaces the service whole: the form sends every field it shows every
   *  time, so no sequence of screens can leave a published page offering a week
   *  its owner never agreed to. */
  updateBooking(
    siteId: string,
    bookingId: string,
    draft: SiteBookingDraft,
  ): Promise<SiteBooking> {
    return this.#write<SiteBooking>("PUT", this.#bookingPath(siteId, bookingId), draft);
  }

  /** Removes the service. Appointments already in the calendar are
   *  appointments — nothing here cancels one. */
  deleteBooking(siteId: string, bookingId: string): Promise<void> {
    return this.#discard("DELETE", this.#bookingPath(siteId, bookingId));
  }

  #bookingPath(siteId: string, bookingId: string): string {
    return `/sites/${encodeURIComponent(siteId)}/bookings/${encodeURIComponent(bookingId)}`;
  }

  /** Every order this site's published catalogs took, newest first, each with
   *  the lines it asked for. There is no create: the only writer of an order
   *  is the public door, which prices it from the publish. */
  orders(siteId: string): Promise<SiteOrder[]> {
    return this.#read<{ orders?: SiteOrder[] }>(
      `/sites/${encodeURIComponent(siteId)}/orders`,
    ).then((response) => response.orders ?? []);
  }

  /** Moves one order through the workflow, in either direction. Answers the
   *  order as it now stands, so the screen shows the server's row rather than
   *  its own guess at it. */
  setOrderStatus(
    siteId: string,
    orderId: string,
    status: SiteOrderStatus,
  ): Promise<SiteOrder> {
    return this.#write<SiteOrder>("PUT", this.#orderPath(siteId, orderId), { status });
  }

  /** Deletes an order and its lines — spam, a duplicate, or a customer asking
   *  for their data to be removed. */
  deleteOrder(siteId: string, orderId: string): Promise<void> {
    return this.#discard("DELETE", this.#orderPath(siteId, orderId));
  }

  /** The same order inbox as a spreadsheet-safe CSV rendered by the server,
   *  one row per ordered line so the numbers can be summed. */
  async ordersCsv(siteId: string): Promise<string> {
    const response = await this.#send(
      `/sites/${encodeURIComponent(siteId)}/orders.csv`,
      { method: "GET" },
    );
    await SitesApi.#rejectFailed(response);
    return response.text();
  }

  #orderPath(siteId: string, orderId: string): string {
    return `/sites/${encodeURIComponent(siteId)}/orders/${encodeURIComponent(orderId)}`;
  }

  #catalogPath(siteId: string, catalogId: string): string {
    return `/sites/${encodeURIComponent(siteId)}/catalogs/${encodeURIComponent(catalogId)}`;
  }

  collectionPreview(siteId: string, collectionId: string): Promise<SiteCollectionPreview> {
    return this.#read<SiteCollectionPreview>(
      `/sites/${encodeURIComponent(siteId)}/collections/${encodeURIComponent(collectionId)}/preview`,
    );
  }

  /** Every readable personal or Space Base, discovered through Drive's own
   *  access-scoped list route. Sites never receives a second permission
   *  vocabulary and never asks the user to paste an opaque id. */
  async collectionSources(): Promise<SiteCollectionSource[]> {
    const spaces = await this.#read<{
      spaces?: Array<{ id: string; archived: boolean }>;
    }>("/spaces").then((response) => response.spaces ?? []);
    const locations: Array<string | null> = [
      null,
      ...spaces.filter((space) => !space.archived).map((space) => space.id),
    ];
    const baseNodes = (
      await Promise.all(locations.map((space) => this.#baseNodes(space, null)))
    ).flat();
    const sources = await Promise.all(
      baseNodes.map(async (node) => {
        const base = await this.#read<{
          nodeId: string;
          tables?: Array<{
            id: string;
            name: string;
            fields?: Array<{ id: string; name: string; type: string }>;
            records?: unknown[];
          }>;
        }>(`/drive/base/${encodeURIComponent(node.id)}`);
        return {
          nodeId: node.id,
          name: node.name,
          tables: (base.tables ?? []).map((table) => ({
            id: table.id,
            name: table.name,
            recordCount: table.records?.length ?? 0,
            fields: (table.fields ?? []).flatMap((field) =>
              field.type === "text" || field.type === "date" || field.type === "attachment"
                ? [{ id: field.id, name: field.name, type: field.type }]
                : [],
            ),
          })),
        } satisfies SiteCollectionSource;
      }),
    );
    return sources.sort((left, right) => left.name.localeCompare(right.name));
  }

  /** Creates a page at the end of the navigation order, with an empty section
   *  stack; answers the stored page. */
  createPage(siteId: string, draft: PageDraft): Promise<SitePage> {
    return this.#write<SitePage>("POST", `/sites/${encodeURIComponent(siteId)}/pages`, draft);
  }

  /** Proposes a guarded, reviewable operation set and writes nothing. */
  proposePageEdit(
    siteId: string,
    pageId: string,
    instruction: string,
  ): Promise<ProposedSiteEdit> {
    return this.#write<ProposedSiteEdit>(
      "POST",
      `${this.#pagePath(siteId, pageId)}/ai-edits`,
      { instruction },
    );
  }

  /** Proposes one guarded rewrite of one existing section string. All copy
   *  actions share the page proposal door and still require approval. */
  proposePageCopyEdit(
    siteId: string,
    pageId: string,
    copy: SiteCopyRequest,
  ): Promise<ProposedSiteEdit> {
    return this.#write<ProposedSiteEdit>(
      "POST",
      `${this.#pagePath(siteId, pageId)}/ai-edits`,
      { copy },
    );
  }

  /** Applies only the operation set the owner reviewed. The server replays it
   *  against the current page, refusing stale targets. */
  applyPageEdit(
    siteId: string,
    pageId: string,
    proposal: SiteEditEnvelope,
  ): Promise<SectionsEnvelope> {
    return this.#sections(
      this.#write("PUT", `${this.#pagePath(siteId, pageId)}/ai-edits`, { proposal }),
    );
  }

  /** The site's blog posts, newest first. */
  posts(siteId: string): Promise<SitePost[]> {
    return this.#read<{ posts?: SitePost[] }>(
      `/sites/${encodeURIComponent(siteId)}/posts`,
    ).then((r) => r.posts ?? []);
  }

  /** Binds an alo Doc to a new draft post and answers the stored metadata. */
  createPost(siteId: string, draft: PostDraft): Promise<SitePost> {
    return this.#write<SitePost>("POST", `/sites/${encodeURIComponent(siteId)}/posts`, draft);
  }

  /** Replaces the public title, path, excerpt, and optional cover. */
  async updatePost(siteId: string, postId: string, update: PostUpdate): Promise<void> {
    await this.#write<{ status?: string }>(
      "PUT",
      this.#postPath(siteId, postId),
      update,
    );
  }

  /** Makes a draft post public. */
  async publishPost(siteId: string, postId: string): Promise<void> {
    await this.#write<{ status?: string }>(
      "POST",
      `${this.#postPath(siteId, postId)}/publish`,
      {},
    );
  }

  /** Returns a public post to private draft state. */
  async unpublishPost(siteId: string, postId: string): Promise<void> {
    await this.#write<{ status?: string }>(
      "POST",
      `${this.#postPath(siteId, postId)}/unpublish`,
      {},
    );
  }

  /** One page with its sections envelope — the editor's load. */
  page(siteId: string, pageId: string): Promise<SitePageDetail> {
    return this.#read<SitePageDetail>(this.#pagePath(siteId, pageId));
  }

  localizedPage(
    siteId: string,
    pageId: string,
    locale: string,
  ): Promise<LocalizedSitePageDetail> {
    return this.#read<LocalizedSitePageDetail>(this.#localizedPagePath(siteId, pageId, locale));
  }

  setLocalizedPage(
    siteId: string,
    pageId: string,
    locale: string,
    page: Pick<SitePageDetail, "title" | "slug" | "sections" | "seoTitle" | "seoDescription">,
  ): Promise<LocalizedSitePageDetail> {
    return this.#write<LocalizedSitePageDetail>(
      "PUT",
      this.#localizedPagePath(siteId, pageId, locale),
      page,
    );
  }

  /** Updates the visible page name/path while preserving its section stack. */
  async setPageIdentity(siteId: string, pageId: string, title: string, slug: string): Promise<void> {
    await this.#write<{ status?: string }>("PUT", this.#pagePath(siteId, pageId), {
      title,
      slug,
    });
  }

  /** Sets or clears one page's search and sharing copy. Blank strings clear
   *  the overrides through the server's existing normalization gate. */
  async setPageSeo(
    siteId: string,
    pageId: string,
    seoTitle: string,
    seoDescription: string,
  ): Promise<void> {
    await this.#write<{ status?: string }>("PUT", this.#pagePath(siteId, pageId), {
      seoTitle,
      seoDescription,
    });
  }

  /** Whether this page asks its visitors for a password, and when that was
   *  last decided. Never the password: a forgotten one is replaced. */
  pagePassword(siteId: string, pageId: string): Promise<SitePageProtection> {
    return this.#read<RawPageProtection>(
      `${this.#pagePath(siteId, pageId)}/password`,
    ).then(pageProtection);
  }

  /** Every page of this site that asks for a password — one read, so a page
   *  list can mark them without a request per row. */
  protectedPages(siteId: string): Promise<SitePageProtection[]> {
    return this.#read<{ pages?: RawPageProtection[] }>(
      `/sites/${encodeURIComponent(siteId)}/passwords`,
    ).then((r) => (r.pages ?? []).map(pageProtection));
  }

  /** Protects the page, or replaces the password it already carries — from
   *  the owner's side those are one decision. Effective on the next visitor
   *  request, with no republish, and it closes every session opened with the
   *  previous password. Password rules belong to the server; a refusal comes
   *  back as a `422` naming the rule. */
  setPagePassword(
    siteId: string,
    pageId: string,
    password: string,
  ): Promise<SitePageProtection> {
    return this.#write<RawPageProtection>(
      "PUT",
      `${this.#pagePath(siteId, pageId)}/password`,
      { password },
    ).then(pageProtection);
  }

  /** Puts the page back in front of everyone. Idempotent. */
  removePagePassword(siteId: string, pageId: string): Promise<SitePageProtection> {
    return this.#write<RawPageProtection>(
      "DELETE",
      `${this.#pagePath(siteId, pageId)}/password`,
      {},
    ).then(pageProtection);
  }

  // ---- domains -------------------------------------------------------------

  /** The domains this website answers to besides its alo address, with the
   *  TXT proof each pending one is waiting for. */
  siteDomains(siteId: string): Promise<SiteDomain[]> {
    return this.#read<{ domains?: SiteDomain[] }>(
      `${this.#domainsPath(siteId)}`,
    ).then((r) => r.domains ?? []);
  }

  /** Claims a domain the tenant already owns elsewhere; answers the pending
   *  claim together with the exact record to publish. Syntax and ownership
   *  rules are the server's — a refusal comes back as a `422` naming one. */
  addSiteDomain(siteId: string, domain: string): Promise<SiteDomain> {
    return this.#write<SiteDomain>("POST", this.#domainsPath(siteId), { domain });
  }

  /** Looks for the TXT proof now. A record that is not there yet is a normal
   *  answer with the claim still `pending`, not a failure — DNS takes its
   *  time, and a screen that called that an error would be wrong every time. */
  verifySiteDomain(siteId: string, domain: string): Promise<SiteDomain> {
    return this.#write<SiteDomain>(
      "POST",
      `${this.#domainsPath(siteId)}/${encodeURIComponent(domain)}/verify`,
      {},
    );
  }

  /** Releases a claim. The domain itself is untouched: this is alo forgetting
   *  it, not a registry losing it. */
  async removeSiteDomain(siteId: string, domain: string): Promise<void> {
    await this.#write<{ status?: string }>(
      "DELETE",
      `${this.#domainsPath(siteId)}/${encodeURIComponent(domain)}`,
      undefined,
    );
  }

  /** The endings this deployment sells and what they cost. A deployment that
   *  sells none answers `503 {"reason":"unconfigured"}`, which the screen
   *  branches on to offer connecting a domain instead. */
  domainCatalog(): Promise<DomainCatalog> {
    return this.#read<DomainCatalog>("/sites/domain-catalog");
  }

  /** Prices one typed name against the given endings. `q` is sent as typed —
   *  the server understands `acme`, `Acme.com` and a pasted URL alike, and a
   *  second, weaker parser here is how two doors end up disagreeing. */
  searchDomains(query: string, tlds: string[]): Promise<DomainSearchResult> {
    const params = new URLSearchParams({ q: query });
    if (tlds.length > 0) params.set("tlds", tlds.join(","));
    return this.#read<DomainSearchResult>(`/sites/domain-search?${params.toString()}`);
  }

  /** This website's domain purchases, newest first. Carries no registrant. */
  domainPurchases(siteId: string): Promise<SiteDomainPurchase[]> {
    return this.#read<{ purchases?: SiteDomainPurchase[] }>(
      this.#purchasesPath(siteId),
    ).then((r) => r.purchases ?? []);
  }

  /** Starts a purchase at the price the seller states in this very request.
   *  Nothing is charged and nothing is registered: the answer is a `quoted`
   *  purchase, and approving it is a second, deliberate act. */
  startDomainPurchase(
    siteId: string,
    draft: SiteDomainPurchaseDraft,
  ): Promise<SiteDomainPurchase> {
    return this.#write<SiteDomainPurchase>("POST", this.#purchasesPath(siteId), draft);
  }

  /** Records that this person agreed to **these exact numbers**. The body is
   *  the quote the screen had up, all six fields of it; the server refuses an
   *  approval whose quote has drifted, so a price that moved between the
   *  screen and the charge stops here instead of being re-quoted silently. */
  approveDomainPurchase(
    siteId: string,
    purchaseId: string,
    agreed: DomainQuote,
  ): Promise<SiteDomainPurchase> {
    return this.#write<SiteDomainPurchase>(
      "POST",
      `${this.#purchasesPath(siteId)}/${encodeURIComponent(purchaseId)}/approve`,
      { agreed },
    );
  }

  /** Calls a purchase off. Only before money moved — after that it is a refund
   *  conversation, and the server says so in those words. */
  cancelDomainPurchase(siteId: string, purchaseId: string): Promise<SiteDomainPurchase> {
    return this.#write<SiteDomainPurchase>(
      "POST",
      `${this.#purchasesPath(siteId)}/${encodeURIComponent(purchaseId)}/cancel`,
      {},
    );
  }

  /** The draft page rendered by the server as one complete, self-contained
   *  HTML document — the editor's preview. Answers text, not JSON; the
   *  caller puts it in a sandboxed iframe via `srcdoc`. */
  async pagePreview(siteId: string, pageId: string, locale?: string): Promise<string> {
    const path = locale === undefined
      ? `${this.#pagePath(siteId, pageId)}/preview`
      : `${this.#localizedPagePath(siteId, pageId, locale)}/preview`;
    const res = await this.#send(path, { method: "GET" });
    await SitesApi.#rejectFailed(res);
    return res.text();
  }

  /** The section palette for this page: one tile per section type, seeded
   *  from the tenant's own website (ADR 0042 §4). Read-only — dropping a tile
   *  goes through [`addSection`](SitesApi#addSection) like every other add. */
  async pagePalette(siteId: string, pageId: string): Promise<PaletteTile[]> {
    return readPalette(
      await this.#read<unknown>(`${this.#pagePath(siteId, pageId)}/palette`),
    );
  }

  /** One palette tile rendered as a complete, self-contained HTML document in
   *  the site's own theme — the picture on the tile. Answers text, not JSON;
   *  the caller puts it in a sandboxed iframe via `srcdoc`. */
  async palettePreview(
    siteId: string,
    pageId: string,
    kind: string,
  ): Promise<string> {
    const path = `${this.#pagePath(siteId, pageId)}/palette/${encodeURIComponent(kind)}/preview`;
    const res = await this.#send(path, { method: "GET" });
    await SitesApi.#rejectFailed(res);
    return res.text();
  }

  /** One of the tenant's own image blobs, as bytes — what the framing
   *  control draws its crop rectangle over. The rendered preview inlines its
   *  images as `data:` URIs, which is right for a document and wrong for a
   *  control that needs the source at its own aspect ratio. */
  async siteImage(siteId: string, blobId: string): Promise<Blob> {
    const path = `/sites/${encodeURIComponent(siteId)}/images/${encodeURIComponent(blobId)}`;
    const res = await this.#send(path, { method: "GET" });
    await SitesApi.#rejectFailed(res);
    return res.blob();
  }

  /** Inserts a section at `index` (appends when absent); answers the stored
   *  envelope, canonical as the schema gate wrote it. */
  addSection(
    siteId: string,
    pageId: string,
    section: Section,
    index?: number,
  ): Promise<SectionsEnvelope> {
    return this.#sections(
      this.#write("POST", `${this.#pagePath(siteId, pageId)}/sections`, { section, index }),
    );
  }

  /** Replaces the section at `index`; answers the stored envelope. */
  updateSection(
    siteId: string,
    pageId: string,
    index: number,
    section: Section,
  ): Promise<SectionsEnvelope> {
    return this.#sections(
      this.#write("PUT", `${this.#pagePath(siteId, pageId)}/sections/${index}`, { section }),
    );
  }

  /** Moves the section at `index` to position `to`; answers the stored
   *  envelope. */
  moveSection(
    siteId: string,
    pageId: string,
    index: number,
    to: number,
  ): Promise<SectionsEnvelope> {
    return this.#sections(
      this.#write("POST", `${this.#pagePath(siteId, pageId)}/sections/${index}/move`, { to }),
    );
  }

  /** Removes the section at `index`; answers the stored envelope. */
  removeSection(siteId: string, pageId: string, index: number): Promise<SectionsEnvelope> {
    return this.#sections(
      this.#write("DELETE", `${this.#pagePath(siteId, pageId)}/sections/${index}`, undefined),
    );
  }

  #pagePath(siteId: string, pageId: string): string {
    return `/sites/${encodeURIComponent(siteId)}/pages/${encodeURIComponent(pageId)}`;
  }

  #localizedPagePath(siteId: string, pageId: string, locale: string): string {
    return `${this.#pagePath(siteId, pageId)}/locales/${encodeURIComponent(locale)}`;
  }

  #publishPath(siteId: string, publishId: string): string {
    return `/sites/${encodeURIComponent(siteId)}/publishes/${encodeURIComponent(publishId)}`;
  }

  #schedulePath(siteId: string): string {
    return `/sites/${encodeURIComponent(siteId)}/schedule`;
  }

  #domainsPath(siteId: string): string {
    return `/sites/${encodeURIComponent(siteId)}/domains`;
  }

  #purchasesPath(siteId: string): string {
    return `/sites/${encodeURIComponent(siteId)}/domain-purchases`;
  }

  #postPath(siteId: string, postId: string): string {
    return `/sites/${encodeURIComponent(siteId)}/posts/${encodeURIComponent(postId)}`;
  }

  /** Every section op answers `{"sections": <envelope>}` — unwraps it. */
  async #sections(answer: Promise<{ sections: SectionsEnvelope }>): Promise<SectionsEnvelope> {
    return (await answer).sections;
  }

  async #baseNodes(
    space: string | null,
    parent: string | null,
  ): Promise<Array<{ id: string; name: string }>> {
    const query = new URLSearchParams();
    if (space !== null) query.set("space", space);
    if (parent !== null) query.set("parent", parent);
    const nodes = await this.#read<{
      nodes?: Array<{ id: string; kind: string; name: string }>;
    }>(`/drive/list?${query.toString()}`).then((response) => response.nodes ?? []);
    const nested = await Promise.all(
      nodes
        .filter((node) => node.kind === "folder")
        .map((folder) => this.#baseNodes(space, folder.id)),
    );
    return [
      ...nodes
        .filter((node) => node.kind === "base")
        .map(({ id, name }) => ({ id, name })),
      ...nested.flat(),
    ];
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

  /** A write whose success carries no body (`204`). Reading one as JSON would
   *  throw a parse error where the request in fact succeeded. */
  async #discard(method: string, path: string): Promise<void> {
    await SitesApi.#rejectFailed(await this.#send(path, { method }));
  }

  async #send(path: string, init: RequestInit): Promise<Response> {
    try {
      return await this.#fetch(`${API_BASE}/api${path}`, init);
    } catch {
      // A dropped connection is not a status code; give it one the UI can
      // treat like any other failure rather than an unhandled rejection.
      throw new SitesError(0, null);
    }
  }

  async #json<T>(res: Response): Promise<T> {
    await SitesApi.#rejectFailed(res);
    return (await res.json()) as T;
  }

  /** Turns a non-2xx answer into the shaped [`SitesError`]. */
  static async #rejectFailed(res: Response): Promise<void> {
    if (!res.ok) {
      const problem = (await res.json().catch(() => ({}))) as {
        detail?: unknown;
        reason?: unknown;
      };
      const detail = typeof problem.detail === "string" ? problem.detail : null;
      const reason = typeof problem.reason === "string" ? problem.reason : null;
      throw new SitesError(res.status, detail, reason);
    }
  }
}

async function publicInvitationResponse<T>(response: Response): Promise<T> {
  if (!response.ok) {
    let detail: string | null = null;
    try {
      const body = (await response.json()) as { detail?: unknown };
      detail = typeof body.detail === "string" ? body.detail : null;
    } catch {
      // A dropped proxy or non-JSON response uses the localized caller fallback.
    }
    throw new SitesError(response.status, detail);
  }
  return response.json() as Promise<T>;
}

/** Public token-gated invitation facts. The token is the authority; no user
 * directory or tenant metadata is returned. */
export function siteInvitation(token: string): Promise<SiteInvitation> {
  return fetch(`${API_BASE}/api/sites/invitations/${encodeURIComponent(token)}`).then(
    publicInvitationResponse<SiteInvitation>,
  );
}

/** Sets the invited collaborator's first password and spends the token. */
export function acceptSiteInvitation(
  token: string,
  password: string,
): Promise<SiteInvitation & { status: "accepted" }> {
  return fetch(`${API_BASE}/api/sites/invitations/${encodeURIComponent(token)}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ password }),
  }).then(publicInvitationResponse<SiteInvitation & { status: "accepted" }>);
}

/** The sites client bound to the current session. Memoized per auth context,
 *  so a re-render never re-creates it and effects keyed on it do not loop. */
export function useSitesApi(): SitesApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new SitesApi(authorizedFetch), [authorizedFetch]);
}
