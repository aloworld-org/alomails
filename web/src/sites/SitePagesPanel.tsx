import { useMemo, useState } from "react";
import { FileText, Palette, Search } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { Button } from "../ds";
import { strings } from "../i18n";
import { EmptyState } from "./parts";
import { CircularCreateButton } from "./CircularCreateButton";
import { SitePageRow } from "./SitePageRow";
import type { SitePage } from "./types";

export function SitePagesPanel({
  pages,
  loading,
  protectedPages,
  siteStatus,
  enabledLocales,
  onTheme,
  onCreate,
  onRename,
  onDuplicate,
  onSetHome,
  onDelete,
}: {
  pages: SitePage[];
  loading: boolean;
  protectedPages: Set<string>;
  siteStatus: "draft" | "live";
  enabledLocales: string[];
  onTheme: () => void;
  onCreate: () => void;
  onRename: (page: SitePage) => void;
  onDuplicate: (page: SitePage) => void;
  onSetHome: (page: SitePage) => void;
  onDelete: (page: SitePage) => void;
}) {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<"all" | "home" | "protected">("all");
  const [sort, setSort] = useState<"order" | "name" | "path">("order");

  const visiblePages = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const filtered = pages.filter((page) => {
      const matchesQuery =
        needle === "" ||
        page.title.toLowerCase().includes(needle) ||
        `/${page.slug}`.toLowerCase().includes(needle);
      const matchesFilter =
        filter === "all" ||
        (filter === "home" && page.home) ||
        (filter === "protected" && protectedPages.has(page.id));
      return matchesQuery && matchesFilter;
    });
    if (sort === "name") {
      return [...filtered].sort((a, b) => a.title.localeCompare(b.title));
    }
    if (sort === "path") {
      return [...filtered].sort((a, b) => a.slug.localeCompare(b.slug));
    }
    return [...filtered].sort((a, b) => (a.navOrder ?? 0) - (b.navOrder ?? 0));
  }, [filter, pages, protectedPages, query, sort]);

  return (
    <section className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm">
      <div className="flex flex-col gap-4 border-b border-subtle px-5 py-4 sm:px-6">
        <div className="flex min-h-12 flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="m-0 text-lg font-semibold text-text-primary">
              {strings.sitesPages}
            </h2>
            <p className="m-0 text-sm text-text-secondary">
              {strings.sitesPageCount(pages.length)}
            </p>
          </div>
          <Button
            variant="ghost"
            size="sm"
            icon={<Palette size="var(--icon-size-inline)" />}
            onClick={onTheme}
          >
            {strings.sitesTheme}
          </Button>
        </div>
        <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
          <label className="relative min-w-0 flex-1">
            <span className="sr-only">{strings.sitesSearchPages}</span>
            <Search
              className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-text-tertiary"
              aria-hidden="true"
            />
            <input
              className="min-h-11 w-full rounded-xl border border-default bg-surface py-2 pl-10 pr-3 text-sm font-medium text-text-primary outline-none transition-colors placeholder:text-text-tertiary focus:border-accent focus:ring-2 focus:ring-accent-soft"
              value={query}
              placeholder={strings.sitesSearchPages}
              onChange={(event) => setQuery(event.target.value)}
            />
          </label>
          <div
            className="inline-flex w-fit rounded-xl bg-surface-raised p-1"
            role="group"
            aria-label={strings.sitesPageFilter}
          >
            {(["all", "home", "protected"] as const).map((value) => (
              <button
                key={value}
                type="button"
                className={`min-h-9 rounded-lg px-3 text-sm font-semibold transition-colors ${
                  filter === value
                    ? "bg-surface text-accent shadow-sm"
                    : "text-text-secondary hover:text-text-primary"
                }`}
                aria-pressed={filter === value}
                onClick={() => setFilter(value)}
              >
                {value === "all"
                  ? strings.sitesFilterAllPages
                  : value === "home"
                    ? strings.sitesFilterHomePage
                    : strings.sitesFilterProtectedPages}
              </button>
            ))}
          </div>
          <label className="flex min-w-44 items-center gap-2 text-sm font-semibold text-text-secondary">
            <span>{strings.sitesSortPages}</span>
            <select
              className="min-h-10 rounded-xl border border-default bg-surface px-3 text-sm font-medium text-text-primary outline-none transition-colors focus:border-accent focus:ring-2 focus:ring-accent-soft"
              value={sort}
              onChange={(event) =>
                setSort(event.target.value as "order" | "name" | "path")
              }
            >
              <option value="order">{strings.sitesSortNavigation}</option>
              <option value="name">{strings.sitesSortName}</option>
              <option value="path">{strings.sitesSortPath}</option>
            </select>
          </label>
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
        <>
          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-left">
              <thead className="bg-surface-raised text-xs font-semibold uppercase tracking-wide text-text-secondary">
                <tr>
                  <th className="px-5 py-3 sm:px-6" scope="col">
                    {strings.sitesColPage}
                  </th>
                  <th className="hidden px-5 py-3 sm:table-cell sm:px-6" scope="col">
                    {strings.sitesColSeo}
                  </th>
                  <th className="hidden px-5 py-3 lg:table-cell sm:px-6" scope="col">
                    {strings.sitesColAccess}
                  </th>
                  <th className="px-5 py-3 text-right sm:px-6" scope="col">
                    {strings.sitesColActions}
                  </th>
                </tr>
              </thead>
              <tbody>
                {visiblePages.map((page) => (
                  <SitePageRow
                    key={page.id}
                    page={page}
                    protectedPage={protectedPages.has(page.id)}
                    siteStatus={siteStatus}
                    enabledLocales={enabledLocales}
                    onOpen={(pageId) => navigate(`pages/${pageId}`)}
                    onRename={onRename}
                    onDuplicate={onDuplicate}
                    onSetHome={onSetHome}
                    onDelete={onDelete}
                  />
                ))}
              </tbody>
            </table>
          </div>
          {visiblePages.length === 0 && (
            <div className="border-t border-subtle px-5 py-8 text-center text-sm font-medium text-text-secondary sm:px-6">
              {strings.sitesNoMatchingPages}
            </div>
          )}
          <div className="flex justify-center border-t border-subtle py-4">
            <CircularCreateButton
              label={strings.sitesNewPage}
              onClick={onCreate}
            />
          </div>
        </>
      )}
    </section>
  );
}
