// One site: its name, address and live/draft state, and the list of its
// pages in navigation order. This is the site's home surface — the section
// editor (S1.12), theme (S1.14) and publish action (S1.15) all mount here as
// they land. A stale or foreign id reads as "not found" with the way back,
// never a broken screen.
import { useCallback, useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { ArrowLeft, FileText, Palette } from "lucide-react";

import { strings } from "../i18n";
import { Button, Spinner } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import { NewPageDialog } from "./NewPageDialog";
import { ThemeDialog } from "./ThemeDialog";
import { EmptyState, ErrorBanner } from "./parts";
import type { SiteDetail, SitePage } from "./types";
import styles from "./SitesModule.module.css";

export function SiteView() {
  const { siteId = "" } = useParams();
  const api = useSitesApi();
  const [site, setSite] = useState<SiteDetail | null>(null);
  const [pages, setPages] = useState<SitePage[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [theming, setTheming] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [detail, pageList] = await Promise.all([api.site(siteId), api.pages(siteId)]);
      setSite(detail);
      setPages(pageList);
      setError(null);
    } catch (err) {
      setError(sitesMessage(err, strings.sitesSiteLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  const live = site?.status === "live";

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to=".." relative="path" className={styles.backLink}>
          <ArrowLeft size={16} aria-hidden="true" />
          {strings.sitesBack}
        </Link>
        {site !== null && (
          <div className={styles.siteHead}>
            <h1 className={styles.title}>{site.name}</h1>
            <span className={styles.mono}>{site.subdomain}</span>
            <span className={live ? `${styles.chip} ${styles.chipLive}` : styles.chip}>
              {live ? strings.sitesStatusLive : strings.sitesStatusDraft}
            </span>
          </div>
        )}
        {loading && <Spinner size={16} />}
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {site !== null && (
        <>
          <div className={styles.sectionBar}>
            <h2 className={styles.sectionTitle}>{strings.sitesPages}</h2>
            <div className={styles.sectionBarActions}>
              <Button
                variant="ghost"
                size="sm"
                icon={<Palette size={14} />}
                onClick={() => setTheming(true)}
              >
                {strings.sitesTheme}
              </Button>
              <Button size="sm" onClick={() => setCreating(true)}>
                {strings.sitesNewPage}
              </Button>
            </div>
          </div>

          {pages.length === 0 && !loading ? (
            <EmptyState
              Icon={FileText}
              title={strings.sitesNoPagesTitle}
              body={strings.sitesNoPagesBody}
              cta={strings.sitesNewPage}
              onCta={() => setCreating(true)}
            />
          ) : (
            <div className={styles.tableWrap}>
              <table className={styles.table}>
                <thead>
                  <tr>
                    <th scope="col">{strings.sitesColPage}</th>
                    <th scope="col">{strings.sitesColPath}</th>
                  </tr>
                </thead>
                <tbody>
                  {pages.map((p) => (
                    <tr key={p.id}>
                      <td>
                        {/* Opens the page's section editor. */}
                        <Link to={`pages/${p.id}`} className={styles.pageLink}>
                          {p.title}
                        </Link>
                        {p.home && <span className={styles.badge}>{strings.sitesHomeBadge}</span>}
                      </td>
                      <td className={styles.mono}>/{p.slug}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}

      {theming && site !== null && (
        <ThemeDialog
          siteId={site.id}
          onClose={() => setTheming(false)}
          onApplied={() => {
            setTheming(false);
            void load();
          }}
        />
      )}

      {creating && site !== null && (
        <NewPageDialog
          siteId={site.id}
          firstPage={pages.length === 0}
          onClose={() => setCreating(false)}
          onCreated={() => {
            setCreating(false);
            void load();
          }}
        />
      )}
    </div>
  );
}
