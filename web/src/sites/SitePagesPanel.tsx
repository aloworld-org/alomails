import { useMemo, useState } from "react";
import { FileText, Search } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { Card, Table, TableEmpty, Th } from "../ds";
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
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());

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

  const treeRows = useMemo(() => {
    const pageIds = new Set(visiblePages.map((page) => page.id));
    const children = new Map<string, SitePage[]>();
    const roots: SitePage[] = [];

    for (const page of visiblePages) {
      if (page.parentId != null && pageIds.has(page.parentId)) {
        const siblings = children.get(page.parentId) ?? [];
        siblings.push(page);
        children.set(page.parentId, siblings);
      } else {
        roots.push(page);
      }
    }

    const rows: Array<{
      page: SitePage;
      depth: number;
      hasChildren: boolean;
    }> = [];
    const visited = new Set<string>();
    const visit = (page: SitePage, depth: number) => {
      if (visited.has(page.id)) return;
      visited.add(page.id);
      const childPages = children.get(page.id) ?? [];
      rows.push({ page, depth, hasChildren: childPages.length > 0 });
      if (!collapsed.has(page.id)) {
        for (const child of childPages) visit(child, depth + 1);
      } else {
        const markHidden = (child: SitePage) => {
          if (visited.has(child.id)) return;
          visited.add(child.id);
          for (const descendant of children.get(child.id) ?? []) {
            markHidden(descendant);
          }
        };
        for (const child of childPages) markHidden(child);
      }
    };

    for (const page of roots) visit(page, 0);
    for (const page of visiblePages) visit(page, 0);
    return rows;
  }, [collapsed, visiblePages]);

  function togglePage(pageId: string) {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(pageId)) next.delete(pageId);
      else next.add(pageId);
      return next;
    });
  }

  return (
    <Card as="section" pad="none">
      <div className="px-5 pb-3 pt-5 sm:px-6">
        <div>
          <h2 className="m-0 text-lg font-semibold tracking-tight text-text-primary">
            {strings.sitesPages}
          </h2>
          <p className="mt-1 text-sm text-text-secondary">
            {strings.sitesPageCount(pages.length)}
          </p>
        </div>
        {pages.length > 1 && (
          <div className="mt-5 flex flex-col gap-3 border-t border-subtle pt-5 lg:flex-row lg:items-center">
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
            <label>
              <span className="sr-only">{strings.sitesPageFilter}</span>
              <select
                aria-label={strings.sitesPageFilter}
                className="min-h-11 min-w-40 rounded-xl border border-default bg-surface px-3 text-sm font-medium text-text-primary outline-none transition-colors focus:border-accent focus:ring-2 focus:ring-accent-soft"
                value={filter}
                onChange={(event) =>
                  setFilter(event.target.value as "all" | "home" | "protected")
                }
              >
                <option value="all">{strings.sitesFilterAllPages}</option>
                <option value="home">{strings.sitesFilterHomePage}</option>
                <option value="protected">
                  {strings.sitesFilterProtectedPages}
                </option>
              </select>
            </label>
            <label>
              <span className="sr-only">{strings.sitesSortPages}</span>
              <select
                aria-label={strings.sitesSortPages}
                className="min-h-11 min-w-40 rounded-xl border border-default bg-surface px-3 text-sm font-medium text-text-primary outline-none transition-colors focus:border-accent focus:ring-2 focus:ring-accent-soft"
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
        )}
      </div>

      {pages.length === 0 && !loading ? (
        <>
          <EmptyState
            Icon={FileText}
            title={strings.sitesNoPagesTitle}
            body={strings.sitesNoPagesBody}
          />
          <div className="flex justify-center py-4">
            <CircularCreateButton
              label={strings.sitesNewPage}
              onClick={onCreate}
            />
          </div>
        </>
      ) : (
        <>
          <Table
            label={strings.sitesPages}
            density="compact"
            flat
            interactiveRows
            scrollable={false}
          >
            <thead className="bg-raised">
              <tr>
                <Th>{strings.sitesColPage}</Th>
                <Th>{strings.sitesColStatus}</Th>
                <Th>{strings.sitesColUpdated}</Th>
                <Th align="end" hideLabel>
                  {strings.sitesColActions}
                </Th>
              </tr>
            </thead>
            <tbody>
              {treeRows.map(({ page, depth, hasChildren }) => (
                <SitePageRow
                  key={page.id}
                  page={page}
                  depth={depth}
                  hasChildren={hasChildren}
                  expanded={!collapsed.has(page.id)}
                  protectedPage={protectedPages.has(page.id)}
                  siteStatus={siteStatus}
                  onToggle={() => togglePage(page.id)}
                  onOpen={(pageId) => navigate(`pages/${pageId}`)}
                  onRename={onRename}
                  onDuplicate={onDuplicate}
                  onSetHome={onSetHome}
                  onDelete={onDelete}
                />
              ))}
              {visiblePages.length === 0 && (
                <TableEmpty cols={4}>{strings.sitesNoMatchingPages}</TableEmpty>
              )}
            </tbody>
          </Table>
          <div className="flex justify-center py-4">
            <CircularCreateButton
              label={strings.sitesNewPage}
              onClick={onCreate}
            />
          </div>
        </>
      )}
    </Card>
  );
}
