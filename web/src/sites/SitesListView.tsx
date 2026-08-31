// The site list: every website the tenant has, and the only place one is
// created. A row opens the site's own page (pages now, the editor from
// S1.12); the address and the live/draft state are shown because they are
// what a returning owner checks first.
import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ArrowRight, Globe2, Plus } from "lucide-react";

import { strings } from "../i18n";
import { Button, Spinner } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import { NewSiteDialog } from "./NewSiteDialog";
import { SiteStatusChip } from "./SiteStatusChip";
import { EmptyState, ErrorBanner } from "./parts";
import type { Site, SitePage } from "./types";

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
    <div className="flex min-h-0 flex-1 flex-col bg-app">
      <header className="shrink-0 border-b border-subtle bg-surface px-8 py-6 max-sm:px-4 max-sm:py-4">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex min-w-0 items-center gap-3.5">
            <span
              className="flex size-12 shrink-0 items-center justify-center rounded-2xl bg-accent-soft text-accent shadow-sm ring-1 ring-inset ring-accent/10"
              aria-hidden="true"
            >
              <Globe2 className="size-5" />
            </span>
            <div className="min-w-0">
              <h1
                id="sites-heading"
                className="m-0 text-2xl font-bold tracking-tight text-primary"
              >
                {strings.moduleSites}
              </h1>
              <p className="m-0 mt-1 text-sm text-secondary">
                {strings.sitesNoSitesBody}
              </p>
            </div>
          </div>
          {(sites.length > 0 || error !== null) && (
            <Button
              className="max-sm:w-full"
              icon={<Plus />}
              onClick={() => setCreating(true)}
            >
              {strings.sitesNewSite}
            </Button>
          )}
        </div>
      </header>

      <section
        className="flex min-h-0 w-full flex-1 flex-col overflow-y-auto px-8 py-6 max-sm:px-4 max-sm:py-4"
        aria-labelledby="sites-heading"
      >
        {error !== null && <ErrorBanner message={error} />}

        {loading ? (
          <div className="flex min-h-80 flex-1 items-center justify-center rounded-2xl border border-default bg-surface shadow-sm">
            <Spinner />
          </div>
        ) : sites.length === 0 ? (
          <section
            className="flex min-h-80 flex-1 rounded-2xl border border-default bg-surface shadow-sm"
            aria-label={strings.sitesNoSitesTitle}
          >
            <EmptyState
              Icon={Globe2}
              title={strings.sitesNoSitesTitle}
              body={strings.sitesNoSitesBody}
              cta={strings.sitesNewSite}
              onCta={() => setCreating(true)}
            />
          </section>
        ) : (
          <section className="overflow-hidden rounded-2xl border border-default bg-surface shadow-sm">
            <div className="border-b border-subtle px-5 py-4 sm:px-6">
              <h2 className="text-base font-semibold text-primary">
                {strings.moduleSites}
              </h2>
            </div>
            <div className="divide-y divide-subtle">
              {sites.map((site) => (
                <button
                  key={site.id}
                  type="button"
                  className="group flex min-h-20 w-full items-center gap-4 px-6 py-5 text-left transition-colors hover:bg-raised focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent sm:px-7"
                  onClick={() => void navigate(site.id)}
                >
                  <span className="inline-flex size-10 shrink-0 items-center justify-center rounded-xl bg-accent-soft text-accent ring-1 ring-inset ring-accent/10">
                    <Globe2 className="size-5" aria-hidden="true" />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-semibold text-primary">
                      {site.name}
                    </span>
                    <span className="mt-1 block truncate font-mono text-sm text-secondary">
                      {site.subdomain}
                    </span>
                  </span>
                  <SiteStatusChip status={site.status} />
                  <span className="inline-flex size-9 shrink-0 items-center justify-center rounded-lg text-tertiary transition-colors group-hover:bg-surface group-hover:text-primary">
                    <ArrowRight
                      className="size-4 transition-transform group-hover:translate-x-0.5"
                      aria-hidden="true"
                    />
                  </span>
                </button>
              ))}
            </div>
          </section>
        )}
      </section>

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
