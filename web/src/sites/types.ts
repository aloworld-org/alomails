// The wire types of the `/sites/*` edit surface that this module reads —
// deliberately only the fields the screens render, so a server that says more
// (theme envelopes, SEO fields, timestamps the UI does not show yet) never
// forces a change here. The server is the authority on every rule; these
// types carry its answers, they do not re-state its validation.
import type { SectionsEnvelope } from "./sections";

/** A site as the list answers it. */
export interface Site {
  id: string;
  name: string;
  subdomain: string;
  /** `draft` until the first publish; `live` while a published set serves. */
  status: "draft" | "live";
}

/** One site with its current publish (`null` while unpublished) and its
 *  stored theme envelope. */
export interface SiteDetail extends Site {
  publish: { id: string; publishedAt: string } | null;
  /** The stored theme. A site that never set one stores `{}`, so every
   *  field reads as possibly absent; absent means the default. */
  theme: StoredTheme;
}

/** The theme envelope as stored — `{}` until the first save. */
export interface StoredTheme {
  schema_version?: number | undefined;
  preset?: string | undefined;
  logo?: string | undefined;
  favicon?: string | undefined;
}

/** What the theme form sends: always the full current-version envelope,
 *  with absent logo/favicon spelled as absent keys (never null). */
export interface ThemeEnvelope {
  schema_version: number;
  preset: string;
  logo?: string | undefined;
  favicon?: string | undefined;
}

/** One shipped theme preset as `/sites/theme-presets` answers it — the
 *  tokens the picker renders its swatches and type sample from. */
export interface ThemePreset {
  id: string;
  name: string;
  palette: {
    background: string;
    surface: string;
    text: string;
    mutedText: string;
    primary: string;
    onPrimary: string;
    border: string;
  };
  typography: {
    headingFamily: string;
    bodyFamily: string;
    headingWeight: number;
  };
}

/** A page as the per-site list answers it (lean: no sections envelope). */
export interface SitePage {
  id: string;
  /** URL path segment; the home page's slug is the empty string. */
  slug: string;
  title: string;
  home: boolean;
}

/** One page with its sections envelope — what the editor loads and edits. */
export interface SitePageDetail extends SitePage {
  sections: SectionsEnvelope;
}

/** One visitor message in the site's contact-form inbox. */
export interface SiteSubmission {
  id: string;
  formId: string;
  formName: string;
  senderName: string;
  senderEmail: string;
  message: string;
  handled: boolean;
  receivedAt: string;
}

/** The deployment-wide sites config: published sites serve at
 *  `<subdomain>.<domain>`. The UI composes "goes live at" copy and live
 *  links from it — the domain is the server's to know, never hardcoded. */
export interface SitesConfig {
  domain: string;
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
