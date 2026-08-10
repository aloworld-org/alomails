// The page editor: the stack of sections a page is built from. Every gesture
// — add (picker → prop form), edit, drag- or button-reorder, delete — is one
// call to the section ops of the edit API, and the stack always renders the
// canonical envelope the server answered, so what you see IS what is stored.
// There is no local dirty buffer to lose, and a refusal (422) points at the
// exact gesture that broke the rule.
import { useCallback, useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import {
  ArrowLeft,
  ChevronDown,
  ChevronUp,
  GripVertical,
  Layers,
  Monitor,
  Palette,
  Pencil,
  SearchCheck,
  Smartphone,
  Trash2,
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
  const api = useSitesApi();
  const [page, setPage] = useState<SitePageDetail | null>(null);
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

  const [previewHtml, setPreviewHtml] = useState<string | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewMobile, setPreviewMobile] = useState(false);
  const [proposedPreviewHtml, setProposedPreviewHtml] = useState<string | null>(null);
  const [previewVersion, setPreviewVersion] = useState<"before" | "after">("before");
  const [themeOpen, setThemeOpen] = useState(false);
  const [seoOpen, setSeoOpen] = useState(false);
  // Bumped when the theme changes — the preview document depends on the
  // site's theme, not only on this page's sections.
  const [previewEpoch, setPreviewEpoch] = useState(0);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const detail = await api.page(siteId, pageId);
      setPage(detail);
      setSections(detail.sections.sections);
      setError(null);
    } catch (err) {
      setError(sitesMessage(err, strings.sitesPageLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId, pageId]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    setProposedPreviewHtml(null);
    setPreviewVersion("before");
  }, [siteId, pageId]);

  // The live preview: the server renders the draft with the exact renderer
  // publishing uses. `sections` is always the envelope the last op answered,
  // so keying on it refreshes the pane after every successful save — and
  // not after a refused one.
  useEffect(() => {
    if (page === null) return undefined;
    let stale = false;
    api.pagePreview(siteId, pageId).then(
      (html) => {
        if (!stale) {
          setPreviewHtml(html);
          setPreviewError(null);
        }
      },
      (err: unknown) => {
        if (!stale) setPreviewError(sitesMessage(err, strings.sitesPreviewFailed));
      },
    );
    return () => {
      stale = true;
    };
  }, [api, siteId, pageId, page, sections, previewEpoch]);

  /** Runs one stack op and renders the envelope the server answered. */
  async function run(op: Promise<SectionsEnvelope>) {
    setWorking(true);
    setConfirmDelete(null);
    try {
      setSections((await op).sections);
      setError(null);
    } catch (err) {
      setError(sitesMessage(err, strings.sitesSaveFailed));
    } finally {
      setWorking(false);
    }
  }

  function move(from: number, to: number) {
    if (to < 0 || to >= sections.length || from === to) return;
    void run(api.moveSection(siteId, pageId, from, to));
  }

  /** The prop form's save: add for a fresh section, replace for a stored
   *  one. A refusal stays in the dialog with everything the user typed. */
  async function save(target: FormTarget, section: Section) {
    setFormBusy(true);
    try {
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
            {page.home && <span className={styles.badge}>{strings.sitesHomeBadge}</span>}
          </div>
        )}
        {loading && <Spinner size={16} />}
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {page !== null && (
        <div className={styles.editorLayout}>
          <div className={styles.stackPane}>
            <div className={styles.sectionBar}>
              <h2 className={styles.sectionTitle}>{strings.sitesSections}</h2>
              <div className={styles.sectionBarActions}>
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<SearchCheck size={14} />}
                  onClick={() => setSeoOpen(true)}
                >
                  {strings.sitesSeoAction}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<Palette size={14} />}
                  onClick={() => setThemeOpen(true)}
                >
                  {strings.sitesTheme}
                </Button>
                <Button size="sm" onClick={() => setPicking(true)} disabled={working}>
                  {strings.sitesAddSection}
                </Button>
              </div>
            </div>

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

            {empty && !loading ? (
              <EmptyState
                Icon={Layers}
                title={strings.sitesNoSectionsTitle}
                body={strings.sitesNoSectionsBody}
                cta={strings.sitesAddSection}
                onCta={() => setPicking(true)}
              />
            ) : (
              <ol className={styles.stack}>
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
                        <span className={styles.cardKind}>{kindLabel(section.type)}</span>
                        {summary !== "" && <span className={styles.cardSummary}>{summary}</span>}
                      </div>
                      <div className={styles.cardActions}>
                        <IconButton
                          size="sm"
                          label={strings.sitesMoveUp}
                          icon={<ChevronUp size={15} />}
                          disabled={working || i === 0}
                          onClick={() => move(i, i - 1)}
                        />
                        <IconButton
                          size="sm"
                          label={strings.sitesMoveDown}
                          icon={<ChevronDown size={15} />}
                          disabled={working || i === sections.length - 1}
                          onClick={() => move(i, i + 1)}
                        />
                        <IconButton
                          size="sm"
                          label={strings.sitesEditSection}
                          icon={<Pencil size={15} />}
                          disabled={working}
                          onClick={() => openForm({ kind: section.type, index: i })}
                        />
                        {confirmDelete === i ? (
                          // The second, armed step of deleting: one more click
                          // removes the section; anything else disarms.
                          <Button
                            variant="danger"
                            size="sm"
                            disabled={working}
                            onClick={() => void run(api.removeSection(siteId, pageId, i))}
                          >
                            {strings.sitesConfirmDelete}
                          </Button>
                        ) : (
                          <IconButton
                            size="sm"
                            label={strings.sitesDeleteSection}
                            icon={<Trash2 size={15} />}
                            disabled={working}
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

          <aside className={styles.previewPane} aria-label={strings.sitesPreview}>
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
            {previewError !== null && <ErrorBanner message={previewError} />}
            <div
              className={
                previewMobile ? styles.previewViewportMobile : styles.previewViewport
              }
            >
              {/* Sandboxed: scripts may run (the menu toggle), but the draft
                  document never touches this origin or navigates the app. */}
              <iframe
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
        />
      )}
    </div>
  );
}
