import {
  Copy,
  Edit3,
  Eye,
  FileText,
  Home,
  House,
  Lock,
  MoreHorizontal,
  SearchCheck,
  Trash2,
} from "lucide-react";
import { Link } from "react-router-dom";

import { Button } from "../ds";
import { strings } from "../i18n";
import type { SitePage } from "./types";

function pathFor(page: SitePage): string {
  return page.home ? "/" : `/${page.slug}`;
}

function seoReady(page: SitePage): boolean {
  return (
    (page.seoTitle?.trim() ?? "") !== "" &&
    (page.seoDescription?.trim() ?? "") !== ""
  );
}

function editedLabel(page: SitePage): string | null {
  if (page.updatedAt === undefined) return null;
  try {
    return strings.sitesLastEdited(
      new Intl.DateTimeFormat(undefined, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(new Date(page.updatedAt)),
    );
  } catch {
    return strings.sitesLastEdited(page.updatedAt);
  }
}

export function SitePageRow({
  page,
  protectedPage,
  siteStatus,
  enabledLocales,
  onOpen,
  onRename,
  onDuplicate,
  onSetHome,
  onDelete,
}: {
  page: SitePage;
  protectedPage: boolean;
  siteStatus: "draft" | "live";
  enabledLocales: string[];
  onOpen: (pageId: string) => void;
  onRename: (page: SitePage) => void;
  onDuplicate: (page: SitePage) => void;
  onSetHome: (page: SitePage) => void;
  onDelete: (page: SitePage) => void;
}) {
  const ready = seoReady(page);
  const statusLabel =
    siteStatus === "live" ? strings.sitesStatusPublished : strings.sitesStatusDraft;
  const updated = editedLabel(page);

  return (
    <tr
      className="group cursor-pointer border-t border-subtle transition-colors first:border-t-0 hover:bg-surface-raised focus-within:bg-surface-raised"
      onClick={() => onOpen(page.id)}
    >
      <td className="px-5 py-4 sm:px-6">
        <div className="flex min-w-0 items-center gap-3">
          <span
            className="grid size-10 shrink-0 place-items-center rounded-lg bg-accent-soft text-accent"
            aria-hidden="true"
          >
            <FileText size={18} />
          </span>
          <span className="min-w-0">
            <span className="flex min-w-0 flex-wrap items-center gap-2">
              <Link
                to={`pages/${page.id}`}
                className="truncate font-semibold text-text-primary no-underline hover:text-accent focus-visible:rounded focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                onClick={(event) => event.stopPropagation()}
              >
                {page.title}
              </Link>
              {page.home && (
                <span
                  className="inline-flex size-6 items-center justify-center rounded-full bg-accent-soft text-accent"
                  title={strings.sitesHomeBadge}
                >
                  <House size={12} aria-hidden="true" />
                  <span className="sr-only">{strings.sitesHomeBadge}</span>
                </span>
              )}
            </span>
            <span className="mt-1 block font-mono text-sm text-text-secondary">
              {pathFor(page)}
            </span>
            {updated !== null && (
              <span className="mt-1 block text-xs font-medium text-text-tertiary">
                {updated}
              </span>
            )}
            <span className="mt-2 flex flex-wrap items-center gap-1.5">
              <span
                className={`rounded-full px-2.5 py-1 text-xs font-semibold ${
                  siteStatus === "live"
                    ? "bg-success-tint text-success"
                    : "bg-surface-raised text-text-secondary"
                }`}
              >
                {statusLabel}
              </span>
              {enabledLocales.map((locale) => (
                <span
                  key={locale}
                  className="rounded-full bg-surface-raised px-2 py-0.5 font-mono text-[0.6875rem] font-semibold text-text-secondary"
                >
                  {locale.toUpperCase()}
                </span>
              ))}
            </span>
          </span>
        </div>
      </td>
      <td className="hidden px-5 py-4 sm:table-cell sm:px-6">
        <span
          className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-semibold ${
            ready
              ? "bg-success-tint text-success"
              : "bg-surface-raised text-text-secondary"
          }`}
        >
          <SearchCheck size={12} aria-hidden="true" />
          {ready ? strings.sitesSeoReady : strings.sitesSeoNeedsWork}
        </span>
      </td>
      <td className="hidden px-5 py-4 lg:table-cell sm:px-6">
        {protectedPage ? (
          <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-raised px-2.5 py-1 text-xs font-semibold text-text-secondary">
            <Lock size={12} aria-hidden="true" />
            {strings.sitesPagePasswordBadge}
          </span>
        ) : (
          <span className="text-sm text-text-tertiary">{strings.sitesPublicPage}</span>
        )}
      </td>
      <td className="px-5 py-4 sm:px-6">
        <div className="flex justify-end gap-2 opacity-100 sm:opacity-0 sm:transition-opacity sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
          <Button
            variant="ghost"
            size="sm"
            icon={<Edit3 size="var(--icon-size-inline)" />}
            onClick={(event) => {
              event.stopPropagation();
              onOpen(page.id);
            }}
          >
            {strings.sitesEditPage}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            icon={<Eye size="var(--icon-size-inline)" />}
            onClick={(event) => {
              event.stopPropagation();
              onOpen(page.id);
            }}
          >
            {strings.sitesPreview}
          </Button>
          <details
            className="relative"
            onClick={(event) => event.stopPropagation()}
          >
            <summary
              className="grid size-10 cursor-pointer list-none place-items-center rounded-lg border border-default text-text-secondary transition-colors marker:hidden hover:bg-surface hover:text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent [&::-webkit-details-marker]:hidden"
              aria-label={strings.sitesPageActions}
              title={strings.sitesPageActions}
            >
              <MoreHorizontal size={16} aria-hidden="true" />
            </summary>
            <div className="absolute right-0 z-20 mt-2 grid min-w-48 overflow-hidden rounded-xl border border-subtle bg-surface p-1 shadow-lg">
              <button
                type="button"
                className="flex min-h-10 items-center gap-2 rounded-lg px-3 text-left text-sm font-medium text-text-primary hover:bg-surface-raised"
                onClick={() => onRename(page)}
              >
                <Edit3 size={15} aria-hidden="true" />
                {strings.sitesRenamePage}
              </button>
              <button
                type="button"
                className="flex min-h-10 items-center gap-2 rounded-lg px-3 text-left text-sm font-medium text-text-primary hover:bg-surface-raised"
                onClick={() => onDuplicate(page)}
              >
                <Copy size={15} aria-hidden="true" />
                {strings.sitesDuplicatePage}
              </button>
              {!page.home && (
                <button
                  type="button"
                  className="flex min-h-10 items-center gap-2 rounded-lg px-3 text-left text-sm font-medium text-text-primary hover:bg-surface-raised"
                  onClick={() => onSetHome(page)}
                >
                  <Home size={15} aria-hidden="true" />
                  {strings.sitesSetHomepage}
                </button>
              )}
              <button
                type="button"
                className="flex min-h-10 items-center gap-2 rounded-lg px-3 text-left text-sm font-medium text-danger hover:bg-danger-tint"
                onClick={() => onDelete(page)}
              >
                <Trash2 size={15} aria-hidden="true" />
                {strings.sitesDeletePage}
              </button>
            </div>
          </details>
        </div>
      </td>
    </tr>
  );
}
