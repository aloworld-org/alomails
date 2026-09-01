import { AlignLeft, CheckCircle2, Circle, FileType2, Image, SearchCheck } from "lucide-react";

import { strings } from "../i18n";
import type { SiteReadinessResult } from "./siteReadiness";

function ratioWidth(complete: number, total: number): string {
  if (total === 0 || complete === 0) return "w-0";
  const ratio = complete / total;
  if (ratio <= 0.25) return "w-1/4";
  if (ratio <= 0.5) return "w-1/2";
  if (ratio <= 0.75) return "w-3/4";
  return "w-full";
}

export function SiteQualityAuditCard({ quality }: { quality: SiteReadinessResult["quality"] }) {
  const ratios = [
    [strings.sitesOverviewSeoTitles, quality.seoTitles, quality.pages, FileType2],
    [strings.sitesOverviewMetaDescriptions, quality.metaDescriptions, quality.pages, AlignLeft],
    [strings.sitesOverviewImageDescriptions, quality.imagesWithAlt, quality.images, Image],
  ] as const;
  const assets = [
    [strings.sitesOverviewLogo, quality.logo],
    [strings.sitesOverviewFavicon, quality.favicon],
  ] as const;

  return (
    <article className="rounded-2xl border border-subtle bg-surface p-5 font-ui shadow-sm sm:p-6">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h2 className="m-0 text-base font-semibold text-text-primary">{strings.sitesOverviewQuality}</h2>
          <p className="m-0 mt-1 max-w-3xl text-sm leading-5 text-text-secondary">{strings.sitesOverviewQualityHint}</p>
        </div>
        <SearchCheck className="size-5 shrink-0 text-accent" aria-hidden="true" />
      </div>

      <div className="mt-5 grid gap-3 md:grid-cols-3">
        {ratios.map(([label, complete, total, Icon]) => {
          const noImages = label === strings.sitesOverviewImageDescriptions && total === 0;
          const shownComplete = noImages ? 0 : complete;
          return (
            <div key={label} className="rounded-xl border border-subtle bg-surface p-4">
              <div className="flex items-start justify-between gap-3">
                <span className="grid size-9 place-items-center rounded-xl bg-accent-soft text-accent"><Icon className="size-4" aria-hidden="true" /></span>
                <strong className="text-sm font-semibold tabular-nums text-text-secondary">{noImages ? "—" : `${shownComplete}/${total}`}</strong>
              </div>
              <span className="mt-4 block text-sm font-semibold text-text-primary">{label}</span>
              <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-raised">
                <span className={`block h-full rounded-full bg-accent ${ratioWidth(complete, total)}`} />
              </div>
            </div>
          );
        })}
      </div>

      <div className="mt-3 grid gap-3 sm:grid-cols-2">
        {assets.map(([label, complete]) => (
          <span key={label} className={`inline-flex min-h-12 items-center gap-3 rounded-xl border bg-surface px-4 text-sm font-medium text-text-primary ${complete ? "border-success/20" : "border-dashed border-default"}`}>
            {complete ? (
              <span className="grid size-7 place-items-center rounded-full bg-success-tint"><CheckCircle2 className="size-4 text-success" aria-hidden="true" /></span>
            ) : (
              <span className="grid size-7 place-items-center rounded-full bg-surface"><Circle className="size-4 text-tertiary" aria-hidden="true" /></span>
            )}
            {label}
          </span>
        ))}
      </div>
    </article>
  );
}
