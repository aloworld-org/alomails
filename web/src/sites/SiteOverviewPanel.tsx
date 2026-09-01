import {
  BarChart3,
  Globe2,
  History,
  Languages,
  Settings,
} from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
import { SiteElementsCard } from "./SiteElementsCard";
import { SiteQualityAuditCard } from "./SiteQualityAuditCard";
import { SiteReadinessCard } from "./SiteReadinessCard";
import { calculateSiteReadiness } from "./siteReadiness";
import type { SiteDetail, SitePage, SiteTranslationReadiness } from "./types";

function readinessText(readiness: SiteTranslationReadiness | null): string {
  if (readiness === null || readiness.totalPages === 0) {
    return strings.sitesTranslationAllReady;
  }
  const missing = readiness.languages.reduce(
    (count, language) => count + readiness.totalPages - language.translatedPages,
    0,
  );
  return missing === 0
    ? strings.sitesTranslationAllReady
    : strings.sitesTranslationPublishHint(missing);
}

export function SiteOverviewPanel({
  site,
  pages,
  host,
  readiness,
  onNavigate,
}: {
  site: SiteDetail;
  pages: SitePage[];
  host: string | null;
  readiness: SiteTranslationReadiness | null;
  onNavigate: (target: string) => void;
}) {
  const live = site.status === "live";
  const siteReadiness = calculateSiteReadiness(site, pages, host, readiness);

  return (
    <div className="grid gap-4">
    <section className="grid items-stretch gap-4 lg:grid-cols-3">
      <article className="rounded-2xl border border-subtle bg-surface p-5 font-ui shadow-sm">
        <div className="flex items-start justify-between gap-3">
          <div>
            <h2 className="m-0 text-base font-semibold text-text-primary">
              {strings.sitesOverviewHealth}
            </h2>
            <p className="m-0 mt-1 text-sm text-text-secondary">
              {live ? strings.sitesStatusLive : strings.sitesStatusDraft}
            </p>
          </div>
          <span className="grid size-9 place-items-center rounded-xl bg-accent-soft text-accent"><Globe2 size={18} aria-hidden="true" /></span>
        </div>
        <div className="mt-4 grid gap-2.5 text-sm">
          <span className="rounded-xl border border-subtle bg-surface px-3.5 py-3 shadow-sm">
            <span className="block text-xs font-medium text-text-secondary">{strings.sitesDomains}</span>
            <span className="mt-1 block truncate font-semibold text-text-primary" title={host ?? site.subdomain}>
              {host ?? site.subdomain}
            </span>
          </span>
          <span className="flex items-center justify-between gap-3 px-1">
            <span className="text-text-secondary">{strings.sitesPages}</span>
            <span className="font-semibold text-text-primary">
              {strings.sitesPageCount(pages.length)}
            </span>
          </span>
        </div>
      </article>

      <article className="rounded-2xl border border-subtle bg-surface p-5 font-ui shadow-sm">
        <div className="flex items-start justify-between gap-3">
          <div>
            <h2 className="m-0 text-base font-semibold text-text-primary">
              {strings.sitesLanguages}
            </h2>
            <p className="m-0 mt-1 text-sm text-text-secondary">
              {readinessText(readiness)}
            </p>
          </div>
          <span className="grid size-9 place-items-center rounded-xl bg-accent-soft text-accent"><Languages size={18} aria-hidden="true" /></span>
        </div>
        <div className="mt-4 flex flex-wrap gap-2">
          {site.enabledLocales.map((locale) => (
            <span
              key={locale}
              className="rounded-full bg-surface-raised px-2.5 py-1 text-xs font-semibold tracking-wide text-text-secondary ring-1 ring-inset ring-subtle"
            >
              {locale.toUpperCase()}
            </span>
          ))}
        </div>
      </article>

      <article className="rounded-2xl border border-subtle bg-surface p-5 font-ui shadow-sm">
        <div className="flex items-start justify-between gap-3">
          <div>
            <h2 className="m-0 text-base font-semibold text-text-primary">
              {strings.sitesOverviewActions}
            </h2>
            <p className="m-0 mt-1 text-sm text-text-secondary">
              {strings.sitesSiteSettingsHint}
            </p>
          </div>
          <span className="grid size-9 place-items-center rounded-xl bg-accent-soft text-accent"><Settings size={18} aria-hidden="true" /></span>
        </div>
        <div className="mt-4 flex flex-wrap items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            className="!justify-start !rounded-xl !border !border-default !bg-surface !px-3 hover:!border-accent hover:!bg-accent-soft"
            icon={<BarChart3 size="var(--icon-size-inline)" />}
            onClick={() => onNavigate("analytics")}
          >
            {strings.sitesAnalytics}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="!justify-start !rounded-xl !border !border-default !bg-surface !px-3 hover:!border-accent hover:!bg-accent-soft"
            icon={<History size="var(--icon-size-inline)" />}
            onClick={() => onNavigate("history")}
          >
            {strings.sitesHistory}
          </Button>
        </div>
      </article>
    </section>
    <section className="grid items-stretch gap-4 lg:grid-cols-[minmax(0,1.7fr)_minmax(18rem,1fr)]">
      <SiteReadinessCard readiness={siteReadiness} />
      <SiteElementsCard
        elements={siteReadiness.elements}
        onAction={() => onNavigate(pages[0] ? `pages/${pages[0].id}` : "?section=pages")}
      />
    </section>
    <SiteQualityAuditCard quality={siteReadiness.quality} />
    </div>
  );
}
