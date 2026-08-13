// The page editor: the stack of sections a page is built from. Every gesture
// — add (picker → prop form), edit, drag- or button-reorder, delete — is one
// call to the section ops of the edit API, and the stack always renders the
// canonical envelope the server answered, so what you see IS what is stored.
// There is no local dirty buffer to lose, and a refusal (422) points at the
// exact gesture that broke the rule.
//
// Text is edited where it lives (ADR 0042): the preview is rendered with the
// coordinate of every single-string property on the element that shows it, and
// typing there comes back as one `rewrite_copy` — the identical operation the
// AI panel proposes, through the identical door. That is what lets one undo
// history cover both, and why there is no "inline save" endpoint.
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import {
  ArrowLeft,
  Copy,
  ChevronDown,
  ChevronUp,
  GripVertical,
  Layers,
  Lock,
  Monitor,
  Palette,
  Pencil,
  Redo2,
  SearchCheck,
  Smartphone,
  Trash2,
  Undo2,
} from "lucide-react";

import { strings } from "../i18n";
import { Button, IconButton, Spinner } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import { kindLabel, sectionSummary } from "./sectionInfo";
import { SectionFormDialog } from "./SectionForm";
import { SectionPicker } from "./SectionPicker";
import { ThemeDialog } from "./ThemeDialog";
import { PageSeoDialog } from "./PageSeoDialog";
import { PageAiEditPanel } from "./PageAiEditPanel";
import { PagePassword } from "./PagePassword";
import {
  emptyTextEditHistory,
  keyTarget,
  readTextEditMessage,
  recordTextEdit,
  redoTextEdit,
  textEditEnvelope,
  textEditOperation,
  undoTextEdit,
  type TextEditHistory,
  type TextEditStep,
} from "./inlineText";
import { EmptyState, ErrorBanner } from "./parts";
import type { Section, SectionKind, SectionsEnvelope } from "./sections";
import type { SitePageDetail } from "./types";
import styles from "./SitesModule.module.css";

/** Which section the prop form is editing: a fresh one of `kind` when
 *  `index` is null, the stored one at `index` otherwise. */
interface FormTarget {
  kind: SectionKind;
  index: number | null;
}

