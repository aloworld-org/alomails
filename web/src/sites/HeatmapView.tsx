// One page's attention map (S2.09b): where visitors clicked, and how far down
// they read, aggregated per class of screen.
//
// The screen is deliberately narrow in what it claims. The store keeps counts
// per region of the page and nothing else — no cursor path, no session, no
// visitor — so this is a shape, never a rate: the twenty-click cap per page
// view and the browsers that report nothing both mean a click count and a
// visit count are not comparable, and the copy says so rather than leaving an
// owner to divide one by the other.
//
// Two guards live between the store and the eye. A screen class with fewer
// than `HEATMAP_MINIMUM_SAMPLE` events is not drawn at all, because a map of
// three clicks is a map of three people; and every coloured square is
// summarised in words beside it, so the finding survives without the colours.
import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { ArrowLeft, MousePointerClick, ShieldCheck } from "lucide-react";

import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { DimensionPanel } from "./AnalyticsPanels";
import { deviceLabel, pathLabel } from "./analyticsLabels";
import {
  HEATMAP_MINIMUM_SAMPLE,
  clickRegions,
  depthRows,
  tooFewToShow,
} from "./heatmapReading";
import { HeatmapOverlay } from "./HeatmapOverlay";
import { EmptyState, ErrorBanner } from "./parts";
import type {
  SiteDetail,
  SiteHeatmapPage,
  SiteHeatmapPathRow,
  SiteHeatmapViewport,
} from "./types";
import styles from "./SitesModule.module.css";

type HeatmapPeriod = 7 | 30 | 90;
const PERIODS: HeatmapPeriod[] = [7, 30, 90];

/** How busy one screen class is overall — what the default selection and the
 *  tab counts are ordered by. Clicks and depth reports together, exactly as
 *  the page menu counts them. */
function events(viewport: SiteHeatmapViewport): number {
  return viewport.clickTotal + viewport.scrollTotal;
}

/** The screen class to open on: the one with the most to show. Falling back to
 *  the first keeps a page with nothing at all on a real tab rather than none. */
function busiestViewport(page: SiteHeatmapPage | null): string | null {
  if (page === null || page.viewports.length === 0) return null;
  return page.viewports.reduce((best, viewport) =>
    events(viewport) > events(best) ? viewport : best,
  ).viewport;
}

