// One site's privacy-friendly traffic desk: immediate totals, a complete
// daily series, and — grouped so the screen stays readable — where people came
// from, what they looked at, and how they read it. There is no tracking setup
// or consent ceremony because collection stores no cookies or personal
// browsing profile.
import { useEffect, useMemo, useState } from "react";
import type { CSSProperties } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { ArrowLeft, BarChart3, ShieldCheck } from "lucide-react";

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
import type { SiteAnalyticsDimension, SiteAnalyticsReport, SiteDetail } from "./types";
import styles from "./SitesModule.module.css";

/** Names one dimension's buckets for reading, keeping the server's order —
 *  which is by count everywhere except the reading-time histogram. */
function named(
  rows: SiteAnalyticsDimension[] | undefined,
  name: (label: string) => string,
): AnalyticsRow[] {
  return (rows ?? []).map((row) => ({ label: name(row.label), visits: row.visits }));
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
        if (current) setSiteError(sitesMessage(error, strings.sitesSiteLoadFailed));
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
        if (current) setReportError(sitesMessage(error, strings.sitesAnalyticsLoadFailed));
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
  const maxVisits = report?.daily.reduce((max, row) => Math.max(max, row.visits), 0) ?? 0;
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
    <div className={styles.page}>
      <header className={`${styles.header} ${styles.analyticsHeader}`}>
        <Link className={styles.backLink} to={`/sites/${encodeURIComponent(siteId)}`}>
          <ArrowLeft size="var(--icon-size-inline)" />
          {strings.sitesBackToSite}
        </Link>
        <div className={styles.siteHead}>
          <h1 className={styles.title}>{strings.sitesAnalytics}</h1>
          {site !== null && <span className={styles.submissionSiteName}>{site.name}</span>}
        </div>
        <div className={styles.analyticsPeriods} aria-label={strings.sitesAnalyticsPeriod}>
          {PERIODS.map((days) => (
            <button
              type="button"
              key={days}
              className={`${styles.analyticsPeriod} ${
                period === days ? styles.analyticsPeriodActive : ""
              }`}
              aria-pressed={period === days}
              onClick={() => setPeriod(days)}
            >
              {strings.sitesAnalyticsDays(days)}
            </button>
          ))}
        </div>
      </header>

      {siteError !== null && <ErrorBanner message={siteError} />}
      {reportError !== null && <ErrorBanner message={reportError} />}

      {loadingSite || loadingReport ? (
        <div
          className={styles.analyticsSkeletons}
          role="status"
          aria-label={strings.sitesAnalyticsLoading}
        >
          <span />
          <span />
          <span />
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
        <main className={styles.analyticsContent}>
          <PrivacyNote />
          <section className={styles.analyticsSummary} aria-label={strings.sitesAnalyticsSummary}>
            <article className={styles.analyticsMetric}>
              <span>{strings.sitesAnalyticsVisits}</span>
              <strong>{numbers.format(report.totals.visits)}</strong>
            </article>
            <article className={styles.analyticsMetric}>
              <span>{strings.sitesAnalyticsVisitors}</span>
              <strong>{numbers.format(report.totals.uniqueVisitors)}</strong>
            </article>
          </section>

          <section className={styles.analyticsPanel}>
            <div className={styles.analyticsPanelHead}>
              <h2>{strings.sitesAnalyticsOverTime}</h2>
              <span>
                {dates.format(new Date(`${report.from}T00:00:00`))} –{" "}
                {dates.format(new Date(`${report.to}T00:00:00`))}
              </span>
            </div>
            <ol className={styles.analyticsChart} aria-label={strings.sitesAnalyticsChartLabel}>
              {report.daily.map((day) => (
                <li
                  key={day.date}
                  aria-label={strings.sitesAnalyticsDayLabel(
                    dates.format(new Date(`${day.date}T00:00:00`)),
                    day.visits,
                  )}
                >
                  <span
                    className={styles.analyticsBar}
                    style={
                      {
                        "--analytics-value": maxVisits === 0 ? 0 : day.visits / maxVisits,
                      } as CSSProperties
                    }
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
        </main>
      ) : null}
    </div>
  );
}

function PrivacyNote() {
  return (
    <aside className={styles.analyticsPrivacy}>
      <ShieldCheck size="var(--icon-size-control)" aria-hidden="true" />
      <div>
        <strong>{strings.sitesAnalyticsPrivacyTitle}</strong>
        <p>{strings.sitesAnalyticsPrivacyBody}</p>
        <p>{strings.sitesAnalyticsPrivacyBeacon}</p>
      </div>
    </aside>
  );
}
