// The wire types of the `/sites/*` edit surface that this module reads —
// deliberately only the fields the screens render, so a server that says more
// (timestamps and fields the UI does not show yet) never
// forces a change here. The server is the authority on every rule; these
// types carry its answers, they do not re-state its validation.
import type { Section, SectionKind, SectionsEnvelope } from "./sections";

/** A site as the list answers it. */
export interface Site {
  id: string;
  name: string;
  subdomain: string;
  /** `draft` until the first publish; `live` while a published set serves. */
  status: "draft" | "live";
  /** The language served at unprefixed public paths. */
  defaultLocale: string;
  /** Enabled languages in the order visitors see them. */
  enabledLocales: string[];
}

/** One site with its current publish (`null` while unpublished) and its
 *  stored theme envelope. */
export interface SiteDetail extends Site {
  publish: { id: string; publishedAt: string } | null;
  /** True only for the creator or a workspace admin. Restricted editors can
   * edit the site but never see or operate its sharing controls. */
  canManageCollaborators: boolean;
  /** The stored theme. A site that never set one stores `{}`, so every
   *  field reads as possibly absent; absent means the default. */
  theme: StoredTheme;
}

export interface SiteCollaborator {
  id: string;
  email: string;
  status: "pending" | "active";
}

export interface SiteCollaboratorInvite {
  collaborator: SiteCollaborator;
  /** Present only for a new or refreshed pending invitation. */
  inviteUrl: string | null;
  expiresInHours: number | null;
}

export interface SiteInvitation {
  email: string;
  siteName: string;
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
  /** Optional search/share overrides. `null` means use the page/site default. */
  seoTitle: string | null;
  seoDescription: string | null;
}

/** One page with its sections envelope — what the editor loads and edits. */
export interface SitePageDetail extends SitePage {
  sections: SectionsEnvelope;
}

/** One requested-language page draft. A fallback is readable for reference
 * but must be explicitly copied before edits can write that language. */
export interface LocalizedSitePageDetail extends SitePageDetail {
  requestedLocale: string;
  resolvedLocale: string;
  fallback: boolean;
}

export interface SiteTranslationLanguage {
  locale: string;
  translatedPages: number;
  ready: boolean;
}

export interface SiteTranslationReadiness {
  defaultLocale: string;
  totalPages: number;
  languages: SiteTranslationLanguage[];
}

export interface SiteTranslationPageSnapshot {
  id: string;
  title: string;
  slug: string;
  seo_title: string | null;
  seo_description: string | null;
  sections: SectionsEnvelope;
}

export interface SiteTranslationPostSnapshot {
  id: string;
  title: string;
  slug: string;
  excerpt: string;
}

export interface SiteTranslationEnvelope {
  schema_version: number;
  source_locale: string;
  target_locale: string;
  pages: {
    before: SiteTranslationPageSnapshot;
    after: SiteTranslationPageSnapshot;
  }[];
  posts: {
    before: SiteTranslationPostSnapshot;
    after: SiteTranslationPostSnapshot;
  }[];
}

/** One blog post linked to its source alo Doc. The document remains the
 *  authoring source; Sites stores only the public metadata and publish state. */
