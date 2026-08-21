// One site's privacy-friendly traffic desk: immediate totals, a complete
// daily series, and — grouped so the screen stays readable — where people came
// from, what they looked at, and how they read it. There is no tracking setup
// or consent ceremony because collection stores no cookies or personal
// browsing profile.
import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import {
  ArrowLeft,
  BarChart3,
  MousePointerClick,
  ShieldCheck,
} from "lucide-react";

import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { AnalyticsGroup, DimensionPanel } from "./AnalyticsPanels";
import type { AnalyticsRow } from "./AnalyticsPanels";
import {
  campaignLabel,
  countryLabel,
  deviceLabel,
  outboundLabel,
  pathLabel,
  readTimeLabel,
  referrerLabel,
} from "./analyticsLabels";
import { EmptyState, ErrorBanner } from "./parts";
import type {
  SiteAnalyticsDimension,
  SiteAnalyticsReport,
  SiteDetail,
} from "./types";

/** Names one dimension's buckets for reading, keeping the server's order —
 *  which is by count everywhere except the reading-time histogram. */
function named(
  rows: SiteAnalyticsDimension[] | undefined,
  name: (label: string) => string,
): AnalyticsRow[] {
  return (rows ?? []).map((row) => ({
    label: name(row.label),
    visits: row.visits,
  }));
}

type AnalyticsPeriod = 7 | 30 | 90;
const PERIODS: AnalyticsPeriod[] = [7, 30, 90];