export function PageEditorView() {
  const { siteId = "", pageId = "" } = useParams();
  const [searchParams, setSearchParams] = useSearchParams();
  const locale = searchParams.get("locale");
  const api = useSitesApi();
  const [page, setPage] = useState<SitePageDetail | null>(null);
  const [defaultLocale, setDefaultLocale] = useState("en");
  const [enabledLocales, setEnabledLocales] = useState<string[]>(["en"]);
  const [translationFallback, setTranslationFallback] = useState(false);
  const [resolvedLocale, setResolvedLocale] = useState("en");
  const [titleDraft, setTitleDraft] = useState("");
  const [slugDraft, setSlugDraft] = useState("");
  const [seoTitleDraft, setSeoTitleDraft] = useState("");
  const [seoDescriptionDraft, setSeoDescriptionDraft] = useState("");
  const [translationBusy, setTranslationBusy] = useState(false);
  const [translationError, setTranslationError] = useState<string | null>(null);
  const [sections, setSections] = useState<Section[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [working, setWorking] = useState(false);

  const [picking, setPicking] = useState(false);
  const [form, setForm] = useState<FormTarget | null>(null);
  const [formBusy, setFormBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<number | null>(null);
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [dragOver, setDragOver] = useState<number | null>(null);

  const previewFrame = useRef<HTMLIFrameElement | null>(null);
  // The stack the inline-edit handlers resolve a coordinate against. A ref
  // rather than the state variable: a message from the preview frame or a
  // ⌘Z arrive whenever the person decides, and both have to be answered
  // against the page as it is *now* — a handler that closed over an older
  // stack would aim a rewrite at a section that has since moved.
  const sectionsRef = useRef<Section[]>([]);
  const [history, setHistory] = useState<TextEditHistory>(emptyTextEditHistory);
  const [textNotice, setTextNotice] = useState("");
  const [previewHtml, setPreviewHtml] = useState<string | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewMobile, setPreviewMobile] = useState(false);
  const [proposedPreviewHtml, setProposedPreviewHtml] = useState<string | null>(
    null,
  );
  const [previewVersion, setPreviewVersion] = useState<"before" | "after">(
    "before",
  );
  const [themeOpen, setThemeOpen] = useState(false);
  const [seoOpen, setSeoOpen] = useState(false);
  // Whether visitors meet an unlock screen before this page (S2.06b). Owned
  // by the password panel and mirrored here for one reason: a preview that
  // does not say so is a preview that lies about what the internet sees.
  const [pageProtected, setPageProtected] = useState(false);
  // Bumped when the theme changes — the preview document depends on the
  // site's theme, not only on this page's sections.
  const [previewEpoch, setPreviewEpoch] = useState(0);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [site, detail] = await Promise.all([
        api.site(siteId),
        locale === null
          ? api.page(siteId, pageId)
          : api.localizedPage(siteId, pageId, locale),
      ]);
      const siteDefault = site.defaultLocale ?? "en";
      const siteLanguages =
        Array.isArray(site.enabledLocales) && site.enabledLocales.length > 0
          ? site.enabledLocales
          : [siteDefault];
      setDefaultLocale(siteDefault);
      setEnabledLocales(siteLanguages);
      setPage(detail);
      setSections(detail.sections.sections);
      setTitleDraft(detail.title);
      setSlugDraft(detail.slug);
      setSeoTitleDraft(detail.seoTitle ?? "");
      setSeoDescriptionDraft(detail.seoDescription ?? "");
      setTranslationFallback("fallback" in detail && detail.fallback === true);
      setResolvedLocale(
        "resolvedLocale" in detail && typeof detail.resolvedLocale === "string"
          ? detail.resolvedLocale
          : siteDefault,
      );
      setTranslationError(null);
      setError(null);
    } catch (err) {
      setError(sitesMessage(err, strings.sitesPageLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId, pageId, locale]);

  useEffect(() => {
    void load();
  }, [load]);

  // A layout effect, not a passive one: it must have run by the time any
  // event can reach a handler, and passive effects are allowed to wait.
  useLayoutEffect(() => {
    sectionsRef.current = sections;
  }, [sections]);

  useEffect(() => {
    setProposedPreviewHtml(null);
    setPreviewVersion("before");
  }, [siteId, pageId]);

  // Undo belongs to the page being edited. Carrying it across a page or a
  // language would offer to take back a change on a document that is no
  // longer on screen.
  useEffect(() => {
    setHistory(emptyTextEditHistory);
    setTextNotice("");
  }, [siteId, pageId, locale]);

  // The live preview: the server renders the draft with the exact renderer
  // publishing uses. `sections` is always the envelope the last op answered,
  // so keying on it refreshes the pane after every successful save — and
  // not after a refused one.
  useEffect(() => {
    if (page === null) return undefined;
    let stale = false;
    api.pagePreview(siteId, pageId, locale ?? undefined).then(
      (html) => {
        if (!stale) {
          setPreviewHtml(html);
          setPreviewError(null);
        }
      },
      (err: unknown) => {
        if (!stale)
          setPreviewError(sitesMessage(err, strings.sitesPreviewFailed));
      },
    );
    return () => {
      stale = true;
    };
  }, [api, siteId, pageId, locale, page, sections, previewEpoch]);

  async function saveLocalized(
    nextSections: Section[],
    identity = false,
  ): Promise<boolean> {
    if (page === null || locale === null) return false;
    setTranslationBusy(true);
    setTranslationError(null);
    try {
      const saved = await api.setLocalizedPage(siteId, pageId, locale, {
        title: identity ? titleDraft : page.title,
        slug: identity ? slugDraft : page.slug,
        sections: { ...page.sections, sections: nextSections },
        seoTitle: identity ? seoTitleDraft : page.seoTitle,
        seoDescription: identity ? seoDescriptionDraft : page.seoDescription,
      });
      setPage(saved);
      setSections(saved.sections.sections);
      setTitleDraft(saved.title);
      setSlugDraft(saved.slug);
      setSeoTitleDraft(saved.seoTitle ?? "");
      setSeoDescriptionDraft(saved.seoDescription ?? "");
      setTranslationFallback(false);
      setResolvedLocale(saved.resolvedLocale);
      setPreviewEpoch((epoch) => epoch + 1);
      setError(null);
      return true;
    } catch (err) {
      setTranslationError(
        sitesMessage(err, strings.sitesTranslationSaveFailed),
      );
      return false;
    } finally {
      setTranslationBusy(false);
    }
  }

  async function saveIdentity() {
    if (page === null || translationFallback) return;
    if (locale !== null) {
      await saveLocalized(sections, true);
      return;
    }
    setTranslationBusy(true);
    setTranslationError(null);
    try {
      await api.setPageIdentity(siteId, pageId, titleDraft, slugDraft);
      await api.setPageSeo(siteId, pageId, seoTitleDraft, seoDescriptionDraft);
      await load();
    } catch (err) {
      setTranslationError(
        sitesMessage(err, strings.sitesTranslationSaveFailed),
      );
    } finally {
      setTranslationBusy(false);
    }
  }

  function chooseLocale(nextLocale: string) {
    setSearchParams({ locale: nextLocale });
  }

  /** Runs one stack op and renders the envelope the server answered. */
  async function run(op: Promise<SectionsEnvelope>) {
    setWorking(true);
    setConfirmDelete(null);
    try {
      setSections((await op).sections);
      setError(null);
    } catch (err) {
      focusAfterMove.current = null;
      setError(sitesMessage(err, strings.sitesSaveFailed));
    } finally {
      setWorking(false);
    }
  }

  /** Focus follows the section, not the row it used to sit in.
   *
   *  A move replaces the whole list with the server's answer, so React
   *  unmounts the row that had focus and the caret falls to `<body>`: measured
   *  in a browser at 360px, moving one section down cost ten more Tab presses
   *  to get back to it, which is a reorder nobody can do twice. The row is
   *  found again by position after the new list renders, and the same control
   *  is refocused — falling back to its sibling when the button that was
   *  pressed is the one the new position disables (the last row has no "move
   *  down", the first no "move up"). */
  const stackRef = useRef<HTMLOListElement | null>(null);
  const focusAfterMove = useRef<{
    index: number;
    control: string;
    /** The list as it was when the move was asked for. Until `sections` is a
     *  different array the stack on screen is still the old order, and the row
     *  at `index` holds somebody else's section. */
    before: Section[];
  } | null>(null);
  const [moveNotice, setMoveNotice] = useState("");

  useEffect(() => {
    const want = focusAfterMove.current;
    // Two things have to have happened. The server's list has to have
    // replaced the old one — until then the row at `index` is the section
    // that was there before — and the op has to have finished, because every
    // control is disabled while it is in flight and a disabled button cannot
    // take focus.
    if (want === null || sections === want.before) return;
    if (working || translationBusy) return;
    const row = stackRef.current?.children.item(want.index);
    if (!(row instanceof HTMLElement)) return;
    const wanted = row.querySelector<HTMLButtonElement>(
      `[data-section-control="${want.control}"]`,
    );
    const target =
      wanted !== null && !wanted.disabled
        ? wanted
        : row.querySelector<HTMLButtonElement>(
            "[data-section-control]:not(:disabled)",
          );
    if (target === null) return;
    focusAfterMove.current = null;
    target.focus();
  }, [sections, working, translationBusy]);

  function move(from: number, to: number, control?: string) {
    if (to < 0 || to >= sections.length || from === to) return;
    const moving = sections[from];
    if (moving !== undefined) {
      // Announced as well as done: a reorder is invisible to a reader who
      // cannot see the stack reflow.
      setMoveNotice(
        strings.sitesSectionMoved(
          kindLabel(moving.type),
          to + 1,
          sections.length,
        ),
      );
    }
    if (control !== undefined)
      focusAfterMove.current = { index: to, control, before: sections };
    if (locale !== null) {
      const reordered = [...sections];
      const [moved] = reordered.splice(from, 1);
      if (moved === undefined) return;
      reordered.splice(to, 0, moved);
      void saveLocalized(reordered);
      return;
    }
    void run(api.moveSection(siteId, pageId, from, to));
  }

  function remove(index: number) {
    if (locale !== null) {
      void saveLocalized(
        sections.filter((_, sectionIndex) => sectionIndex !== index),
      );
      setConfirmDelete(null);
      return;
    }
    void run(api.removeSection(siteId, pageId, index));
  }

  /** Applies one text edit typed on the page.
   *
   *  The coordinate is resolved against the sections this editor is holding,
   *  never trusted from the frame, and the result travels as the same
   *  `rewrite_copy` operation an approved AI proposal carries. Whatever
   *  happens, the preview ends up showing the stored truth: on success the
   *  server's envelope replaces the stack (which re-renders the frame), on a
   *  refusal the frame is re-fetched so the typed text does not linger as a
   *  change that was never saved.
   *
   *  `replay` is set when undo or redo is re-applying a known step, which is
   *  the one case that must not push a new entry onto the history. */
  const applyInlineText = useCallback(
    async (
      key: string,
      text: string,
      replay: TextEditStep | null = null,
    ): Promise<boolean> => {
      const current = sectionsRef.current;
      const found = keyTarget(current, key);
      const operation = textEditOperation(current, key, text);
      if (found === null || operation === null) {
        setError(strings.sitesInlineTextStale);
        setPreviewEpoch((epoch) => epoch + 1);
        return false;
      }
      if (found.current === text) return true;
      setWorking(true);
      try {
        const envelope = await api.applyPageEdit(
          siteId,
          pageId,
          textEditEnvelope(operation),
        );
        setSections(envelope.sections);
        setError(null);
        if (replay === null) {
          setHistory((current) =>
            recordTextEdit(current, {
              key,
              before: found.current,
              after: text,
            }),
          );
          setTextNotice(strings.sitesInlineTextSaved);
        }
        return true;
      } catch (err) {
        setError(sitesMessage(err, strings.sitesSaveFailed));
        setPreviewEpoch((epoch) => epoch + 1);
        return false;
      } finally {
        setWorking(false);
      }
    },
    [api, pageId, siteId],
  );

  // The frame's half of direct manipulation. Its document has an opaque
  // origin, so `event.origin` proves nothing and the sender is proven
  // instead: only this editor's own preview window is listened to.
  useEffect(() => {
    if (locale !== null) return undefined;
    function onMessage(event: MessageEvent) {
      const frame = previewFrame.current;
      const edit = readTextEditMessage(
        event.data,
        frame !== null && event.source === frame.contentWindow,
      );
      if (edit === null) return;
      void applyInlineText(edit.key, edit.text);
    }
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [applyInlineText, locale]);

  async function undoText() {
    const step = undoTextEdit(history);
    if (step === null) return;
    if (await applyInlineText(step.step.key, step.text, step.step)) {
      setHistory(step.history);
      setTextNotice(strings.sitesInlineTextUndone);
    }
  }

  async function redoText() {
    const step = redoTextEdit(history);
    if (step === null) return;
    if (await applyInlineText(step.step.key, step.text, step.step)) {
      setHistory(step.history);
      setTextNotice(strings.sitesInlineTextRedone);
    }
  }

  // ⌘Z / Ctrl+Z, with shift to redo. Keys pressed inside the preview stay
  // inside the frame, so this never competes with typing on the page; a field
  // in the app around it keeps its own native undo.
  useEffect(() => {
    if (locale !== null) return undefined;
    function onKeyDown(event: KeyboardEvent) {
      if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "z") {
        return;
      }
      const target = event.target;
      if (
        target instanceof HTMLElement &&
        (target.isContentEditable ||
          ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))
      ) {
        return;
      }
      event.preventDefault();
      void (event.shiftKey ? redoText() : undoText());
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  /** The prop form's save: add for a fresh section, replace for a stored
   *  one. A refusal stays in the dialog with everything the user typed. */
  async function save(target: FormTarget, section: Section) {
    setFormBusy(true);
    try {
      if (locale !== null) {
        const nextSections = [...sections];
        if (target.index === null) nextSections.push(section);
        else nextSections[target.index] = section;
        const saved = await saveLocalized(nextSections);
        if (!saved) return;
        setForm(null);
        setFormError(null);
        return;
      }
      const envelope =
        target.index === null
          ? await api.addSection(siteId, pageId, section)
          : await api.updateSection(siteId, pageId, target.index, section);
      setSections(envelope.sections);
      setForm(null);
      setFormError(null);
    } catch (err) {
      setFormError(sitesMessage(err, strings.sitesSaveFailed));
    } finally {
      setFormBusy(false);
    }
  }

  function openForm(target: FormTarget) {
    setFormError(null);
    setConfirmDelete(null);
    setForm(target);
  }

  const empty = sections.length === 0;
  const requestedLanguage = locale === null ? defaultLocale : locale;

  async function copyFallback() {
    if (page === null || locale === null) return;
    await saveLocalized(sections);
  }

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to={`/sites/${siteId}`} className={styles.backLink}>
          <ArrowLeft size={16} aria-hidden="true" />
          {strings.sitesBackToSite}
        </Link>
        {page !== null && (
          <div className={styles.siteHead}>
            <h1 className={styles.title}>{page.title}</h1>
            <span className={styles.mono}>/{page.slug}</span>
            {page.home && (
              <span className={styles.badge}>{strings.sitesHomeBadge}</span>
            )}
          </div>
        )}
        {loading && <Spinner size={16} />}
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {page !== null && (
        <>
          <nav
            className={styles.localeStrip}
            aria-label={strings.sitesLanguagesLabel}
          >
            <span className={styles.localeStripLabel}>
              {strings.sitesEditingLanguage}
            </span>
            <div className={styles.localeTabs}>
              {enabledLocales.map((enabledLocale) => (
                <Button
                  key={enabledLocale}
                  variant="ghost"
                  size="sm"
                  className={
                    requestedLanguage === enabledLocale
                      ? styles.localeTabActive
                      : styles.localeTab
                  }
                  aria-pressed={requestedLanguage === enabledLocale}
                  onClick={() => chooseLocale(enabledLocale)}
                >
                  {enabledLocale.toUpperCase()}
                </Button>
              ))}
            </div>
          </nav>

          {locale !== null && translationFallback && (
            <section className={styles.translationMissing} aria-live="polite">
              <div>
                <h2 className={styles.translationMissingTitle}>
                  {strings.sitesTranslationMissingTitle(locale.toUpperCase())}
                </h2>
                <p className={styles.translationMissingBody}>
                  {strings.sitesTranslationMissingBody(
                    locale.toUpperCase(),
                    resolvedLocale.toUpperCase(),
                  )}
                </p>
              </div>
              <Button
                size="sm"
                icon={<Copy size="var(--icon-size-inline)" />}
                disabled={translationBusy}
                onClick={() => void copyFallback()}
              >
                {strings.sitesCopyTranslation(
                  resolvedLocale.toUpperCase(),
                  locale.toUpperCase(),
                )}
              </Button>
            </section>
          )}

          {locale !== null && !translationFallback && (
            <section className={styles.translationDetails}>
              <div className={styles.translationDetailsIntro}>
                <h2 className={styles.translationDetailsTitle}>
                  {strings.sitesTranslationDetails}
                </h2>
                <p className={styles.translationDetailsHint}>
                  {strings.sitesTranslationDetailsHint(locale.toUpperCase())}
                </p>
              </div>
              <div className={styles.translationFields}>
                <label className={styles.translationField}>
                  <span>{strings.sitesFieldPageTitle}</span>
                  <input
                    className={styles.input}
                    value={titleDraft}
                    disabled={translationBusy}
                    onChange={(event) => setTitleDraft(event.target.value)}
                  />
                </label>
                <label className={styles.translationField}>
                  <span>{strings.sitesFieldSlug}</span>
                  <input
                    className={styles.input}
                    value={slugDraft}
                    disabled={translationBusy || page.home}
                    onChange={(event) => setSlugDraft(event.target.value)}
                  />
                </label>
                <label className={styles.translationField}>
                  <span>{strings.sitesSeoFieldTitle}</span>
                  <input
                    className={styles.input}
                    value={seoTitleDraft}
                    disabled={translationBusy}
                    onChange={(event) => setSeoTitleDraft(event.target.value)}
                  />
                </label>
                <label className={styles.translationField}>
                  <span>{strings.sitesSeoFieldDescription}</span>
                  <input
                    className={styles.input}
                    value={seoDescriptionDraft}
                    disabled={translationBusy}
                    onChange={(event) =>
                      setSeoDescriptionDraft(event.target.value)
                    }
                  />
                </label>
              </div>
              <Button
                size="sm"
                disabled={translationBusy}
                onClick={() => void saveIdentity()}
              >
                {strings.sitesSaveTranslation}
              </Button>
            </section>
          )}

          {translationError !== null && (
            <ErrorBanner message={translationError} />
          )}

          {/* Who may read this page sits above how it is built: it is a fact
              about the page itself, not about one language's copy of it. */}
          <PagePassword
            siteId={siteId}
            pageId={pageId}
            multilingual={enabledLocales.length > 1}
            onChange={setPageProtected}
          />

          <div className={styles.editorLayout}>
            <div className={styles.stackPane}>
              <div className={styles.sectionBar}>
                <h2 className={styles.sectionTitle}>{strings.sitesSections}</h2>
                <div className={styles.sectionBarActions}>
                  {locale === null && (
                    <>
                      <IconButton
                        size="sm"
                        label={strings.sitesUndoTextEdit}
                        icon={<Undo2 size={15} />}
                        disabled={working || history.past.length === 0}
                        onClick={() => void undoText()}
                      />
                      <IconButton
                        size="sm"
                        label={strings.sitesRedoTextEdit}
                        icon={<Redo2 size={15} />}
                        disabled={working || history.future.length === 0}
                        onClick={() => void redoText()}
                      />
                    </>
                  )}
                  {locale === null && (
                    <Button
                      variant="ghost"
                      size="sm"
                      icon={<SearchCheck size={14} />}
                      onClick={() => setSeoOpen(true)}
                    >
                      {strings.sitesSeoAction}
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={<Palette size={14} />}
                    onClick={() => setThemeOpen(true)}
                  >
                    {strings.sitesTheme}
                  </Button>
                  <Button
                    size="sm"
                    onClick={() => setPicking(true)}
                    disabled={working || translationBusy || translationFallback}
                  >
                    {strings.sitesAddSection}
                  </Button>
                </div>
              </div>

              {locale === null && (
                <PageAiEditPanel
                  siteId={siteId}
                  pageId={pageId}
                  onPreviewChange={(html) => {
                    setProposedPreviewHtml(html);
                    setPreviewVersion(html === null ? "before" : "after");
                  }}
                  onApplied={(envelope) => {
                    setSections(envelope.sections);
                    setError(null);
                  }}
                />
              )}

              {/* Reordering is the one edit on this screen with no visible
                  result outside the stack itself. */}
              <p className={styles.srOnly} role="status">
                {moveNotice}
                {textNotice !== "" && ` ${textNotice}`}
              </p>

              {empty && !loading ? (
                <EmptyState
                  Icon={Layers}
                  title={strings.sitesNoSectionsTitle}
                  body={strings.sitesNoSectionsBody}
                  cta={strings.sitesAddFirstSection}
                  onCta={() => openForm({ kind: "hero", index: null })}
                />
              ) : (
                <ol className={styles.stack} ref={stackRef}>
                  {sections.map((section, i) => {
                    const summary = sectionSummary(section);
                    const cardClass =
                      dragOver === i && dragFrom !== null && dragFrom !== i
                        ? `${styles.card} ${styles.cardDropTarget}`
                        : styles.card;
                    return (
                      // Sections have no identity — the position is the key.
                      <li
                        key={`${section.type}-${i}`}
                        className={cardClass}
                        draggable
                        onDragStart={() => setDragFrom(i)}
                        onDragOver={(e) => {
                          e.preventDefault();
                          setDragOver(i);
                        }}
                        onDrop={(e) => {
                          e.preventDefault();
                          if (dragFrom !== null) move(dragFrom, i);
                          setDragFrom(null);
                          setDragOver(null);
                        }}
                        onDragEnd={() => {
                          setDragFrom(null);
                          setDragOver(null);
                        }}
                      >
                        <span className={styles.dragHandle} aria-hidden="true">
                          <GripVertical size={16} />
                        </span>
                        <div className={styles.cardMeta}>
                          <span className={styles.cardKind}>
                            {kindLabel(section.type)}
                          </span>
                          {summary !== "" && (
                            <span className={styles.cardSummary}>
                              {summary}
                            </span>
                          )}
                        </div>
                        <div className={styles.cardActions}>
                          <IconButton
                            size="sm"
                            label={strings.sitesMoveUp(kindLabel(section.type))}
                            data-section-control="up"
                            icon={<ChevronUp size={15} />}
                            disabled={
                              working ||
                              translationBusy ||
                              translationFallback ||
                              i === 0
                            }
                            onClick={() => move(i, i - 1, "up")}
                          />
                          <IconButton
                            size="sm"
                            label={strings.sitesMoveDown(
                              kindLabel(section.type),
                            )}
                            data-section-control="down"
                            icon={<ChevronDown size={15} />}
                            disabled={
                              working ||
                              translationBusy ||
                              translationFallback ||
                              i === sections.length - 1
                            }
                            onClick={() => move(i, i + 1, "down")}
                          />
                          <IconButton
                            size="sm"
                            label={strings.sitesEditSection(
                              kindLabel(section.type),
                            )}
                            data-section-control="edit"
                            icon={<Pencil size={15} />}
                            disabled={
                              working || translationBusy || translationFallback
                            }
                            onClick={() =>
                              openForm({ kind: section.type, index: i })
                            }
                          />
                          {confirmDelete === i ? (
                            // The second, armed step of deleting: one more click
                            // removes the section; anything else disarms.
                            <Button
                              variant="danger"
                              size="sm"
                              disabled={
                                working ||
                                translationBusy ||
                                translationFallback
                              }
                              onClick={() => remove(i)}
                            >
                              {strings.sitesConfirmDelete}
                            </Button>
                          ) : (
                            <IconButton
                              size="sm"
                              label={strings.sitesDeleteSection(
                                kindLabel(section.type),
                              )}
                              data-section-control="delete"
                              icon={<Trash2 size={15} />}
                              disabled={
                                working ||
                                translationBusy ||
                                translationFallback
                              }
                              onClick={() => setConfirmDelete(i)}
                            />
                          )}
                        </div>
                      </li>
                    );
                  })}
                </ol>
              )}
            </div>

            <aside
              className={styles.previewPane}
              aria-label={strings.sitesPreview}
            >
              <div className={styles.previewBar}>
                <h2 className={styles.sectionTitle}>{strings.sitesPreview}</h2>
                <div className={styles.previewControls}>
                  {proposedPreviewHtml !== null && (
                    <div
                      className={styles.previewCompareToggle}
                      role="group"
                      aria-label={strings.sitesAiPreviewCompare}
                    >
                      <Button
                        variant="ghost"
                        className={
                          previewVersion === "before"
                            ? styles.previewCompareButtonActive
                            : undefined
                        }
                        aria-pressed={previewVersion === "before"}
                        onClick={() => setPreviewVersion("before")}
                      >
                        {strings.sitesAiPreviewBefore}
                      </Button>
                      <Button
                        variant="ghost"
                        className={
                          previewVersion === "after"
                            ? styles.previewCompareButtonActive
                            : undefined
                        }
                        aria-pressed={previewVersion === "after"}
                        onClick={() => setPreviewVersion("after")}
                      >
                        {strings.sitesAiPreviewAfter}
                      </Button>
                    </div>
                  )}
                  <div className={styles.previewToggle}>
                    <IconButton
                      size="sm"
                      label={strings.sitesPreviewDesktop}
                      icon={<Monitor size={15} />}
                      active={!previewMobile}
                      onClick={() => setPreviewMobile(false)}
                    />
                    <IconButton
                      size="sm"
                      label={strings.sitesPreviewMobile}
                      icon={<Smartphone size={15} />}
                      active={previewMobile}
                      onClick={() => setPreviewMobile(true)}
                    />
                  </div>
                </div>
              </div>
              {locale === null && (
                <p className={styles.previewEditHint}>{strings.sitesInlineTextHint}</p>
              )}
              {pageProtected && (
                <p className={styles.previewProtectedNote}>
                  <Lock size={13} aria-hidden="true" />
                  {strings.sitesPagePasswordPreviewNote}
                </p>
              )}
              {previewError !== null && <ErrorBanner message={previewError} />}
              <div
                className={
                  previewMobile
                    ? styles.previewViewportMobile
                    : styles.previewViewport
                }
              >
                {/* Sandboxed: scripts may run (the menu toggle), but the draft
                  document never touches this origin or navigates the app. */}
                <iframe
                  ref={previewFrame}
                  className={styles.previewFrame}
                  title={strings.sitesPreviewTitle}
                  sandbox="allow-scripts"
                  srcDoc={
                    previewVersion === "after" && proposedPreviewHtml !== null
                      ? proposedPreviewHtml
                      : (previewHtml ?? "")
                  }
                />
              </div>
            </aside>
          </div>
        </>
      )}

      {themeOpen && (
        <ThemeDialog
          siteId={siteId}
          onClose={() => setThemeOpen(false)}
          onApplied={() => {
            setThemeOpen(false);
            setPreviewEpoch((epoch) => epoch + 1);
          }}
        />
      )}

      {seoOpen && page !== null && (
        <PageSeoDialog
          siteId={siteId}
          page={page}
          onClose={() => setSeoOpen(false)}
          onSaved={(seoTitle, seoDescription) => {
            setPage({ ...page, seoTitle, seoDescription });
            setSeoOpen(false);
            setPreviewEpoch((epoch) => epoch + 1);
          }}
        />
      )}

      {picking && (
        <SectionPicker
          onPick={(kind) => {
            setPicking(false);
            openForm({ kind, index: null });
          }}
          onClose={() => setPicking(false)}
        />
      )}

      {form !== null && (
        <SectionFormDialog
          kind={form.kind}
          initial={form.index !== null ? sections[form.index] : undefined}
          busy={formBusy}
          error={formError}
          onClose={() => {
            setForm(null);
            setFormError(null);
          }}
          onSave={(section) => void save(form, section)}
          copyContext={
            locale !== null || form.index === null
              ? undefined
              : {
                  siteId,
                  pageId,
                  target: { index: form.index, type: form.kind },
                  onApplied: (envelope) => {
                    setSections(envelope.sections);
                    setForm(null);
                    setFormError(null);
                    setError(null);
                  },
                }
          }
        />
      )}
    </div>
  );
}
