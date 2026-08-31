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
    <header className="rounded-2xl border border-subtle bg-surface shadow-sm">
      <div className="flex flex-col gap-4 px-5 py-5 sm:px-6 lg:flex-row lg:items-center lg:justify-between">
        <div className="flex min-w-0 items-start gap-4">
          <span className="grid size-14 shrink-0 place-items-center rounded-2xl bg-accent-soft text-accent shadow-sm">
            {site?.theme?.favicon ? (
              <img
                className="size-8 object-contain"
                src={site.theme.favicon}
                alt=""
              />
            ) : (
              <Globe2 size={24} aria-hidden="true" />
            )}
          </span>
          <div className="min-w-0">
            <Link
              to=".."
              relative="path"
              className="-ml-2 inline-flex min-h-8 w-fit items-center gap-2 rounded-lg px-2 text-sm font-semibold text-accent no-underline transition-colors hover:bg-accent-soft"
            >
              <ArrowLeft size={16} aria-hidden="true" />
              {strings.sitesBack}
            </Link>
            {site !== null && (
              <div className="mt-1 flex min-w-0 flex-col gap-2">
                <div className="flex min-w-0 flex-wrap items-center gap-3">
                  <h1 className="m-0 truncate text-2xl font-bold tracking-tight text-text-primary sm:text-3xl">
                    {site.name}
                  </h1>
                  <SiteStatusChip status={site.status} />
                </div>
                <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1 text-sm text-text-secondary">
                  <span className="inline-flex min-w-0 items-center gap-1.5">
                    <Globe2 className="shrink-0" size={14} aria-hidden="true" />
                    <span className="truncate font-mono">
                      {host ?? site.subdomain}
                    </span>
                  </span>
                  <span>{strings.sitesDomainHealthy}</span>
                  <span>{lastPublishedLabel(publishedDate(site))}</span>
                </div>
              </div>
            )}
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2 lg:justify-end">
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
              className="inline-flex min-h-10 items-center justify-center gap-2 rounded-lg border border-default px-4 text-sm font-medium text-text-primary no-underline transition-colors hover:bg-surface-raised"
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
      </div>
    </header>
  );
}
