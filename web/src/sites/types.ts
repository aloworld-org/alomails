// The wire types of the `/sites/*` edit surface that this module reads —
// deliberately only the fields the screens render, so a server that says more
// (theme envelopes, SEO fields, timestamps the UI does not show yet) never
// forces a change here. The server is the authority on every rule; these
// types carry its answers, they do not re-state its validation.

/** A site as the list answers it. */
export interface Site {
  id: string;
  name: string;
  subdomain: string;
  /** `draft` until the first publish; `live` while a published set serves. */
  status: "draft" | "live";
}

/** One site with its current publish (`null` while unpublished). */
export interface SiteDetail extends Site {
  publish: { id: string; publishedAt: string } | null;
}

/** A page as the per-site list answers it (lean: no sections envelope). */
export interface SitePage {
  id: string;
  /** URL path segment; the home page's slug is the empty string. */
  slug: string;
  title: string;
  home: boolean;
}

/** The live taken/free answer for the create form. */
export interface SubdomainCheck {
  subdomain: string;
  available: boolean;
}

/** What the create-site form sends. */
export interface SiteDraft {
  name: string;
  subdomain: string;
}

/** What the create-page form sends. The empty slug is only accepted by the
 *  server together with `home: true` — it is the home page's spelling. */
export interface PageDraft {
  title: string;
  slug: string;
  home: boolean;
}
