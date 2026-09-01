import { ArrowRight, LayoutPanelTop, Layers3, Menu, MousePointerClick, PanelTop } from "lucide-react";

import { strings } from "../i18n";
import type { SiteElementCounts } from "./siteReadiness";

const widthClass = (count: number): string => {
  if (count === 0) return "w-0";
  if (count === 1) return "w-1/3";
  if (count === 2) return "w-2/3";
  return "w-full";
};

export function SiteElementsCard({ elements, onAction }: { elements: SiteElementCounts; onAction: () => void }) {
  const rows = [
    [strings.sitesOverviewNavigationElements, elements.navigation, Menu],
    [strings.sitesOverviewHeroElements, elements.hero, PanelTop],
    [strings.sitesOverviewContentElements, elements.content, Layers3],
    [strings.sitesOverviewActionElements, elements.action, MousePointerClick],
  ] as const;
  const nextStep = elements.hero === 0
    ? strings.sitesOverviewAddIntroduction
    : elements.content === 0
      ? strings.sitesOverviewAddContent
      : strings.sitesOverviewAddAction;

  return (
    <article className="flex flex-col rounded-2xl border border-subtle bg-surface p-5 font-ui shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h2 className="m-0 text-lg font-semibold tracking-tight text-text-primary">{strings.sitesOverviewElements}</h2>
          <p className="m-0 mt-1 text-sm leading-5 text-text-secondary">{strings.sitesOverviewElementsHint}</p>
        </div>
        <LayoutPanelTop className="size-5 shrink-0 text-accent" aria-hidden="true" />
      </div>
      <div className="mt-5 grid grid-cols-2 gap-3">
        {rows.map(([label, count, Icon]) => (
          <div key={label} className={`rounded-xl border p-3.5 ${count > 0 ? "border-accent/20 bg-accent-soft/50" : "border-subtle bg-surface"}`}>
            <div className="flex items-start justify-between gap-2">
              <span className={`grid size-8 place-items-center rounded-lg shadow-sm ${count > 0 ? "bg-surface text-accent" : "bg-raised text-tertiary"}`}><Icon className="size-4" aria-hidden="true" /></span>
              <strong className={`text-xl font-bold tabular-nums ${count > 0 ? "text-accent" : "text-text-tertiary"}`}>{count}</strong>
            </div>
            <span className="mt-3 block truncate text-xs font-medium text-text-secondary">{label}</span>
            <span className="mt-2 block h-1 overflow-hidden rounded-full bg-surface">
              <span className={`block h-full rounded-full bg-accent transition-[width] ${widthClass(count)}`} />
            </span>
          </div>
        ))}
      </div>
      <div className="mt-auto pt-5">
        <div className="rounded-xl bg-[linear-gradient(135deg,var(--accent-soft),var(--bg-surface-raised))] p-4 ring-1 ring-inset ring-accent/10">
          <span className="text-[0.6875rem] font-semibold uppercase tracking-[0.08em] text-accent">{strings.sitesOverviewRecommendedNext}</span>
          <strong className="mt-1.5 block text-sm text-text-primary">{nextStep}</strong>
          <button type="button" className="mt-3 inline-flex min-h-9 items-center gap-2 rounded-lg bg-accent px-3.5 text-xs font-semibold text-on-accent shadow-sm transition-[filter,transform] hover:brightness-95 active:scale-[0.98] focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/20" onClick={onAction}>
            {strings.sitesOverviewEditPage}<ArrowRight className="size-3.5" aria-hidden="true" />
          </button>
        </div>
      </div>
    </article>
  );
}
