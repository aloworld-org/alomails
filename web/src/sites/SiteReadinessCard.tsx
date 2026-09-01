import { Accessibility, BadgeCheck, FileStack, Globe2, Languages, Rocket, Search } from "lucide-react";

import { strings } from "../i18n";
import type { SiteReadinessResult } from "./siteReadiness";

const widthClass = (value: number): string => {
  if (value <= 0) return "w-0";
  if (value <= 25) return "w-1/4";
  if (value <= 50) return "w-1/2";
  if (value <= 75) return "w-3/4";
  return "w-full";
};

export function SiteReadinessCard({ readiness }: { readiness: SiteReadinessResult }) {
  const metrics = [
    [strings.sitesOverviewFoundation, readiness.foundation, Globe2],
    [strings.sitesOverviewContent, readiness.content, FileStack],
    [strings.sitesOverviewSeo, readiness.seo, Search],
    [strings.sitesOverviewAccessibility, readiness.accessibility, Accessibility],
    [strings.sitesOverviewBranding, readiness.branding, BadgeCheck],
    [strings.sitesOverviewLocalization, readiness.localization, Languages],
    [strings.sitesOverviewLaunch, readiness.launch, Rocket],
  ] as const;
  const circumference = 2 * Math.PI * 38;
  const dashOffset = circumference * (1 - readiness.overall / 100);

  return (
    <article className="overflow-hidden rounded-2xl border border-subtle bg-surface font-ui shadow-sm">
      <div className="flex flex-wrap items-center justify-between gap-5 bg-[linear-gradient(135deg,var(--bg-surface),var(--accent-soft))] px-5 py-5 sm:px-6">
        <div className="max-w-xl">
          <span className="mb-2 inline-flex items-center rounded-full bg-surface/80 px-2.5 py-1 text-[0.6875rem] font-semibold uppercase tracking-[0.08em] text-accent shadow-sm">
            {strings.sitesOverviewReadinessScore}
          </span>
          <h2 className="m-0 text-lg font-semibold tracking-tight text-text-primary">{strings.sitesOverviewReadiness}</h2>
          <p className="m-0 mt-1 text-sm leading-5 text-text-secondary">{strings.sitesOverviewReadinessHint}</p>
        </div>
        <div className="relative grid size-24 shrink-0 place-items-center" aria-label={`${strings.sitesOverviewReadinessScore}: ${readiness.overall}%`}>
          <svg className="absolute inset-0 size-full -rotate-90" viewBox="0 0 88 88" aria-hidden="true">
            <circle cx="44" cy="44" r="38" fill="none" stroke="var(--bg-surface)" strokeWidth="7" />
            <circle cx="44" cy="44" r="38" fill="none" stroke="var(--accent)" strokeWidth="7" strokeLinecap="round" strokeDasharray={circumference} strokeDashoffset={dashOffset} />
          </svg>
          <strong className="text-xl font-bold tabular-nums text-accent">{readiness.overall}%</strong>
        </div>
      </div>

      <div className="grid gap-x-8 px-5 py-3 sm:grid-cols-2 sm:px-6">
        {metrics.map(([label, value, Icon]) => (
          <div key={label} className="border-b border-subtle py-3.5">
            <div className="mb-2.5 flex items-center gap-2.5 text-xs">
              <span className={`grid size-7 place-items-center rounded-lg ${value > 0 ? "bg-accent-soft text-accent" : "bg-raised text-tertiary"}`}><Icon className="size-3.5" aria-hidden="true" /></span>
              <span className="flex-1 font-semibold text-text-primary">{label}</span>
              <span className={`font-semibold tabular-nums ${value === 100 ? "text-success" : "text-text-secondary"}`}>{value}%</span>
            </div>
            <div className="h-1.5 overflow-hidden rounded-full bg-raised">
              <span className={`block h-full rounded-full transition-[width] ${value === 100 ? "bg-success" : "bg-accent"} ${widthClass(value)}`} />
            </div>
          </div>
        ))}
      </div>
    </article>
  );
}
