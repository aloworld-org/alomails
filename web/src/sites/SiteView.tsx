// One site: its name, address and live/draft state, the publish switch, and
// the list of its pages in navigation order. This is the site's home surface
// — the section editor (S1.12), theme (S1.14) and publish (S1.15) all mount
// here. A stale or foreign id reads as "not found" with the way back, never
// a broken screen.
import { useCallback, useEffect, useState } from "react";
import {
  useNavigate,
  useParams,
  useSearchParams,
} from "react-router-dom";
import {
  BarChart3,
  Bot,
  CalendarClock,
  FileText,
  Globe2,
  ArrowRight,
  Check,
  Handshake,
  History,
  Inbox,
  Languages,
  Newspaper,
  Package,
  Receipt,
  ShoppingBag,
  Sparkles,
  Rows3,
  Ticket,
  X,
} from "lucide-react";

import { RecordAgentPanel } from "../agents";
import { strings } from "../i18n";
import { Button, Spinner, useDialogs } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import { NewPageDialog } from "./NewPageDialog";
import { SchedulePublish } from "./SchedulePublish";
import { SiteCollaborators } from "./SiteCollaborators";
import { SiteOverviewPanel } from "./SiteOverviewPanel";
import { SitePagesPanel } from "./SitePagesPanel";
import {
  SiteSectionNavigation,
  type SiteWorkspace,
} from "./SiteSectionNavigation";
import { SiteWorkspaceHeader } from "./SiteWorkspaceHeader";
import { ThemeDialog } from "./ThemeDialog";
import { ErrorBanner } from "./parts";
import type {
  SiteDetail,
  SitePage,
  SiteTranslationEnvelope,
  SiteTranslationReadiness,
} from "./types";

