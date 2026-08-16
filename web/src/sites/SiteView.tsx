// One site: its name, address and live/draft state, the publish switch, and
// the list of its pages in navigation order. This is the site's home surface
// — the section editor (S1.12), theme (S1.14) and publish (S1.15) all mount
// here. A stale or foreign id reads as "not found" with the way back, never
// a broken screen.
import { useCallback, useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import {
  ArrowLeft,
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
  Lock,
  Newspaper,
  Package,
  Palette,
  Receipt,
  ShoppingBag,
  Sparkles,
  Rows3,
  Store,
  Ticket,
  X,
} from "lucide-react";

import { strings } from "../i18n";
import { Button, Spinner } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import { NewPageDialog } from "./NewPageDialog";
import { SchedulePublish } from "./SchedulePublish";
import { SiteCollaborators } from "./SiteCollaborators";
import { ThemeDialog } from "./ThemeDialog";
import { EmptyState, ErrorBanner } from "./parts";
import type {
  SiteDetail,
  SitePage,
  SiteTranslationEnvelope,
  SiteTranslationReadiness,
} from "./types";
import styles from "./SitesModule.module.css";

export function SiteView() {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
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

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to=".." relative="path" className={styles.backLink}>
          <ArrowLeft size={16} aria-hidden="true" />
          {strings.sitesBack}
        </Link>
        {site !== null && (
          <div className={styles.siteHead}>
            <h1 className={styles.title}>{site.name}</h1>
            <span className={styles.mono}>{site.subdomain}</span>
            <span
              className={
                live ? `${styles.chip} ${styles.chipLive}` : styles.chip
              }
            >
              {live ? strings.sitesStatusLive : strings.sitesStatusDraft}
            </span>
          </div>
        )}
        {loading && <Spinner size={16} />}
      </header>

      {/* Everything below the header scrolls as one document: this screen is
          a stack of panels, not a viewport column, and on a phone the pages
          table lives below the fold. */}
      <div className={styles.pageBody}>
      {error !== null && <ErrorBanner message={error} />}

      {site !== null && (
        <>
          <div className={styles.publishBar}>
            <div className={styles.publishCopy}>
              {live && host !== null && (
                <>
                  <span>{strings.sitesLiveAtLabel}</span>
                  <a
                    href={`https://${host}`}
                    target="_blank"
                    rel="noreferrer"
                    className={styles.liveLink}
                  >
                    {host}
                  </a>
                </>
              )}
              {!live && host !== null && (
                <span>{strings.sitesGoesLiveAt(host)}</span>
              )}
              {readiness !== null && readiness.totalPages > 0 && (
                <span
                  className={
                    missingTranslations === 0
                      ? styles.translationReady
                      : styles.translationWarning
                  }
                >
                  {missingTranslations === 0
                    ? strings.sitesTranslationAllReady
                    : strings.sitesTranslationPublishHint(missingTranslations)}
                </span>
              )}
              {publishError !== null && (
                <span className={styles.publishError} role="alert">
                  {publishError}
                </span>
              )}
            </div>
            <div className={styles.publishActions}>
              {/* Domains belongs beside the address, not among the content
                  screens: it is the question "where does this website live?",
                  which is what the line to its left just answered. */}
              <Button
                variant="ghost"
                size="sm"
                icon={<Globe2 size="var(--icon-size-inline)" />}
                onClick={() => navigate("domains")}
              >
                {strings.sitesDomains}
              </Button>
              {/* History belongs beside Publish: it is the question "what did
                  the last publish look like, and can I have it back?". */}
              <Button
                variant="ghost"
                size="sm"
                icon={<History size="var(--icon-size-inline)" />}
                onClick={() => navigate("history")}
              >
                {strings.sitesHistory}
              </Button>
              {live && (
                <Button
                  variant={confirmingOffline ? "danger" : "ghost"}
                  size="sm"
                  disabled={publishBusy}
                  onClick={() => void unpublish()}
                >
                  {confirmingOffline
                    ? strings.sitesConfirmUnpublish
                    : strings.sitesUnpublish}
                </Button>
              )}
              <Button
                size="sm"
                disabled={publishBusy}
                onClick={() => void publish()}
              >
                {live ? strings.sitesPublishChanges : strings.sitesPublish}
              </Button>
            </div>
          </div>

          {/* Publishing later belongs directly under publishing now: they are
              the same decision, one of them with a moment attached. */}
          <SchedulePublish siteId={site.id} onPublished={() => void load()} />

          {site.canManageCollaborators && <SiteCollaborators siteId={site.id} />}

          <section
            className={styles.languagePanel}
            aria-labelledby="site-languages-title"
          >
            <div className={styles.languagePanelIntro}>
              <span className={styles.languagePanelIcon} aria-hidden="true">
                <Languages />
              </span>
              <div>
                <h2 id="site-languages-title" className={styles.languageTitle}>
                  {strings.sitesLanguages}
                </h2>
                <p className={styles.languageHint}>
                  {strings.sitesLanguagesHint}
                </p>
              </div>
            </div>

            <div className={styles.languageRows}>
              {readiness?.languages.map((language) => (
                <div className={styles.languageRow} key={language.locale}>
                  <span className={styles.languageCode}>
                    {language.locale.toUpperCase()}
                  </span>
                  <span className={styles.languageName}>
                    {languageName(language.locale)}
                  </span>
                  {language.locale === site.defaultLocale && (
                    <span className={styles.badge}>
                      {strings.sitesLanguageDefaultBadge}
                    </span>
                  )}
                  <span
                    className={
                      language.ready
                        ? styles.translationReady
                        : styles.translationWarning
                    }
                  >
                    {language.ready
                      ? strings.sitesTranslationReady
                      : strings.sitesTranslationProgress(
                          language.translatedPages,
                          readiness.totalPages,
                        )}
                  </span>
                  {language.locale !== site.defaultLocale && (
                    <span className={styles.languageRowActions}>
                      <Button
                        variant="ghost"
                        size="sm"
                        icon={<Sparkles size="var(--icon-size-inline)" />}
                        disabled={languageBusy || translationBusy}
                        onClick={() => void prepareTranslation(language.locale)}
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

            <div className={styles.languageControls}>
              <label className={styles.languageControl}>
                <span>{strings.sitesDefaultLanguage}</span>
                <select
                  className={styles.input}
                  value={site.defaultLocale}
                  disabled={languageBusy}
                  onChange={(event) =>
                    void saveLanguages(event.target.value, site.enabledLocales)
                  }
                >
                  {site.enabledLocales.map((locale) => (
                    <option key={locale} value={locale}>
                      {languageName(locale)} ({locale})
                    </option>
                  ))}
                </select>
              </label>
              <label className={styles.languageControl}>
                <span>{strings.sitesAddLanguage}</span>
                <span className={styles.languageAddRow}>
                  <input
                    className={styles.input}
                    value={languageInput}
                    placeholder={strings.sitesLanguagePlaceholder}
                    disabled={languageBusy}
                    onChange={(event) => setLanguageInput(event.target.value)}
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
                )}
            </div>
            {languageError !== null && (
              <span className={styles.publishError} role="alert">
                {languageError}
              </span>
            )}
            {translationError !== null && (
              <span className={styles.publishError} role="alert">
                {translationError}
              </span>
            )}
            {translationBusy && translationProposal === null && (
              <div className={styles.translationPreparing} role="status">
                <Spinner size={16} />
                <span>{strings.sitesWholeTranslationPreparing}</span>
              </div>
            )}
            {translationProposal !== null && (
              <section
                className={styles.translationReview}
                aria-labelledby="translation-review-title"
              >
                <div className={styles.translationReviewHead}>
                  <div>
                    <h3 id="translation-review-title">
                      {strings.sitesWholeTranslationReview(
                        languageName(translationProposal.target_locale),
                      )}
                    </h3>
                    <p>{strings.sitesWholeTranslationReviewHint}</p>
                  </div>
                  <div className={styles.translationReviewActions}>
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
                <div className={styles.translationReviewList}>
                  {translationProposal.pages.map(({ before, after }) => (
                    <article
                      className={styles.translationReviewItem}
                      key={`page-${before.id}`}
                    >
                      <span className={styles.translationReviewKind}>
                        {strings.sitesTranslationPageKind}
                      </span>
                      <span>{before.title}</span>
                      <ArrowRight aria-hidden="true" />
                      <strong>{after.title}</strong>
                      <span className={styles.translationReviewSlug}>
                        /{after.slug}
                      </span>
                    </article>
                  ))}
                  {translationProposal.posts.map(({ before, after }) => (
                    <article
                      className={styles.translationReviewItem}
                      key={`post-${before.id}`}
                    >
                      <span className={styles.translationReviewKind}>
                        {strings.sitesTranslationPostKind}
                      </span>
                      <span>{before.title}</span>
                      <ArrowRight aria-hidden="true" />
                      <strong>{after.title}</strong>
                      <span className={styles.translationReviewSlug}>
                        /{after.slug}
                      </span>
                    </article>
                  ))}
                </div>
              </section>
            )}
          </section>

          <div className={styles.sectionBar}>
            <h2 className={styles.sectionTitle}>{strings.sitesPages}</h2>
            <div className={styles.sectionBarActions}>
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
              {/* Shop setup is all owner acts — the proposal names Billing
                  prices and VAT, and every apply goes through owner-side
                  routes (S3.06a) — so like the assistant it only renders for
                  the person who can actually use it. Tickets and Shop stay:
                  their lists are a collaborator's read. */}
              {site.canManageCollaborators && (
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<Store size="var(--icon-size-inline)" />}
                  onClick={() => navigate("shop-setup")}
                >
                  {strings.sitesShopSetup}
                </Button>
              )}
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
              <Button
                variant="ghost"
                size="sm"
                icon={<Palette size="var(--icon-size-inline)" />}
                onClick={() => setTheming(true)}
              >
                {strings.sitesTheme}
              </Button>
              <Button size="sm" onClick={() => setCreating(true)}>
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
              onCta={() => setCreating(true)}
            />
          ) : (
            <div className={styles.tableWrapStatic}>
              <table className={styles.table}>
                <thead>
                  <tr>
                    <th scope="col">{strings.sitesColPage}</th>
                    <th scope="col">{strings.sitesColPath}</th>
                  </tr>
                </thead>
                <tbody>
                  {pages.map((p) => (
                    <tr key={p.id}>
                      <td>
                        {/* Opens the page's section editor. */}
                        <Link to={`pages/${p.id}`} className={styles.pageLink}>
                          {p.title}
                        </Link>
                        {p.home && (
                          <span className={styles.badge}>
                            {strings.sitesHomeBadge}
                          </span>
                        )}
                        {protectedPages.has(p.id) && (
                          <span className={styles.pageLockBadge}>
                            <Lock size={11} aria-hidden="true" />
                            {strings.sitesPagePasswordBadge}
                          </span>
                        )}
                      </td>
                      <td className={styles.mono}>/{p.slug}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
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