export interface SitePost {
  id: string;
  docNodeId: string;
  slug: string;
  title: string;
  excerpt: string;
  coverBlobId: string | null;
  status: "draft" | "published";
  publishedAt: string | null;
  createdAt: string;
  updatedAt: string;
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

export interface SiteCollectionFieldMapping {
  title: string;
  slug: string | null;
  summary: string | null;
  body: string | null;
  image: string | null;
  link: string | null;
  publishedAt: string | null;
}

/** One saved connection from this site to a readable alo Base table. */
export interface SiteCollection {
  id: string;
  name: string;
  baseNodeId: string;
  baseTableId: string;
  mapping: SiteCollectionFieldMapping;
  createdAt: string;
  updatedAt: string;
}

export interface SiteCollectionDraft {
  name: string;
  baseNodeId: string;
  baseTableId: string;
  mapping: SiteCollectionFieldMapping;
}

export interface SiteCollectionSourceField {
  id: string;
  name: string;
  type: "text" | "date" | "attachment";
}

export interface SiteCollectionSourceTable {
  id: string;
  name: string;
  fields: SiteCollectionSourceField[];
  recordCount: number;
}

/** A Base the caller can already read in Drive, ready to connect in Sites. */
export interface SiteCollectionSource {
  nodeId: string;
  name: string;
  tables: SiteCollectionSourceTable[];
}

export interface SiteCollectionPreviewItem {
  title: string;
  slug: string | null;
  summary: string | null;
  body: string | null;
  imageBlobId: string | null;
  link: string | null;
  publishedAt: string | null;
}

export interface SiteCollectionPreview {
  id: string;
  name: string;
  items: SiteCollectionPreviewItem[];
}

/** One version of a website: a publish that happened, and what it froze.
 *  `pages` and `collections` are counts — the paths themselves are a separate
 *  read, because a history list has no use for every path in every version. */
export interface SitePublishVersion {
  id: string;
  publishedAt: string;
  /** The user id that published it (the server never sends an address here). */
  publishedBy: string;
  defaultLocale: string;
  enabledLocales: string[];
  /** The version this one is a copy of, when it was made by a restore. */
  restoredFrom: string | null;
  /** True for the version currently on the internet. */
  current: boolean;
  pages: number;
  /** The languages this version actually froze pages for. */
  locales: string[];
  collections: number;
}

/** One page a version froze, as the preview switcher lists them. */
export interface SitePublishPage {
  pageId: string;
  locale: string;
  slug: string;
  title: string;
  home: boolean;
  navOrder: number;
}

/** How one page differs between two versions. `fields` is empty unless the
 *  page exists in both and its frozen content differs. */
export interface SitePublishPageChange {
  pageId: string;
  locale: string;
  slug: string;
  title: string;
  change: "added" | "removed" | "changed";
  fields: string[];
}

/** How one collection differs between two versions. */
export interface SitePublishCollectionChange {
  collectionId: string;
  name: string;
  change: "added" | "removed" | "changed";
  itemsBefore: number;
  itemsAfter: number;
}

/** What a visitor would see differently between two versions — metadata
 *  only; the section content itself is what the preview shows. */
export interface SitePublishComparison {
  from: SitePublishVersion;
  to: SitePublishVersion;
  identical: boolean;
  themeChanged: boolean;
  defaultLocaleChanged: boolean;
  localesAdded: string[];
  localesRemoved: string[];
  pages: SitePublishPageChange[];
  unchangedPages: number;
  collections: SitePublishCollectionChange[];
  unchangedCollections: number;
}

/** Where one scheduled publish is in its life. `scheduled` is waiting,
 *  `publishing` is happening right now; the other three are terminal and are
 *  kept so the owner reads what happened instead of watching an entry vanish. */
export type SitePublishScheduleStatus =
  | "scheduled"
  | "publishing"
  | "published"
  | "cancelled"
  | "failed";

/** One intention to publish a website at a chosen moment. `publishAt` is an
 *  RFC 3339 instant in UTC — every screen turns it into the reader's own
 *  time, and never shows the raw string. */
export interface SitePublishSchedule {
  id: string;
  siteId: string;
  publishAt: string;
  status: SitePublishScheduleStatus;
  requestedBy: string;
  createdAt: string;
  updatedAt: string;
  finishedAt: string | null;
  attempts: number;
  /** The version it produced, once it produced one. */
  publishId: string | null;
  /** Why it could not publish, in the server's own words. */
  lastError: string | null;
}

/** The answer to a restore: the NEW version now live, and the one it copied. */
export interface SitePublishRestore {
  publishId: string;
  restoredFrom: string;
}

/** Privacy-preserving traffic aggregates for one inclusive period. */
export interface SiteAnalyticsReport {
  from: string;
  to: string;
  totals: { visits: number; uniqueVisitors: number };
  daily: Array<{ date: string; visits: number; uniqueVisitors: number }>;
  topPages: Array<{ path: string; visits: number; uniqueVisitors: number }>;
  topReferrers: Array<{
    domain: string;
    visits: number;
    uniqueVisitors: number;
  }>;
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

/** The atomic draft returned by the description-to-site endpoint. Every page
 *  already carries its generated section stack, and the site is always a
 *  private draft until its owner explicitly publishes it. */
export interface GeneratedSiteDraft {
  site: Site;
  pages: SitePageDetail[];
}

export interface SiteEditTarget {
  index: number;
  type: SectionKind;
}

/** The closed operation vocabulary returned for review by page AI editing. */
export type SiteEditOperation =
  | { op: "add_section"; at: number; section: Section }
  | { op: "remove_section"; target: SiteEditTarget }
  | { op: "reorder_section"; target: SiteEditTarget; to: number }
  | { op: "set_prop"; target: SiteEditTarget; pointer: string; value: unknown }
  | {
      op: "rewrite_copy";
      target: SiteEditTarget;
      pointer: string;
      text: string;
    };

export interface SiteEditEnvelope {
  schema_version: number;
  operations: SiteEditOperation[];
}

/** A validated no-write proposal together with the exact public-renderer
 *  document that approval would produce. */
export interface ProposedSiteEdit {
  proposal: SiteEditEnvelope;
  previewHtml: string;
}

export type SiteCopyAction = "rewrite" | "tone" | "shorter" | "longer";

/** One deliberately narrow copy request. The server accepts a result only
 *  when it contains one rewrite operation for this exact string leaf. */
export interface SiteCopyRequest {
  target: SiteEditTarget;
  pointer: string;
  action: SiteCopyAction;
  tone?: string | undefined;
}

/** What the create-page form sends. The empty slug is only accepted by the
 *  server together with `home: true` — it is the home page's spelling. */
export interface PageDraft {
  title: string;
  slug: string;
  home: boolean;
}

/** Metadata that binds a new alo Doc to a draft blog post. */
export interface PostDraft {
  docNodeId: string;
  slug: string;
  title: string;
  excerpt: string;
  coverBlobId?: string | undefined;
}

/** The complete replace-body for a post's editable public metadata. */
export interface PostUpdate {
  slug: string;
  title: string;
  excerpt: string;
  coverBlobId: string | null;
}