export function HeatmapView() {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [site, setSite] = useState<SiteDetail | null>(null);
  const [domain, setDomain] = useState<string | null>(null);
  const [period, setPeriod] = useState<HeatmapPeriod>(30);
  const [paths, setPaths] = useState<SiteHeatmapPathRow[]>([]);
  const [page, setPage] = useState<SiteHeatmapPage | null>(null);
  // Null means "not chosen yet": the busiest page and screen class are used
  // until an owner picks, and a picked one survives a period change.
  const [chosenPath, setChosenPath] = useState<string | null>(null);
  const [chosenViewport, setChosenViewport] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loadingMenu, setLoadingMenu] = useState(true);
  const [loadingPage, setLoadingPage] = useState(false);

  const path = chosenPath ?? paths[0]?.path ?? null;

  useEffect(() => {
    let current = true;
    void Promise.all([api.site(siteId), api.config().catch(() => null)])
      .then(([detail, config]) => {
        if (!current) return;
        setSite(detail);
        setDomain(config?.domain ?? null);
      })
      .catch((error: unknown) => {
        if (current) setError(sitesMessage(error, strings.sitesSiteLoadFailed));
      });
    return () => {
      current = false;
    };
  }, [api, siteId]);

  // The menu of pages that have data, asked without a path so it is answered
  // before anything is chosen.
  useEffect(() => {
    let current = true;
    setLoadingMenu(true);
    void api
      .heatmap(siteId, period)
      .then((report) => {
        if (!current) return;
        setPaths(report.paths);
        setError(null);
      })
      .catch((error: unknown) => {
        if (current) setError(sitesMessage(error, strings.sitesHeatmapLoadFailed));
      })
      .finally(() => {
        if (current) setLoadingMenu(false);
      });
    return () => {
      current = false;
    };
  }, [api, period, siteId]);

  useEffect(() => {
    if (path === null) {
      setPage(null);
      return;
    }
    let current = true;
    setLoadingPage(true);
    void api
      .heatmap(siteId, period, path)
      .then((report) => {
        if (!current) return;
        setPage(report.page);
        setError(null);
      })
      .catch((error: unknown) => {
        if (current) setError(sitesMessage(error, strings.sitesHeatmapLoadFailed));
      })
      .finally(() => {
        if (current) setLoadingPage(false);
      });
    return () => {
      current = false;
    };
  }, [api, path, period, siteId]);

  const numbers = useMemo(() => new Intl.NumberFormat(), []);
  const viewportName = chosenViewport ?? busiestViewport(page);
  const viewport =
    page?.viewports.find((entry) => entry.viewport === viewportName) ?? null;
  const columns = page?.grid.columns ?? 0;
  const rows = page?.grid.rows ?? 0;
  const regions = useMemo(
    () => (viewport === null ? [] : clickRegions(viewport.clicks, columns, rows)),
    [viewport, columns, rows],
  );
  const depth = useMemo(
    () => (viewport === null ? [] : depthRows(viewport.scrollDepth)),
    [viewport],
  );
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

  function choosePath(next: string) {
    setChosenPath(next);
    // A screen class chosen for one page says nothing about the next one.
    setChosenViewport(null);
  }

  return (
    <div className={styles.page}>
      <header className={`${styles.header} ${styles.analyticsHeader}`}>
        <Link
          className={styles.backLink}
          to={`/sites/${encodeURIComponent(siteId)}/analytics`}
        >
          <ArrowLeft size="var(--icon-size-inline)" />
          {strings.sitesBackToAnalytics}
        </Link>
        <div className={styles.siteHead}>
          <h1 className={styles.title}>{strings.sitesHeatmap}</h1>
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

      {error !== null && <ErrorBanner message={error} />}

      {loadingMenu ? (
        <div
          className={styles.analyticsSkeletons}
          role="status"
          aria-label={strings.sitesHeatmapLoading}
        >
          <span />
          <span />
        </div>
      ) : paths.length === 0 ? (
        <>
          <PrivacyNote />
          <EmptyState
            Icon={MousePointerClick}
            title={strings.sitesHeatmapEmptyTitle}
            body={strings.sitesHeatmapEmptyBody}
            cta={
              liveAddress !== null ? strings.sitesAnalyticsOpenSite : strings.sitesOpenPages
            }
            onCta={openSite}
          />
        </>
      ) : (
        <div className={styles.analyticsContent}>
          <PrivacyNote />

          <div className={styles.heatmapToolbar}>
            <label className={styles.heatmapPageField}>
              <span>{strings.sitesHeatmapPage}</span>
              <select
                className={styles.input}
                value={path ?? ""}
                onChange={(event) => choosePath(event.target.value)}
              >
                {paths.map((row) => (
                  <option key={row.path} value={row.path}>
                    {strings.sitesHeatmapPageOption(pathLabel(row.path), row.events)}
                  </option>
                ))}
              </select>
            </label>
            {page !== null && (
              <div className={styles.analyticsPeriods} aria-label={strings.sitesHeatmapScreens}>
                {page.viewports.map((entry) => (
                  <button
                    type="button"
                    key={entry.viewport}
                    className={`${styles.analyticsPeriod} ${
                      entry.viewport === viewportName ? styles.analyticsPeriodActive : ""
                    }`}
                    aria-pressed={entry.viewport === viewportName}
                    onClick={() => setChosenViewport(entry.viewport)}
                  >
                    {strings.sitesHeatmapScreenTab(
                      deviceLabel(entry.viewport),
                      numbers.format(events(entry)),
                    )}
                  </button>
                ))}
              </div>
            )}
          </div>

          {loadingPage ? (
            <div
              className={styles.analyticsSkeletons}
              role="status"
              aria-label={strings.sitesHeatmapLoading}
            >
              <span />
              <span />
            </div>
          ) : viewport === null || page === null ? null : (
            <div className={styles.heatmapLayout}>
              <section className={styles.analyticsPanel}>
                <div className={styles.analyticsPanelHead}>
                  <h2>{strings.sitesHeatmapClicks}</h2>
                </div>
                <p className={styles.analyticsNote}>{strings.sitesHeatmapClicksNote}</p>
                {tooFewToShow(viewport.clickTotal) ? (
                  <div className={styles.analyticsPanelEmpty}>
                    <strong>
                      {viewport.clickTotal === 0
                        ? strings.sitesHeatmapClicksEmpty
                        : strings.sitesHeatmapTooFewTitle}
                    </strong>
                    {viewport.clickTotal > 0 && (
                      <p>
                        {strings.sitesHeatmapTooFewClicks(
                          viewport.clickTotal,
                          HEATMAP_MINIMUM_SAMPLE,
                        )}
                      </p>
                    )}
                  </div>
                ) : (
                  <HeatmapOverlay
                    cells={viewport.clicks}
                    columns={page.grid.columns}
                    rows={page.grid.rows}
                    label={strings.sitesHeatmapClicksLabel(
                      pathLabel(page.path),
                      deviceLabel(viewport.viewport),
                      viewport.clickTotal,
                    )}
                  />
                )}
              </section>

              <DimensionPanel
                title={strings.sitesHeatmapSpots}
                note={strings.sitesHeatmapSpotsNote}
                empty={
                  viewport.clickTotal === 0
                    ? strings.sitesHeatmapSpotsEmpty
                    : strings.sitesHeatmapSpotsHeldBack
                }
                rows={tooFewToShow(viewport.clickTotal) ? [] : regions}
                numbers={numbers}
              />

              <DimensionPanel
                title={strings.sitesHeatmapDepth}
                note={strings.sitesHeatmapDepthNote}
                empty={
                  viewport.scrollTotal === 0
                    ? strings.sitesHeatmapDepthEmpty
                    : strings.sitesHeatmapTooFewDepth(
                        viewport.scrollTotal,
                        HEATMAP_MINIMUM_SAMPLE,
                      )
                }
                rows={tooFewToShow(viewport.scrollTotal) ? [] : depth}
                ordered
                numbers={numbers}
              />
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function PrivacyNote() {
  return (
    <aside className={styles.analyticsPrivacy}>
      <ShieldCheck size="var(--icon-size-control)" aria-hidden="true" />
      <div>
        <strong>{strings.sitesHeatmapPrivacyTitle}</strong>
        <p>{strings.sitesHeatmapPrivacyBody}</p>
        <p>{strings.sitesHeatmapPrivacyShape}</p>
      </div>
    </aside>
  );
}
