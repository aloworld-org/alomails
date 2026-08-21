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
import { EmptyState, ErrorBanner } from "./parts";
import type { Site, SitePage } from "./types";

/** The live/draft state chip of a site row. */
function StatusChip({ status }: { status: Site["status"] }) {
  const live = status === "live";
  return (
    <span
      className={
        live
          ? "inline-flex items-center gap-1.5 rounded-full bg-success-tint px-2.5 py-1 text-xs font-medium text-success"
          : "inline-flex items-center gap-1.5 rounded-full bg-raised px-2.5 py-1 text-xs font-medium text-secondary"
      }
    >
      <span
        className={live ? "size-1.5 rounded-full bg-success" : "size-1.5 rounded-full bg-tertiary"}
        aria-hidden="true"
      />
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
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto bg-app px-5 py-6 sm:px-8 lg:px-10">
      <header className="mx-auto flex w-full max-w-screen-xl items-center justify-between gap-6">
        <div>
          <h1 className="m-0 text-3xl font-semibold tracking-tight text-primary">
            {strings.moduleSites}
          </h1>
          <p className="mt-1 text-sm text-secondary">{strings.sitesNoSitesBody}</p>
        </div>
        <Button icon={<Plus />} onClick={() => setCreating(true)}>
          {strings.sitesNewSite}
        </Button>
      </header>

      <div className="mx-auto mt-6 w-full max-w-screen-xl">
        {error !== null && <ErrorBanner message={error} />}

        {loading ? (
          <div className="flex min-h-72 items-center justify-center rounded-2xl border border-default bg-surface">
            <Spinner />
          </div>
        ) : sites.length === 0 ? (
          <div className="rounded-2xl border border-default bg-surface shadow-sm">
            <EmptyState
              Icon={Globe2}
              title={strings.sitesNoSitesTitle}
              body={strings.sitesNoSitesBody}
              cta={strings.sitesNewSite}
              onCta={() => setCreating(true)}
            />
          </div>
        ) : (
          <section className="overflow-hidden rounded-2xl border border-default bg-surface shadow-sm">
            <div className="border-b border-subtle px-5 py-4 sm:px-6">
              <h2 className="text-base font-semibold text-primary">{strings.moduleSites}</h2>
            </div>
            <div className="divide-y divide-subtle">
              {sites.map((site) => (
                <button
                  key={site.id}
                  type="button"
                  className="group flex min-h-20 w-full items-center gap-4 px-5 py-4 text-left transition-colors hover:bg-raised focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent sm:px-6"
                  onClick={() => void navigate(site.id)}
                >
                  <span className="inline-flex size-11 shrink-0 items-center justify-center rounded-xl bg-accent-soft text-accent">
                    <Globe2 size={21} aria-hidden="true" />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-semibold text-primary">{site.name}</span>
                    <span className="mt-1 block truncate font-mono text-sm text-secondary">
                      {site.subdomain}
                    </span>
                  </span>
                  <StatusChip status={site.status} />
                  <ArrowRight
                    className="size-5 shrink-0 text-tertiary transition-transform group-hover:translate-x-0.5 group-hover:text-primary"
                    aria-hidden="true"
                  />
                </button>
              ))}
            </div>
          </section>
        )}
      </div>

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
