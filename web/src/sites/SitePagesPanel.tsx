import { FileText, House, Lock, Palette } from "lucide-react";
import { Link, useNavigate } from "react-router-dom";

import { Button } from "../ds";
import { strings } from "../i18n";
import { EmptyState } from "./parts";
import type { SitePage } from "./types";

export function SitePagesPanel({
  pages,
  loading,
  protectedPages,
  onTheme,
  onCreate,
}: {
  pages: SitePage[];
  loading: boolean;
  protectedPages: Set<string>;
  onTheme: () => void;
  onCreate: () => void;
}) {
  const navigate = useNavigate();

  return (
    <section className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm">
      <div className="flex min-h-16 flex-wrap items-center justify-between gap-3 border-b border-subtle px-5 py-3 sm:px-6">
        <div>
          <h2 className="m-0 text-lg font-semibold text-text-primary">
            {strings.sitesPages}
          </h2>
          <p className="m-0 text-sm text-text-secondary">
            {strings.sitesPageCount(pages.length)}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            icon={<Palette size="var(--icon-size-inline)" />}
            onClick={onTheme}
          >
            {strings.sitesTheme}
          </Button>
          <Button size="sm" onClick={onCreate}>
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
          onCta={onCreate}
        />
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-left">
            <thead className="bg-surface-raised text-xs font-semibold uppercase tracking-wide text-text-secondary">
              <tr>
                <th className="px-5 py-3 sm:px-6" scope="col">
                  {strings.sitesColPage}
                </th>
                <th className="px-5 py-3 sm:px-6" scope="col">
                  {strings.sitesColPath}
                </th>
              </tr>
            </thead>
            <tbody>
              {pages.map((page) => (
                <tr
                  className="cursor-pointer border-t border-subtle transition-colors first:border-t-0 hover:bg-surface-raised focus-within:bg-surface-raised"
                  key={page.id}
                  onClick={() => navigate(`pages/${page.id}`)}
                >
                  <td className="px-5 py-4 sm:px-6">
                    <Link
                      to={`pages/${page.id}`}
                      className="font-semibold text-text-primary no-underline hover:text-accent focus-visible:rounded focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                    >
                      {page.title}
                    </Link>
                    {page.home && (
                      <span
                        className="ml-2 inline-flex size-6 items-center justify-center rounded-full bg-accent-soft text-accent"
                        title={strings.sitesHomeBadge}
                      >
                        <House size={12} aria-hidden="true" />
                        <span className="sr-only">{strings.sitesHomeBadge}</span>
                      </span>
                    )}
                    {protectedPages.has(page.id) && (
                      <span className="ml-2 inline-flex items-center gap-1 rounded-full bg-surface-raised px-2 py-0.5 text-xs font-medium text-text-secondary">
                        <Lock size={11} aria-hidden="true" />
                        {strings.sitesPagePasswordBadge}
                      </span>
                    )}
                  </td>
                  <td className="px-5 py-4 font-mono text-sm text-text-secondary sm:px-6">
                    /{page.slug}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
