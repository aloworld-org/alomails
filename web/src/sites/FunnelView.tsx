// What one website is actually worth: the arc from a page view to an invoice,
// per contact form and for the site (S2.10c, over the seam built in S2.10b).
//
// Its domain reference is the funnel report of a marketing suite — HubSpot,
// Squarespace Analytics — and the way it beats them is by refusing to round
// off the parts of the arithmetic that are not clean:
//
//   * The first two steps are reported by the visitor's browser and the rest
//     are rows somebody created, so every rate across that line is a floor and
//     the screen says which side each number comes from.
//   * The site figures are not the sum of the columns — one invoice reachable
//     from two forms counts once for the site and once under each — so the
//     table is a per-form reading, never a set of addends.
//   * Two currencies are two lines and no total, because a forecast has no
//     issue date to convert at. That is CRM's rule, inherited deliberately.
//   * What "invoices" counts is a stated rule (`invoiceRule`), shown as a
//     sentence rather than implied to be revenue this page generated.
//
// The screen is also the one place in Sites that reads CRM and Billing, so it
// carries their absence: a site editor, a colleague CRM is switched off for,
// and a colleague Billing is switched off for each get a different, complete
// answer instead of a broken page.
import { useEffect, useMemo, useState } from "react";
import type { CSSProperties } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { ArrowLeft, BarChart3, Inbox, Lock, Sprout } from "lucide-react";

import { strings } from "../i18n";
import { SitesError, sitesMessage, useSitesApi } from "./api";
import { funnelMoney, funnelStages, sourceIsQuiet, sourceLabel } from "./funnelReading";
import type { FunnelStage } from "./funnelReading";
import { EmptyState, ErrorBanner } from "./parts";
import type { SiteAttributionReport, SiteAttributionSource, SiteDetail } from "./types";
import styles from "./SitesModule.module.css";

type FunnelPeriod = 7 | 30 | 90;
const PERIODS: FunnelPeriod[] = [7, 30, 90];