export function AnalyticsView() {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [site, setSite] = useState<SiteDetail | null>(null);
  const [domain, setDomain] = useState<string | null>(null);
  const [period, setPeriod] = useState<AnalyticsPeriod>(30);
  const [report, setReport] = useState<SiteAnalyticsReport | null>(null);
  const [siteError, setSiteError] = useState<string | null>(null);
  const [reportError, setReportError] = useState<string | null>(null);
  const [loadingSite, setLoadingSite] = useState(true);
  const [loadingReport, setLoadingReport] = useState(true);

  useEffect(() => {
    let current = true;
    setLoadingSite(true);
    void Promise.all([api.site(siteId), api.config().catch(() => null)])
      .then(([detail, config]) => {
        if (!current) return;
        setSite(detail);
        setDomain(config?.domain ?? null);
        setSiteError(null);
      })
      .catch((error: unknown) => {
        if (current)
          setSiteError(sitesMessage(error, strings.sitesSiteLoadFailed));
      })
      .finally(() => {
        if (current) setLoadingSite(false);
      });
    return () => {
      current = false;
    };
  }, [api, siteId]);

  useEffect(() => {
    let current = true;
    setLoadingReport(true);
    void api
      .analytics(siteId, period)
      .then((answer) => {
        if (!current) return;
        setReport(answer);
        setReportError(null);
      })
      .catch((error: unknown) => {
        if (current)
          setReportError(sitesMessage(error, strings.sitesAnalyticsLoadFailed));
      })
      .finally(() => {
        if (current) setLoadingReport(false);
      });
    return () => {
      current = false;
    };
  }, [api, period, siteId]);

  const dates = useMemo(
    () =>
      new Intl.DateTimeFormat(undefined, {
        month: "short",
        day: "numeric",
      }),
    [],
  );
  const numbers = useMemo(() => new Intl.NumberFormat(), []);
  const maxVisits =
    report?.daily.reduce((max, row) => Math.max(max, row.visits), 0) ?? 0;
  const liveAddress =
    site?.status === "live" && domain !== null
      ? `https://${site.subdomain}.${domain}`
      : null;

  function openSite() {
    if (liveAddress !== null) {
      window.open(liveAddress, "_blank", "noopener,noreferrer");
    } else {
      navigate(`/sites/${encodeURIComponent(siteId)}`);
    }
  }

  return (
    <div className="mx-auto flex w-full max-w-[90rem] flex-col gap-5 px-4 py-5 sm:px-6 sm:py-7 lg:px-8">
      <header className="flex flex-col gap-4 rounded-2xl border border-subtle bg-surface-raised p-5 shadow-sm sm:p-6">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <Link
            className="inline-flex min-h-10 items-center gap-2 rounded-xl bg-surface px-3.5 py-2 text-sm font-medium text-primary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            to={`/sites/${encodeURIComponent(siteId)}`}
          >
            <ArrowLeft size={18} />
            {strings.sitesBackToSite}
          </Link>
          {/* The attention map is a drill-down of these numbers, so it is
            reached from here rather than from a fifth button on the site
            page. */}
          <Link
            className="inline-flex min-h-10 items-center gap-2 rounded-xl bg-surface px-3.5 py-2 text-sm font-medium text-primary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            to={`/sites/${encodeURIComponent(siteId)}/heatmap`}
          >
            <MousePointerClick size={18} aria-hidden="true" />
            {strings.sitesHeatmap}
          </Link>
        </div>
        <div className="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
          <div>
            <h1 className="text-2xl font-semibold tracking-tight text-primary sm:text-3xl">
              {strings.sitesAnalytics}
            </h1>
            {site !== null && (
              <p className="mt-1 text-sm text-secondary">{site.name}</p>
            )}
          </div>
          <div
            className="inline-flex self-start rounded-xl bg-surface p-1"
            aria-label={strings.sitesAnalyticsPeriod}
          >
            {PERIODS.map((days) => (
              <button
                type="button"
                key={days}
                className={`min-h-9 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${period === days ? "bg-accent-soft text-accent shadow-sm" : "text-secondary hover:bg-surface-raised hover:text-primary"}`}
                aria-pressed={period === days}
                onClick={() => setPeriod(days)}
              >
                {strings.sitesAnalyticsDays(days)}
              </button>
            ))}
          </div>
        </div>
      </header>

      {siteError !== null && <ErrorBanner message={siteError} />}
      {reportError !== null && <ErrorBanner message={reportError} />}

      {loadingSite || loadingReport ? (
        <div
          className="grid gap-4 sm:grid-cols-3"
          role="status"
          aria-label={strings.sitesAnalyticsLoading}
        >
          <span className="h-28 animate-pulse rounded-2xl bg-surface" />
          <span className="h-28 animate-pulse rounded-2xl bg-surface" />
          <span className="h-28 animate-pulse rounded-2xl bg-surface" />
        </div>
      ) : report !== null && report.totals.visits === 0 ? (
        <>
          <PrivacyNote />
          <EmptyState
            Icon={BarChart3}
            title={strings.sitesAnalyticsEmptyTitle}
            body={strings.sitesAnalyticsEmptyBody}
            cta={
              liveAddress !== null
                ? strings.sitesAnalyticsOpenSite
                : strings.sitesOpenPages
            }
            onCta={openSite}
          />
        </>
      ) : report !== null ? (
        <div className="space-y-7">
          <PrivacyNote />
          <section
            className="grid gap-4 sm:grid-cols-2"
            aria-label={strings.sitesAnalyticsSummary}
          >
            <article className="rounded-2xl border border-subtle bg-surface-raised p-5 shadow-sm">
              <span className="text-sm font-medium text-secondary">
                {strings.sitesAnalyticsVisits}
              </span>
              <strong className="mt-2 block text-3xl font-semibold tracking-tight text-primary">
                {numbers.format(report.totals.visits)}
              </strong>
            </article>
            <article className="rounded-2xl border border-subtle bg-surface-raised p-5 shadow-sm">
              <span className="text-sm font-medium text-secondary">
                {strings.sitesAnalyticsVisitors}
              </span>
              <strong className="mt-2 block text-3xl font-semibold tracking-tight text-primary">
                {numbers.format(report.totals.uniqueVisitors)}
              </strong>
            </article>
          </section>

          <section className="rounded-2xl border border-subtle bg-surface-raised p-5 shadow-sm sm:p-6">
            <div className="flex flex-wrap items-baseline justify-between gap-2">
              <h2 className="text-lg font-semibold tracking-tight text-primary">
                {strings.sitesAnalyticsOverTime}
              </h2>
              <span className="text-sm text-secondary">
                {dates.format(new Date(`${report.from}T00:00:00`))} –{" "}
                {dates.format(new Date(`${report.to}T00:00:00`))}
              </span>
            </div>
            <ol
              className="mt-6 flex h-44 items-end gap-1.5 border-b border-subtle px-1"
              aria-label={strings.sitesAnalyticsChartLabel}
            >
              {report.daily.map((day) => (
                <li
                  key={day.date}
                  aria-label={strings.sitesAnalyticsDayLabel(
                    dates.format(new Date(`${day.date}T00:00:00`)),
                    day.visits,
                  )}
                >
                  className="flex h-full min-w-1 flex-1 items-end"
                  <span
                    className="block min-h-1 w-full rounded-t-md bg-accent transition-[height]"
                    style={{
                      height: `${maxVisits === 0 ? 0 : (day.visits / maxVisits) * 100}%`,
                    }}
                    aria-hidden="true"
                  />
                </li>
              ))}
            </ol>
          </section>

          <AnalyticsGroup title={strings.sitesAnalyticsGroupArrival}>
            <DimensionPanel
              title={strings.sitesAnalyticsTopReferrers}
              note={strings.sitesAnalyticsReferrersNote}
              empty={strings.sitesAnalyticsReferrersEmpty}
              rows={report.topReferrers.map((row) => ({
                label: referrerLabel(row.domain),
                visits: row.visits,
              }))}
              numbers={numbers}
            />
            <DimensionPanel
              title={strings.sitesAnalyticsCampaigns}
              note={strings.sitesAnalyticsCampaignsNote}
              empty={strings.sitesAnalyticsCampaignsEmpty}
              rows={named(report.campaigns, campaignLabel)}
              numbers={numbers}
            />
            <DimensionPanel
              title={strings.sitesAnalyticsCountries}
              note={strings.sitesAnalyticsCountriesNote}
              empty={strings.sitesAnalyticsCountriesEmpty}
              rows={named(report.countries, countryLabel)}
              numbers={numbers}
            />
          </AnalyticsGroup>

          <AnalyticsGroup title={strings.sitesAnalyticsGroupPages}>
            <DimensionPanel
              title={strings.sitesAnalyticsTopPages}
              note={strings.sitesAnalyticsTopPagesNote}
              empty={strings.sitesAnalyticsPagesEmpty}
              rows={report.topPages.map((row) => ({
                label: pathLabel(row.path),
                visits: row.visits,
              }))}
              numbers={numbers}
            />
            <DimensionPanel
              title={strings.sitesAnalyticsEntryPages}
              note={strings.sitesAnalyticsEntryPagesNote}
              empty={strings.sitesAnalyticsPagesEmpty}
              rows={named(report.entryPages, pathLabel)}
              numbers={numbers}
            />
            <DimensionPanel
              title={strings.sitesAnalyticsExitPages}
              note={strings.sitesAnalyticsExitPagesNote}
              empty={strings.sitesAnalyticsPagesEmpty}
              rows={named(report.exitPages, pathLabel)}
              numbers={numbers}
            />
          </AnalyticsGroup>

          <AnalyticsGroup title={strings.sitesAnalyticsGroupReading}>
            <DimensionPanel
              title={strings.sitesAnalyticsReadTime}
              note={strings.sitesAnalyticsReadTimeNote}
              empty={strings.sitesAnalyticsReadTimeEmpty}
              rows={named(report.readTime, readTimeLabel)}
              ordered
              numbers={numbers}
            />
            <DimensionPanel
              title={strings.sitesAnalyticsOutbound}
              note={strings.sitesAnalyticsOutboundNote}
              empty={strings.sitesAnalyticsOutboundEmpty}
              rows={named(report.outboundDomains, outboundLabel)}
              numbers={numbers}
            />
            <DimensionPanel
              title={strings.sitesAnalyticsDevices}
              note={strings.sitesAnalyticsDevicesNote}
              empty={strings.sitesAnalyticsDevicesEmpty}
              rows={named(report.devices, deviceLabel)}
              numbers={numbers}
            />
          </AnalyticsGroup>
        </div>
      ) : null}
    </div>
  );
}

function PrivacyNote() {
  return (
    <aside className="flex gap-3 rounded-2xl border border-subtle bg-surface-raised p-4 shadow-sm sm:p-5">
      <span className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-success/10 text-success">
        <ShieldCheck size={20} aria-hidden="true" />
      </span>
      <div className="min-w-0">
        <strong className="text-sm font-semibold text-primary">
          {strings.sitesAnalyticsPrivacyTitle}
        </strong>
        <p className="mt-1 text-sm leading-5 text-secondary">
          {strings.sitesAnalyticsPrivacyBody}
        </p>
        <p className="mt-1 text-sm leading-5 text-secondary">
          {strings.sitesAnalyticsPrivacyBeacon}
        </p>
      </div>
    </aside>
  );
}
