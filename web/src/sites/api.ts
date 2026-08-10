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
import { API_BASE } from "../platform/runtime";
import type { Section, SectionsEnvelope } from "./sections";
import type {
  PageDraft,
  PostDraft,
  PostUpdate,
  Site,
  SiteAnalyticsReport,
  SiteDetail,
  SiteDraft,
  GeneratedSiteDraft,
  SiteEditEnvelope,
  ProposedSiteEdit,
  SiteCopyRequest,
  SitePage,
  SitePageDetail,
  SitePost,
  SiteSubmission,
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

  /** One site with its current publish (`null` while unpublished). */
  site(id: string): Promise<SiteDetail> {
    return this.#read<SiteDetail>(`/sites/${encodeURIComponent(id)}`);
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

  /** The draft page rendered by the server as one complete, self-contained
   *  HTML document — the editor's preview. Answers text, not JSON; the
   *  caller puts it in a sandboxed iframe via `srcdoc`. */
  async pagePreview(siteId: string, pageId: string): Promise<string> {
    const res = await this.#send(`${this.#pagePath(siteId, pageId)}/preview`, { method: "GET" });
    await SitesApi.#rejectFailed(res);
    return res.text();
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

  #postPath(siteId: string, postId: string): string {
    return `/sites/${encodeURIComponent(siteId)}/posts/${encodeURIComponent(postId)}`;
  }

  /** Every section op answers `{"sections": <envelope>}` — unwraps it. */
  async #sections(answer: Promise<{ sections: SectionsEnvelope }>): Promise<SectionsEnvelope> {
    return (await answer).sections;
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
      return await this.#fetch(`${API_BASE}${path}`, init);
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

/** The sites client bound to the current session. Memoized per auth context,
 *  so a re-render never re-creates it and effects keyed on it do not loop. */
export function useSitesApi(): SitesApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new SitesApi(authorizedFetch), [authorizedFetch]);
}