export function FunnelView() {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [site, setSite] = useState<SiteDetail | null>(null);
  const [period, setPeriod] = useState<FunnelPeriod>(30);
  const [report, setReport] = useState<SiteAttributionReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  // A refusal is not a failure: the reader may simply not be allowed to see
  // the business behind this website. It gets its own state so the screen can
  // explain itself in the server's own words instead of showing a red banner.
  const [denial, setDenial] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let current = true;
    api.site(siteId).then(
      (detail) => {
        if (current) setSite(detail);
      },
      () => {
        // The funnel is readable without the site's name; the heading simply
        // stays as the page title.
      },
    );
    return () => {
      current = false;
    };
  }, [api, siteId]);

  useEffect(() => {
    let current = true;
    setLoading(true);
    void api
      .attribution(siteId, period)
      .then((answer) => {
        if (!current) return;
        setReport(answer);
        setError(null);
        setDenial(null);
      })
      .catch((err: unknown) => {
        if (!current) return;
        setReport(null);
        if (err instanceof SitesError && err.status === 403) {
          setDenial(err.detail ?? strings.sitesFunnelDeniedFallback);
          setError(null);
        } else {
          setDenial(null);
          setError(sitesMessage(err, strings.sitesFunnelLoadFailed));
        }
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [api, period, siteId]);

  const numbers = useMemo(() => new Intl.NumberFormat(), []);
  const dates = useMemo(
    () => new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" }),
    [],
  );
  const stages = report === null ? [] : funnelStages(report.totals);
  // The quiet forms last, but never dropped: "nobody has reached this form"
  // is a finding, and a list that hides it hides the reason a number is low.
  const sources = useMemo(
    () =>
      report === null
        ? []
        : [...report.sources].sort(
            (left, right) => Number(sourceIsQuiet(left)) - Number(sourceIsQuiet(right)),
          ),
    [report],
  );

  return (
    <div className={styles.page}>
      <header className={`${styles.header} ${styles.analyticsHeader}`}>
        <Link className={styles.backLink} to={`/sites/${encodeURIComponent(siteId)}`}>
          <ArrowLeft size="var(--icon-size-inline)" />
          {strings.sitesBackToSite}
        </Link>
        <div className={styles.siteHead}>
          <h1 className={styles.title}>{strings.sitesFunnel}</h1>
          {site !== null && <span className={styles.submissionSiteName}>{site.name}</span>}
        </div>
        <Link
          className={styles.analyticsDrill}
          to={`/sites/${encodeURIComponent(siteId)}/analytics`}
        >
          <BarChart3 size="var(--icon-size-inline)" aria-hidden="true" />
          {strings.sitesAnalytics}
        </Link>
        <div className={styles.analyticsPeriods} aria-label={strings.sitesFunnelPeriod}>
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

      {error !== null && <ErrorBanner message={error} />}

      {denial !== null ? (
        <section className={styles.funnelDenied} aria-label={strings.sitesFunnel}>
          <span className={styles.emptyArt} aria-hidden="true">
            <Lock size={38} />
          </span>
          <h2 className={styles.emptyTitle}>{strings.sitesFunnelDeniedTitle}</h2>
          {/* The server's own sentence, verbatim: it names which rule refused
              this reader, which a generic veil cannot. */}
          <p className={styles.emptyBody}>{denial}</p>
          <p className={styles.funnelDeniedWay}>{strings.sitesFunnelDeniedWay}</p>
        </section>
      ) : loading ? (
        <div
          className={styles.analyticsSkeletons}
          role="status"
          aria-label={strings.sitesFunnelLoading}
        >
          <span />
          <span />
          <span />
        </div>
      ) : report !== null && report.sources.length === 0 ? (
        <EmptyState
          Icon={Sprout}
          title={strings.sitesFunnelNoSourcesTitle}
          body={strings.sitesFunnelNoSourcesBody}
          cta={strings.sitesOpenPages}
          onCta={() => navigate(`/sites/${encodeURIComponent(siteId)}`)}
        />
      ) : report !== null ? (
        <div className={styles.analyticsContent}>
          <section className={styles.analyticsPanel} aria-label={strings.sitesFunnelChain}>
            <div className={styles.analyticsPanelHead}>
              <h2>{strings.sitesFunnelChain}</h2>
              <span>
                {dates.format(new Date(`${report.from}T00:00:00`))} –{" "}
                {dates.format(new Date(`${report.to}T00:00:00`))}
              </span>
            </div>
            <ol className={styles.funnelStages}>
              {stages.map((stage) => (
                <StageRow key={stage.key} stage={stage} numbers={numbers} />
              ))}
            </ol>
            <p className={styles.analyticsNote}>{strings.sitesFunnelFloorNote}</p>
          </section>

          <section className={styles.analyticsPanel} aria-label={strings.sitesFunnelMoney}>
            <div className={styles.analyticsPanelHead}>
              <h2>{strings.sitesFunnelMoney}</h2>
              <span>{strings.sitesFunnelInvoiceRule}</span>
            </div>
            {report.totals.money.length === 0 ? (
              <p className={styles.analyticsPanelEmpty}>{strings.sitesFunnelMoneyEmpty}</p>
            ) : (
              <ul className={styles.funnelMoney}>
                {report.totals.money.map((line) => (
                  <li key={line.currency}>
                    <span className={styles.funnelMoneyCurrency}>{line.currency}</span>
                    <span>
                      <em>{strings.sitesFunnelOpen}</em>
                      <strong>{funnelMoney(line.openCents, line.currency)}</strong>
                    </span>
                    <span>
                      <em>{strings.sitesFunnelWon}</em>
                      <strong>{funnelMoney(line.wonCents, line.currency)}</strong>
                    </span>
                    <span>
                      <em>{strings.sitesFunnelInvoiced}</em>
                      <strong>
                        {line.invoicedCents === null
                          ? strings.sitesFunnelHidden
                          : funnelMoney(line.invoicedCents, line.currency)}
                      </strong>
                    </span>
                  </li>
                ))}
              </ul>
            )}
            {!report.billingVisible && (
              <p className={styles.analyticsNote}>{strings.sitesFunnelBillingOff}</p>
            )}
            {report.totals.money.length > 1 && (
              <p className={styles.analyticsNote}>{strings.sitesFunnelCurrencies}</p>
            )}
          </section>

          <section className={styles.analyticsPanel} aria-label={strings.sitesFunnelSources}>
            <div className={styles.analyticsPanelHead}>
              <h2>{strings.sitesFunnelSources}</h2>
              <Link
                className={styles.analyticsDrill}
                to={`/sites/${encodeURIComponent(siteId)}/submissions`}
              >
                <Inbox size="var(--icon-size-inline)" aria-hidden="true" />
                {strings.sitesSubmissions}
              </Link>
            </div>
            <div className={styles.tableWrap}>
              <table className={styles.table}>
                <thead>
                  <tr>
                    <th scope="col">{strings.sitesFunnelColSource}</th>
                    <th scope="col">{strings.sitesFunnelStageViews}</th>
                    <th scope="col">{strings.sitesFunnelStageStarts}</th>
                    <th scope="col">{strings.sitesFunnelStageSubmits}</th>
                    <th scope="col">{strings.sitesFunnelStageLeads}</th>
                    <th scope="col">{strings.sitesFunnelColDeals}</th>
                    <th scope="col">{strings.sitesFunnelStageInvoices}</th>
                  </tr>
                </thead>
                <tbody>
                  {sources.map((source) => (
                    <SourceRow key={`${source.kind}:${source.id}`} source={source} numbers={numbers} />
                  ))}
                </tbody>
              </table>
            </div>
            <p className={styles.analyticsNote}>{strings.sitesFunnelSumNote}</p>
          </section>
        </div>
      ) : null}
    </div>
  );
}

/** One step of the chain: the count, a bar against the largest step, and where
 *  the number came from. */
function StageRow({ stage, numbers }: { stage: FunnelStage; numbers: Intl.NumberFormat }) {
  return (
    <li className={styles.funnelStage}>
      <span className={styles.funnelStageLabel}>{stage.label}</span>
      <span
        className={styles.funnelStageBar}
        style={{ "--funnel-share": stage.share } as CSSProperties}
        aria-hidden="true"
      />
      <strong className={styles.funnelStageCount}>{numbers.format(stage.count)}</strong>
      <span className={styles.funnelEvidence}>
        {stage.evidence === "browser"
          ? strings.sitesFunnelFromBrowser
          : strings.sitesFunnelFromRecord}
      </span>
    </li>
  );
}

/** One conversion point, read across. The three deal states share a cell —
 *  they are one fact about the same opportunities, and three columns of mostly
 *  zeroes reads as noise. */
function SourceRow({
  source,
  numbers,
}: {
  source: SiteAttributionSource;
  numbers: Intl.NumberFormat;
}) {
  return (
    <tr>
      <td>
        <span className={source.name === null ? styles.funnelSourceGone : ""}>
          {sourceLabel(source)}
        </span>
      </td>
      <td>{numbers.format(source.views)}</td>
      <td>{numbers.format(source.starts)}</td>
      <td>{numbers.format(source.submits)}</td>
      <td>{numbers.format(source.leads)}</td>
      <td className={styles.funnelDeals}>
        {strings.sitesFunnelDealsSummary(source.dealsOpen, source.dealsWon, source.dealsLost)}
      </td>
      <td>
        {source.invoices === null ? strings.sitesFunnelHidden : numbers.format(source.invoices)}
      </td>
    </tr>
  );
}
