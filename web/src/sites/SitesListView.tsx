// The site list: every website the tenant has, and the only place one is
// created. A row opens the site's own page (pages now, the editor from
// S1.12); the address and the live/draft state are shown because they are
// what a returning owner checks first.
import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Globe } from "lucide-react";

import { strings } from "../i18n";
import { Button, Spinner } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import { NewSiteDialog } from "./NewSiteDialog";
import { EmptyState, ErrorBanner } from "./parts";
import type { Site, SitePage } from "./types";
import styles from "./SitesModule.module.css";

/** The live/draft state chip of a site row. */
function StatusChip({ status }: { status: Site["status"] }) {
  const live = status === "live";
  return (
    <span className={live ? `${styles.chip} ${styles.chipLive}` : styles.chip}>
      {live ? strings.sitesStatusLive : strings.sitesStatusDraft}
    </span>
  );
}

export function SitesListView() {
  const api = useSitesApi();
  const navigate = useNavigate();
  const [sites, setSites] = useState<Site[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setSites(await api.sites());
      setError(null);
    } catch (err) {
      setError(sitesMessage(err, strings.sitesLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <h1 className={styles.title}>{strings.moduleSites}</h1>
        {loading && <Spinner size={16} />}
        <div className={styles.headerActions}>
          <Button onClick={() => setCreating(true)}>{strings.sitesNewSite}</Button>
        </div>
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {sites.length === 0 && !loading ? (
        <EmptyState
          Icon={Globe}
          title={strings.sitesNoSitesTitle}
          body={strings.sitesNoSitesBody}
          cta={strings.sitesNewSite}
          onCta={() => setCreating(true)}
        />
      ) : (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.sitesColName}</th>
                <th scope="col">{strings.sitesColAddress}</th>
                <th scope="col">{strings.sitesColStatus}</th>
              </tr>
            </thead>
            <tbody>
              {sites.map((s) => (
                <tr key={s.id}>
                  <td>
                    <button
                      type="button"
                      className={styles.rowName}
                      onClick={() => void navigate(s.id)}
                    >
                      {s.name}
                    </button>
                  </td>
                  <td className={styles.mono}>{s.subdomain}</td>
                  <td>
                    <StatusChip status={s.status} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {creating && (
        <NewSiteDialog
          onClose={() => setCreating(false)}
          onCreated={(site: Site, page: SitePage) => {
            setCreating(false);
            void navigate(`${site.id}/pages/${page.id}`);
          }}
        />
      )}
    </div>
  );
}