export function SiteView() {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const api = useSitesApi();
  const dialogs = useDialogs();
  const [site, setSite] = useState<SiteDetail | null>(null);
  const [pages, setPages] = useState<SitePage[]>([]);
  // The pages a visitor has to know a password to open (S2.06b), read in one
  // call so the list can mark them without a request per row.
  const [protectedPages, setProtectedPages] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [theming, setTheming] = useState(false);
  // The deployment-wide sites domain; null while unknown (loading, or the
  // config fetch failed) — the copy that needs it simply stays off.
  const [domain, setDomain] = useState<string | null>(null);
  const [publishBusy, setPublishBusy] = useState(false);
  const [publishError, setPublishError] = useState<string | null>(null);
  const [readiness, setReadiness] = useState<SiteTranslationReadiness | null>(
    null,
  );
  const [languageInput, setLanguageInput] = useState("");
  const [languageBusy, setLanguageBusy] = useState(false);
  const [languageError, setLanguageError] = useState<string | null>(null);
  const [translationBusy, setTranslationBusy] = useState(false);
  const [translationError, setTranslationError] = useState<string | null>(null);
  const [translationProposal, setTranslationProposal] =
    useState<SiteTranslationEnvelope | null>(null);
  // Taking a live site off the air asks for a second click, like deleting a
  // section does: the first click arms, the second one acts.
  const [confirmingOffline, setConfirmingOffline] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [detail, pageList, translationReadiness, protections] =
        await Promise.all([
          api.site(siteId),
          api.pages(siteId),
          api.translationReadiness(siteId),
          api.protectedPages(siteId),
        ]);
      setSite(detail);
      setPages(pageList);
      setReadiness(translationReadiness);
      setProtectedPages(
        new Set(
          protections
            .filter((protection) => protection.protected)
            .flatMap((protection) =>
              protection.pageId === null ? [] : [protection.pageId],
            ),
        ),
      );
      setError(null);
    } catch (err) {
      setError(sitesMessage(err, strings.sitesSiteLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    let cancelled = false;
    api.config().then(
      (c) => {
        if (!cancelled && typeof c.domain === "string" && c.domain !== "")
          setDomain(c.domain);
      },
      () => {
        // Domain unknown: publishing still works, the address copy stays off.
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api]);

  const live = site?.status === "live";
  const host =
    site !== null && domain !== null ? `${site.subdomain}.${domain}` : null;
  const missingTranslations =
    readiness?.languages.reduce(
      (count, language) =>
        count + (readiness.totalPages - language.translatedPages),
      0,
    ) ?? 0;
  const firstIncompleteLocale = readiness?.languages.find(
    (language) => !language.ready,
  )?.locale;
  const firstPageId = pages[0]?.id;
  const requestedWorkspace = searchParams.get("section");
  const workspace: SiteWorkspace =
    requestedWorkspace === "overview" ||
    requestedWorkspace === "pages" ||
    requestedWorkspace === "publishing" ||
    requestedWorkspace === "languages" ||
    requestedWorkspace === "tools" ||
    (requestedWorkspace === "collaborators" &&
      site?.canManageCollaborators)
      ? requestedWorkspace
      : "overview";

  function selectWorkspace(nextWorkspace: SiteWorkspace) {
    const next = new URLSearchParams(searchParams);
    if (nextWorkspace === "overview") next.delete("section");
    else next.set("section", nextWorkspace);
    setSearchParams(next);
  }

  function languageName(locale: string): string {
    try {
      return (
        new Intl.DisplayNames(undefined, { type: "language" }).of(locale) ??
        locale.toUpperCase()
      );
    } catch {
      return locale.toUpperCase();
    }
  }

  async function saveLanguages(
    defaultLocale: string,
    enabledLocales: string[],
  ) {
    setLanguageBusy(true);
    setLanguageError(null);
    try {
      await api.setSiteLocales(siteId, defaultLocale, enabledLocales);
      setLanguageInput("");
      await load();
    } catch (err) {
      setLanguageError(sitesMessage(err, strings.sitesLanguageSaveFailed));
    } finally {
      setLanguageBusy(false);
    }
  }

  function addLanguage() {
    if (site === null || languageInput.trim() === "") return;
    void saveLanguages(site.defaultLocale, [
      ...site.enabledLocales,
      languageInput.trim(),
    ]);
  }

  function removeLanguage(locale: string) {
    if (site === null || locale === site.defaultLocale) return;
    void saveLanguages(
      site.defaultLocale,
      site.enabledLocales.filter((enabled) => enabled !== locale),
    );
  }

  async function prepareTranslation(targetLocale: string) {
    if (site === null) return;
    setTranslationBusy(true);
    setTranslationError(null);
    setTranslationProposal(null);
    try {
      setTranslationProposal(
        await api.proposeSiteTranslation(
          siteId,
          site.defaultLocale,
          targetLocale,
        ),
      );
    } catch (err) {
      setTranslationError(
        sitesMessage(err, strings.sitesWholeTranslationPrepareFailed),
      );
    } finally {
      setTranslationBusy(false);
    }
  }

  async function approveTranslation() {
    if (translationProposal === null) return;
    setTranslationBusy(true);
    setTranslationError(null);
    try {
      await api.applySiteTranslation(siteId, translationProposal);
      setTranslationProposal(null);
      await load();
    } catch (err) {
      setTranslationError(
        sitesMessage(err, strings.sitesWholeTranslationApplyFailed),
      );
    } finally {
      setTranslationBusy(false);
    }
  }

  async function publish() {
    setPublishBusy(true);
    setPublishError(null);
    try {
      await api.publishSite(siteId);
      await load();
    } catch (err) {
      setPublishError(sitesMessage(err, strings.sitesPublishFailed));
    } finally {
      setPublishBusy(false);
      setConfirmingOffline(false);
    }
  }

  async function unpublish() {
    if (!confirmingOffline) {
      setConfirmingOffline(true);
      return;
    }
    setPublishBusy(true);
    setPublishError(null);
    try {
      await api.unpublishSite(siteId);
      await load();
    } catch (err) {
      setPublishError(sitesMessage(err, strings.sitesUnpublishFailed));
    } finally {
      setPublishBusy(false);
      setConfirmingOffline(false);
    }
  }

  async function renamePage(page: SitePage) {
    const title = await dialogs.prompt({
      title: strings.sitesRenamePage,
      message: strings.sitesRenamePagePrompt,
      defaultValue: page.title,
      confirmLabel: strings.save,
    });
    const nextTitle = title?.trim();
    if (nextTitle === undefined || nextTitle === "" || nextTitle === page.title) return;
    try {
      await api.setPageIdentity(siteId, page.id, nextTitle, page.slug);
      await load();
    } catch (err) {
      await dialogs.alert({
        message: sitesMessage(err, strings.sitesPageActionFailed),
      });
    }
  }

  async function duplicatePage(page: SitePage) {
    try {
      await api.duplicatePage(siteId, page.id);
      await load();
    } catch (err) {
      await dialogs.alert({
        message: sitesMessage(err, strings.sitesPageActionFailed),
      });
    }
  }

  async function setHomePage(page: SitePage) {
    if (
      !(await dialogs.confirm({
        title: strings.sitesSetHomepage,
        message: strings.sitesSetHomepageConfirm(page.title),
        confirmLabel: strings.sitesSetHomepage,
      }))
    ) {
      return;
    }
    try {
      await api.setHomePage(siteId, page.id);
      await load();
    } catch (err) {
      await dialogs.alert({
        message: sitesMessage(err, strings.sitesPageActionFailed),
      });
    }
  }

  async function deletePage(page: SitePage) {
    if (
      !(await dialogs.confirm({
        title: strings.sitesDeletePage,
        message: strings.sitesDeletePageConfirm(page.title),
        confirmLabel: strings.sitesDeletePage,
        danger: true,
      }))
    ) {
      return;
    }
    try {
      await api.deletePage(siteId, page.id);
      await load();
    } catch (err) {
      await dialogs.alert({
        message: sitesMessage(err, strings.sitesPageActionFailed),
      });
    }
  }

  return (
    <div className="mx-auto flex w-full max-w-[80rem] flex-col gap-5 px-4 py-5 sm:px-6 lg:px-8 lg:py-7">
      <SiteWorkspaceHeader
        site={site}
        host={host}
        loading={loading}
        publishBusy={publishBusy}
        confirmingOffline={confirmingOffline}
        onTheme={() => setTheming(true)}
        onPublish={() => void publish()}
        onUnpublish={() => void unpublish()}
      />

      {/* Everything below the header scrolls as one document: this screen is
          a stack of panels, not a viewport column, and on a phone the pages
          table lives below the fold. */}
      <div className="flex flex-col gap-5">
        {error !== null && <ErrorBanner message={error} />}

        {site !== null && (
          <>
            <SiteSectionNavigation
              active={workspace}
              showCollaborators={site.canManageCollaborators}
              onSelect={selectWorkspace}
            />

            {workspace === "overview" && (
              <SiteOverviewPanel
                site={site}
                pages={pages}
                host={host}
                readiness={readiness}
                onNavigate={(target) => navigate(target)}
              />
            )}

            {workspace === "publishing" && (
              <section className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm">
              <div className="flex flex-col gap-4 px-5 py-5 sm:px-6 lg:flex-row lg:items-center lg:justify-between">
                <div className="flex min-w-0 items-start gap-3">
                  <span
                    className={`mt-0.5 grid size-10 shrink-0 place-items-center rounded-xl ${live ? "bg-success-tint text-success" : "bg-accent-soft text-accent"}`}
                    aria-hidden="true"
                  >
                    {live ? <Check size={20} /> : <Globe2 size={20} />}
                  </span>
                  <div className="flex min-w-0 flex-col gap-1 text-sm text-text-secondary">
                    <strong className="text-base text-text-primary">
                      {live
                        ? strings.sitesStatusLive
                        : strings.sitesStatusDraft}
                    </strong>
                    {live && host !== null && (
                      <>
                        <span>{strings.sitesLiveAtLabel}</span>
                        <a
                          href={`https://${host}`}
                          target="_blank"
                          rel="noreferrer"
                          className="w-fit font-semibold text-text-primary no-underline hover:text-accent"
                        >
                          {host}
                        </a>
                      </>
                    )}
                    {!live && host !== null && (
                      <span>{strings.sitesGoesLiveAt(host)}</span>
                    )}
                    <span className="mt-1 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-text-tertiary">
                      <span className="inline-flex items-center gap-1.5">
                        <FileText size={14} aria-hidden="true" />
                        {strings.sitesPageCount(pages.length)}
                      </span>
                      {readiness !== null && readiness.totalPages > 0 && (
                        <span className="inline-flex items-center gap-1.5">
                          <Languages size={14} aria-hidden="true" />
                          {missingTranslations === 0
                            ? strings.sitesTranslationAllReady
                            : strings.sitesTranslationPublishHint(
                                missingTranslations,
                              )}
                        </span>
                      )}
                    </span>
                    {publishError !== null && (
                      <span className="font-medium text-danger" role="alert">
                        {publishError}
                      </span>
                    )}
                  </div>
                </div>
                <div className="flex flex-wrap items-center gap-2 lg:justify-end">
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={<Globe2 size="var(--icon-size-inline)" />}
                    aria-label={strings.sitesDomains}
                    title={strings.sitesDomains}
                    onClick={() => navigate("domains")}
                  />
                  {/* History belongs beside Publish: it is the question "what did
                  the last publish look like, and can I have it back?". */}
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={<History size="var(--icon-size-inline)" />}
                    aria-label={strings.sitesHistory}
                    title={strings.sitesHistory}
                    onClick={() => navigate("history")}
                  />
                </div>
              </div>
              </section>
            )}

            {workspace === "pages" && (
              <SitePagesPanel
                pages={pages}
                loading={loading}
                protectedPages={protectedPages}
                siteStatus={site.status}
                enabledLocales={site.enabledLocales}
                onTheme={() => setTheming(true)}
                onCreate={() => setCreating(true)}
                onRename={(page) => void renamePage(page)}
                onDuplicate={(page) => void duplicatePage(page)}
                onSetHome={(page) => void setHomePage(page)}
                onDelete={(page) => void deletePage(page)}
              />
            )}

            {/* Publishing later belongs directly under publishing now: they are
              the same decision, one of them with a moment attached. */}
            {workspace === "publishing" && (
              <SchedulePublish
                siteId={site.id}
                onPublished={() => void load()}
              />
            )}

            {workspace === "collaborators" &&
              site.canManageCollaborators && (
              <SiteCollaborators siteId={site.id} />
            )}

            {workspace === "languages" && (
              <section
                className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm"
                aria-labelledby="site-languages-title"
              >
              <div className="flex min-h-16 items-center gap-3 px-5 py-3 sm:px-6">
                <span
                  className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent"
                  aria-hidden="true"
                >
                  <Languages size={20} />
                </span>
                <span className="min-w-0 flex-1">
                  <h2
                    id="site-languages-title"
                    className="m-0 block font-semibold text-text-primary"
                  >
                    {strings.sitesLanguages}
                  </h2>
                  <span className="block truncate text-sm text-text-secondary">
                    {strings.sitesLanguagesHint}
                  </span>
                </span>
                <span className="text-sm font-medium text-text-secondary">
                  {site.enabledLocales.length}
                </span>
              </div>
              <section
                className="flex flex-col gap-5 border-t border-subtle px-5 py-5 sm:px-6"
                aria-label={strings.sitesLanguagesHint}
              >
                <div className="overflow-hidden rounded-xl border border-subtle bg-surface">
                  {readiness?.languages.map((language) => (
                    <div
                      className="flex min-h-14 flex-wrap items-center gap-x-3 gap-y-2 border-t border-subtle px-4 py-3 first:border-t-0 hover:bg-surface-raised"
                      key={language.locale}
                    >
                      <span className="min-w-10 font-mono text-sm font-semibold text-text-primary">
                        {language.locale.toUpperCase()}
                      </span>
                      <span className="min-w-0 flex-1 text-sm font-medium text-text-primary sm:min-w-40">
                        {languageName(language.locale)}
                      </span>
                      {language.locale === site.defaultLocale && (
                        <span className="inline-flex rounded-full bg-surface-raised px-2.5 py-1 text-xs font-medium text-text-secondary">
                          {strings.sitesLanguageDefaultBadge}
                        </span>
                      )}
                      <span
                        className={`inline-flex rounded-full px-2.5 py-1 text-xs font-semibold ${
                          language.ready
                            ? "bg-success-tint text-success"
                            : "bg-surface-raised text-warning"
                        }`}
                      >
                        {language.ready
                          ? strings.sitesTranslationReady
                          : strings.sitesTranslationProgress(
                              language.translatedPages,
                              readiness.totalPages,
                            )}
                      </span>
                      {language.locale !== site.defaultLocale && (
                        <span className="flex w-full flex-wrap items-center justify-end gap-2 sm:ml-auto sm:w-auto">
                          <Button
                            variant="ghost"
                            size="sm"
                            icon={<Sparkles size="var(--icon-size-inline)" />}
                            disabled={languageBusy || translationBusy}
                            onClick={() =>
                              void prepareTranslation(language.locale)
                            }
                          >
                            {strings.sitesTranslateWholeSite}
                          </Button>
                          <Button
                            variant="ghost"
                            size="sm"
                            icon={<X size="var(--icon-size-inline)" />}
                            disabled={languageBusy || translationBusy}
                            onClick={() => removeLanguage(language.locale)}
                          >
                            {strings.sitesRemoveLanguage(
                              languageName(language.locale),
                            )}
                          </Button>
                        </span>
                      )}
                    </div>
                  ))}
                </div>

                <div className="grid gap-4 rounded-xl bg-surface-raised p-4 lg:grid-cols-2">
                  <label className="flex min-w-0 flex-col gap-1.5 text-xs font-semibold text-text-secondary">
                    <span>{strings.sitesDefaultLanguage}</span>
                    <select
                      className="min-h-11 w-full rounded-xl border border-default bg-surface px-3 text-sm font-medium text-text-primary outline-none transition-colors focus:border-accent focus:ring-2 focus:ring-accent-soft"
                      value={site.defaultLocale}
                      disabled={languageBusy}
                      onChange={(event) =>
                        void saveLanguages(
                          event.target.value,
                          site.enabledLocales,
                        )
                      }
                    >
                      {site.enabledLocales.map((locale) => (
                        <option key={locale} value={locale}>
                          {languageName(locale)} ({locale})
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="flex min-w-0 flex-col gap-1.5 text-xs font-semibold text-text-secondary">
                    <span>{strings.sitesAddLanguage}</span>
                    <span className="flex min-w-0 flex-wrap items-center gap-2 sm:flex-nowrap">
                      <input
                        className="min-h-11 min-w-0 flex-1 rounded-xl border border-default bg-surface px-3 text-sm font-medium text-text-primary outline-none transition-colors placeholder:text-text-tertiary focus:border-accent focus:ring-2 focus:ring-accent-soft"
                        value={languageInput}
                        placeholder={strings.sitesLanguagePlaceholder}
                        disabled={languageBusy}
                        onChange={(event) =>
                          setLanguageInput(event.target.value)
                        }
                        onKeyDown={(event) => {
                          if (event.key === "Enter") {
                            event.preventDefault();
                            addLanguage();
                          }
                        }}
                      />
                      <Button
                        size="sm"
                        disabled={languageBusy || languageInput.trim() === ""}
                        onClick={addLanguage}
                      >
                        {strings.sitesAddLanguageAction}
                      </Button>
                    </span>
                  </label>
                  {firstIncompleteLocale !== undefined &&
                    firstPageId !== undefined && (
                      <div className="flex items-end lg:col-span-2">
                        <Button
                          variant="ghost"
                          size="sm"
                          icon={<Globe2 size="var(--icon-size-inline)" />}
                          onClick={() =>
                            navigate(
                              `pages/${firstPageId}?locale=${encodeURIComponent(firstIncompleteLocale)}`,
                            )
                          }
                        >
                          {strings.sitesContinueTranslating}
                        </Button>
                      </div>
                    )}
                </div>
                {languageError !== null && (
                  <span
                    className="text-sm font-medium text-danger"
                    role="alert"
                  >
                    {languageError}
                  </span>
                )}
                {translationError !== null && (
                  <span
                    className="text-sm font-medium text-danger"
                    role="alert"
                  >
                    {translationError}
                  </span>
                )}
                {translationBusy && translationProposal === null && (
                  <div
                    className="flex min-h-11 items-center gap-2 text-sm text-text-secondary"
                    role="status"
                  >
                    <Spinner size={16} />
                    <span>{strings.sitesWholeTranslationPreparing}</span>
                  </div>
                )}
                {translationProposal !== null && (
                  <section
                    className="overflow-hidden rounded-xl border border-default bg-surface"
                    aria-labelledby="translation-review-title"
                  >
                    <div className="flex flex-col gap-4 border-b border-subtle p-4 sm:flex-row sm:items-center sm:justify-between">
                      <div>
                        <h3
                          id="translation-review-title"
                          className="m-0 text-base font-semibold text-text-primary"
                        >
                          {strings.sitesWholeTranslationReview(
                            languageName(translationProposal.target_locale),
                          )}
                        </h3>
                        <p className="mb-0 mt-1 text-sm text-text-secondary">
                          {strings.sitesWholeTranslationReviewHint}
                        </p>
                      </div>
                      <div className="flex flex-wrap items-center gap-2">
                        <Button
                          variant="ghost"
                          size="sm"
                          disabled={translationBusy}
                          onClick={() => setTranslationProposal(null)}
                        >
                          {strings.cancel}
                        </Button>
                        <Button
                          size="sm"
                          icon={<Check size="var(--icon-size-inline)" />}
                          disabled={translationBusy}
                          onClick={() => void approveTranslation()}
                        >
                          {strings.sitesWholeTranslationApprove}
                        </Button>
                      </div>
                    </div>
                    <div className="flex max-h-96 flex-col overflow-y-auto">
                      {translationProposal.pages.map(({ before, after }) => (
                        <article
                          className="grid min-h-12 grid-cols-1 gap-2 border-b border-subtle px-4 py-3 text-sm text-text-secondary last:border-b-0 sm:grid-cols-[auto_minmax(0,1fr)_auto_minmax(0,1fr)_auto] sm:items-center sm:gap-3"
                          key={`page-${before.id}`}
                        >
                          <span className="text-xs font-semibold uppercase text-accent-active">
                            {strings.sitesTranslationPageKind}
                          </span>
                          <span>{before.title}</span>
                          <ArrowRight aria-hidden="true" />
                          <strong>{after.title}</strong>
                          <span className="font-mono text-xs text-text-tertiary">
                            /{after.slug}
                          </span>
                        </article>
                      ))}
                      {translationProposal.posts.map(({ before, after }) => (
                        <article
                          className="grid min-h-12 grid-cols-1 gap-2 border-b border-subtle px-4 py-3 text-sm text-text-secondary last:border-b-0 sm:grid-cols-[auto_minmax(0,1fr)_auto_minmax(0,1fr)_auto] sm:items-center sm:gap-3"
                          key={`post-${before.id}`}
                        >
                          <span className="text-xs font-semibold uppercase text-accent-active">
                            {strings.sitesTranslationPostKind}
                          </span>
                          <span>{before.title}</span>
                          <ArrowRight aria-hidden="true" />
                          <strong>{after.title}</strong>
                          <span className="font-mono text-xs text-text-tertiary">
                            /{after.slug}
                          </span>
                        </article>
                      ))}
                    </div>
                  </section>
                )}
              </section>
              </section>
            )}

            {workspace === "tools" && (
              <>
              <section className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm">
              <div className="flex min-h-16 items-center gap-3 px-5 py-3 sm:px-6">
                <span
                  className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent"
                  aria-hidden="true"
                >
                  <Rows3 size={20} />
                </span>
                <span className="min-w-0 flex-1">
                  <h2 className="m-0 block font-semibold text-text-primary">
                    {strings.sitesSiteTools}
                  </h2>
                  <span className="block truncate text-sm font-normal text-text-secondary">
                    {strings.sitesSiteToolsHint}
                  </span>
                </span>
              </div>
              <div className="grid gap-2 border-t border-subtle p-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<ShoppingBag size="var(--icon-size-inline)" />}
                  onClick={() => navigate("catalogs")}
                >
                  {strings.sitesCatalogs}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<Receipt size="var(--icon-size-inline)" />}
                  onClick={() => navigate("orders")}
                >
                  {strings.sitesOrders}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<Ticket size="var(--icon-size-inline)" />}
                  onClick={() => navigate("tickets")}
                >
                  {strings.sitesTickets}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<Package size="var(--icon-size-inline)" />}
                  onClick={() => navigate("shop")}
                >
                  {strings.sitesShop}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<CalendarClock size="var(--icon-size-inline)" />}
                  onClick={() => navigate("bookings")}
                >
                  {strings.sitesBookings}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<Rows3 size="var(--icon-size-inline)" />}
                  onClick={() => navigate("collections")}
                >
                  {strings.sitesCollections}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<Newspaper size="var(--icon-size-inline)" />}
                  onClick={() => navigate("posts")}
                >
                  {strings.sitesPosts}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<Inbox size="var(--icon-size-inline)" />}
                  onClick={() => navigate("submissions")}
                >
                  {strings.sitesSubmissions}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<BarChart3 size="var(--icon-size-inline)" />}
                  onClick={() => navigate("analytics")}
                >
                  {strings.sitesAnalytics}
                </Button>
                {/* The assistant is the owner's door — switching it on, setting
                  its budget and publishing what it reads are owner acts
                  (ADR 0040), so like Collaborators it only renders for the
                  person who can actually open it. */}
                {site.canManageCollaborators && (
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={<Bot size="var(--icon-size-inline)" />}
                    onClick={() => navigate("assistant")}
                  >
                    {strings.sitesAssistant}
                  </Button>
                )}
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<Handshake size="var(--icon-size-inline)" />}
                  onClick={() => navigate("funnel")}
                >
                  {strings.sitesFunnel}
                </Button>
              </div>
              </section>

            {/* The site agent is support for the record, not the main task on
                this screen. Keeping it after the site's own controls preserves
                every capability without competing with pages and publishing. */}
            <RecordAgentPanel
              product="sites"
              recordKind="site"
              recordId={site.id}
              recordLabel={site.name}
              origin={null}
            />
              </>
            )}
          </>
        )}
      </div>

      {theming && site !== null && (
        <ThemeDialog
          siteId={site.id}
          onClose={() => setTheming(false)}
          onApplied={() => {
            setTheming(false);
            void load();
          }}
        />
      )}

      {creating && site !== null && (
        <NewPageDialog
          siteId={site.id}
          firstPage={pages.length === 0}
          onClose={() => setCreating(false)}
          onCreated={() => {
            setCreating(false);
            void load();
          }}
        />
      )}
    </div>
  );
}
