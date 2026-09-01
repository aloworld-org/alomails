import { ArrowLeft, ExternalLink, Globe2, Palette } from "lucide-react";
import { Link } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { SiteStatusChip } from "./SiteStatusChip";
import type { SiteDetail } from "./types";

function publishedDate(site: SiteDetail): string {
  if (site.publish === null) return strings.sitesNotPublishedYet;
  try {
    return new Intl.DateTimeFormat(undefined, {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(site.publish.publishedAt));
  } catch {
    return site.publish.publishedAt;
  }
}

function lastPublishedLabel(date: string): string {
  return typeof strings.sitesLastPublished === "function"
    ? strings.sitesLastPublished(date)
    : `Last published: ${date}`;
}

export function SiteWorkspaceHeader({
  site,
  host,
  loading,
  publishBusy,
  confirmingOffline,
  onTheme,
  onPublish,
  onUnpublish,
}: {
  site: SiteDetail | null;
  host: string | null;
  loading: boolean;
  publishBusy: boolean;
  confirmingOffline: boolean;
  onTheme: () => void;
  onPublish: () => void;
  onUnpublish: () => void;
}) {
  const live = site?.status === "live";

  return (
    <header className="flex min-w-0 flex-col gap-3 py-3 font-ui lg:flex-row lg:items-center lg:justify-between">
      <div className="flex min-w-0 items-center gap-3">
        <Link
          to=".."
          relative="path"
          className="inline-flex min-h-10 shrink-0 items-center gap-2 rounded-xl px-2 text-sm font-medium text-secondary no-underline transition-colors hover:bg-raised hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30 sm:px-3"
          aria-label={strings.sitesBack}
          title={strings.sitesBack}
        >
          <ArrowLeft size={18} aria-hidden="true" />
          <span className="hidden sm:inline">{strings.sitesBack}</span>
        </Link>
        <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
          {site?.theme?.favicon ? (
            <img
              className="size-6 object-contain"
              src={site.theme.favicon}
              alt=""
            />
          ) : (
            <Globe2 size={20} aria-hidden="true" />
          )}
        </span>
        {site !== null && (
          <div className="min-w-0">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <h1 className="m-0 truncate text-xl font-bold tracking-tight text-primary sm:text-2xl">
                {site.name}
              </h1>
              <SiteStatusChip status={site.status} />
            </div>
            <div className="mt-1 flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-xs text-secondary">
              <span className="truncate font-medium">
                {host ?? site.subdomain}
              </span>
              <span aria-hidden="true">·</span>
              <span>{lastPublishedLabel(publishedDate(site))}</span>
            </div>
          </div>
        )}
      </div>
      <div className="flex flex-wrap items-center justify-end gap-2 lg:shrink-0">
        {loading && <Spinner size={16} />}
        <Button
          variant="ghost"
          size="sm"
          icon={<Palette size="var(--icon-size-inline)" />}
          onClick={onTheme}
        >
          {strings.sitesTheme}
        </Button>
        {host !== null && (
          <a
            href={`https://${host}`}
            target="_blank"
            rel="noreferrer"
            className="inline-flex min-h-10 items-center justify-center gap-2 rounded-lg border border-default px-4 text-sm font-medium text-primary no-underline transition-colors hover:bg-raised"
          >
            <ExternalLink size={16} aria-hidden="true" />
            {strings.sitesViewSite}
          </a>
        )}
        {live && (
          <Button
            variant={confirmingOffline ? "danger" : "ghost"}
            size="sm"
            disabled={publishBusy}
            onClick={onUnpublish}
          >
            {confirmingOffline
              ? strings.sitesConfirmUnpublish
              : strings.sitesUnpublish}
          </Button>
        )}
        <Button size="sm" disabled={publishBusy} onClick={onPublish}>
          {live ? strings.sitesPublishChanges : strings.sitesPublish}
        </Button>
      </div>
    </header>
  );
}
