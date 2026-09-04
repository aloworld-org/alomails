import {
  ChevronDown,
  ChevronRight,
  Copy,
  Edit3,
  FileText,
  Home,
  Lock,
  MoreHorizontal,
  Trash2,
} from "lucide-react";
import { Link } from "react-router-dom";

import { Badge, Menu, type MenuItem } from "../ds";
import { strings } from "../i18n";
import type { SitePage } from "./types";

function editedDate(page: SitePage): string | null {
  if (page.updatedAt === undefined) return null;
  try {
    return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(
      new Date(page.updatedAt),
    );
  } catch {
    return page.updatedAt;
  }
}

export function SitePageRow({
  page,
  depth,
  hasChildren,
  expanded,
  protectedPage,
  siteStatus,
  onToggle,
  onOpen,
  onRename,
  onDuplicate,
  onSetHome,
  onDelete,
}: {
  page: SitePage;
  depth: number;
  hasChildren: boolean;
  expanded: boolean;
  protectedPage: boolean;
  siteStatus: "draft" | "live";
  onToggle: () => void;
  onOpen: (pageId: string) => void;
  onRename: (page: SitePage) => void;
  onDuplicate: (page: SitePage) => void;
  onSetHome: (page: SitePage) => void;
  onDelete: (page: SitePage) => void;
}) {
  const updated = editedDate(page);
  const depthClass = ["pl-0", "pl-6", "pl-12", "pl-16", "pl-20"][
    Math.min(depth, 4)
  ];
  const actions: MenuItem[] = [
    {
      key: "rename",
      label: strings.sitesRenamePage,
      icon: <Edit3 aria-hidden="true" />,
      onClick: () => onRename(page),
    },
    {
      key: "duplicate",
      label: strings.sitesDuplicatePage,
      icon: <Copy aria-hidden="true" />,
      onClick: () => onDuplicate(page),
    },
  ];
  if (!page.home) {
    actions.push({
      key: "home",
      label: strings.sitesSetHomepage,
      icon: <Home aria-hidden="true" />,
      onClick: () => onSetHome(page),
    });
  }
  actions.push({
    key: "delete",
    label: strings.sitesDeletePage,
    icon: <Trash2 aria-hidden="true" />,
    onClick: () => onDelete(page),
    danger: true,
    divider: true,
  });

  return (
    <tr
      aria-level={depth + 1}
      aria-expanded={hasChildren ? expanded : undefined}
      className="cursor-pointer"
      onClick={() => onOpen(page.id)}
    >
      <td>
        <div className={`flex min-w-0 items-center gap-2 ${depthClass}`}>
          {hasChildren && (
            <button
              type="button"
              className="grid size-7 shrink-0 place-items-center rounded-md text-tertiary hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent"
              aria-label={
                expanded
                  ? strings.sitesCollapseChildPages
                  : strings.sitesExpandChildPages
              }
              aria-expanded={expanded}
              onClick={(event) => {
                event.stopPropagation();
                onToggle();
              }}
            >
              {expanded ? (
                <ChevronDown size={15} aria-hidden="true" />
              ) : (
                <ChevronRight size={15} aria-hidden="true" />
              )}
            </button>
          )}
          <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent ring-1 ring-inset ring-accent/10">
            {page.home ? (
              <Home className="size-4" aria-hidden="true" />
            ) : (
              <FileText className="size-4" aria-hidden="true" />
            )}
          </span>
          <div className="flex min-w-0 flex-col">
            <div className="flex min-w-0 items-center gap-1.5">
              <Link
                to={`pages/${page.id}`}
                className="block truncate font-semibold text-primary no-underline hover:text-accent focus-visible:rounded focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                onClick={(event) => event.stopPropagation()}
              >
                {page.title}
              </Link>
              {protectedPage && (
                <span
                  className="shrink-0 text-tertiary"
                  title={strings.sitesPagePasswordBadge}
                >
                  <Lock size={12} aria-hidden="true" />
                  <span className="sr-only">
                    {strings.sitesPagePasswordBadge}
                  </span>
                </span>
              )}
            </div>
          </div>
        </div>
      </td>
      <td className="w-36">
        <Badge tone={siteStatus === "live" ? "success" : "neutral"}>
          <span
            className="mr-1.5 size-1.5 rounded-full bg-current"
            aria-hidden="true"
          />
          {siteStatus === "live"
            ? strings.sitesStatusPublished
            : strings.sitesStatusDraft}
        </Badge>
      </td>
      <td className="w-44 whitespace-nowrap text-secondary">{updated}</td>
      <td className="w-14 text-right">
        <div
          className="inline-flex"
          onClick={(event) => event.stopPropagation()}
        >
          <Menu
            label={strings.sitesPageActions}
            icon={<MoreHorizontal aria-hidden="true" />}
            items={actions}
            size="comfortable"
          />
        </div>
      </td>
    </tr>
  );
}
