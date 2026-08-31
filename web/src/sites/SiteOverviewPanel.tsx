import { BarChart3, Globe2, History, Languages, Settings } from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
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

  return (
    <section className="grid gap-4 lg:grid-cols-3">
      <article className="rounded-2xl border border-subtle bg-surface p-5 shadow-sm">
        <div className="flex items-start justify-between gap-3">
          <div>
            <h2 className="m-0 text-base font-semibold text-text-primary">
              {strings.sitesOverviewHealth}
            </h2>
            <p className="m-0 mt-1 text-sm text-text-secondary">
              {live ? strings.sitesStatusLive : strings.sitesStatusDraft}
            </p>
          </div>
          <Globe2 className="text-accent" size={20} aria-hidden="true" />
        </div>
        <div className="mt-5 flex flex-col gap-3 text-sm">
          <span className="flex items-center justify-between gap-3">
            <span className="text-text-secondary">{strings.sitesDomains}</span>
            <span className="font-mono font-semibold text-text-primary">
              {host ?? site.subdomain}
            </span>
          </span>
          <span className="flex items-center justify-between gap-3">
            <span className="text-text-secondary">{strings.sitesPages}</span>
            <span className="font-semibold text-text-primary">
              {strings.sitesPageCount(pages.length)}
            </span>
          </span>
        </div>
      </article>

      <article className="rounded-2xl border border-subtle bg-surface p-5 shadow-sm">
        <div className="flex items-start justify-between gap-3">
          <div>
            <h2 className="m-0 text-base font-semibold text-text-primary">
              {strings.sitesLanguages}
            </h2>
            <p className="m-0 mt-1 text-sm text-text-secondary">
              {readinessText(readiness)}
            </p>
          </div>
          <Languages className="text-accent" size={20} aria-hidden="true" />
        </div>
        <div className="mt-5 flex flex-wrap gap-2">
          {site.enabledLocales.map((locale) => (
            <span
              key={locale}
              className="rounded-full bg-surface-raised px-2.5 py-1 font-mono text-xs font-semibold text-text-secondary"
            >
              {locale.toUpperCase()}
            </span>
          ))}
        </div>
      </article>

      <article className="rounded-2xl border border-subtle bg-surface p-5 shadow-sm">
        <div className="flex items-start justify-between gap-3">
          <div>
            <h2 className="m-0 text-base font-semibold text-text-primary">
              {strings.sitesOverviewActions}
            </h2>
            <p className="m-0 mt-1 text-sm text-text-secondary">
              {strings.sitesSiteSettingsHint}
            </p>
          </div>
          <Settings className="text-accent" size={20} aria-hidden="true" />
        </div>
        <div className="mt-5 grid gap-2">
          <Button
            variant="ghost"
            size="sm"
            icon={<BarChart3 size="var(--icon-size-inline)" />}
            onClick={() => onNavigate("analytics")}
          >
            {strings.sitesAnalytics}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            icon={<History size="var(--icon-size-inline)" />}
            onClick={() => onNavigate("history")}
          >
            {strings.sitesHistory}
          </Button>
        </div>
      </article>
    </section>
  );
}
