// The manual way to start a website (S2.11b): the shipped catalog as a
// gallery, beside — never behind — the AI description path. Nothing here is
// generated, so a workspace with no AI configured has a complete, visual first
// run rather than an apology and an empty page.
//
// Two rules shape the interaction. Choosing is ONE click: selecting a card
// immediately renders that template through the same renderer the public
// service uses, so what the gallery shows is what the site would serve rather
// than a screenshot that ages the moment a section changes. And choosing is
// reachable from the keyboard alone: the cards are a real radio group with
// roving focus, arrow keys, and Home/End, because the person who cannot use a
// pointer is exactly the person a "visual" screen tends to lose.
//
// The preview frame is a picture, not a browsing surface: it is sandboxed and
// takes no pointer events, and the template's other pages are reached through
// the tabs above it. A published page's internal links point at a real host,
// and letting one be clicked inside an opaque-origin frame would blank the
// preview for no gain.
import { useCallback, useEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { LayoutTemplate } from "lucide-react";

import { strings } from "../i18n";
import { Spinner } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import type { SiteTemplate } from "./types";
import styles from "./SitesModule.module.css";

/** What the gallery is showing: the blank start, or one shipped template. */
export type TemplateChoice = { kind: "blank" } | { kind: "template"; template: SiteTemplate };

/** The blank card sits first and is selected by default, so a person who
 *  ignores the gallery entirely still gets the old, working path. */
const BLANK_KEY = "";

function optionKey(choice: TemplateChoice): string {
  return choice.kind === "blank" ? BLANK_KEY : choice.template.id;
}

/** Moves the selection with the arrow keys, as a radio group is expected to
 *  behave: selection follows focus, and both ends wrap. */
function nextIndex(key: string, current: number, count: number): number | null {
  switch (key) {
    case "ArrowRight":
    case "ArrowDown":
      return (current + 1) % count;
    case "ArrowLeft":
    case "ArrowUp":
      return (current - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}

export function TemplateGallery({
  selected,
  onSelect,
}: {
  /** The chosen template id, or `null` for the blank start. */
  selected: string | null;
  onSelect: (choice: TemplateChoice) => void;
}) {
  const api = useSitesApi();
  const [templates, setTemplates] = useState<SiteTemplate[] | null>(null);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [page, setPage] = useState<string | null>(null);
  const [previewHtml, setPreviewHtml] = useState<string | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const buttons = useRef(new Map<string, HTMLButtonElement>());

  useEffect(() => {
    let cancelled = false;
    api.siteTemplates().then(
      (items) => {
        if (!cancelled) setTemplates(items);
      },
      (err: unknown) => {
        if (cancelled) return;
        // A catalog that will not load costs the gallery, not the screen: the
        // blank card below is still a complete way to create a website.
        setTemplates([]);
        setCatalogError(sitesMessage(err, strings.sitesTemplatesLoadFailed));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api]);

  const chosen = (templates ?? []).find((template) => template.id === selected) ?? null;
  const chosenId = chosen?.id ?? null;
  // The slug the preview is showing; `null` means the template's home page.
  const shownPage = chosen?.pages.some((p) => p.slug === page) === true ? page : null;

  useEffect(() => {
    if (chosenId === null) {
      setPreviewHtml(null);
      setPreviewError(null);
      return;
    }
    let cancelled = false;
    setPreviewHtml(null);
    setPreviewError(null);
    api.templatePreview(chosenId, shownPage ?? undefined).then(
      (html) => {
        if (!cancelled) setPreviewHtml(html);
      },
      (err: unknown) => {
        if (!cancelled) {
          setPreviewError(sitesMessage(err, strings.sitesTemplatePreviewFailed));
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api, chosenId, shownPage]);

  const choices: TemplateChoice[] = [
    { kind: "blank" },
    ...(templates ?? []).map((template) => ({ kind: "template" as const, template })),
  ];

  const choose = useCallback(
    (choice: TemplateChoice) => {
      setPage(null);
      onSelect(choice);
    },
    [onSelect],
  );

  function onKeyDown(event: KeyboardEvent<HTMLButtonElement>, index: number) {
    const target = nextIndex(event.key, index, choices.length);
    if (target === null) return;
    event.preventDefault();
    const choice = choices[target];
    if (choice === undefined) return;
    choose(choice);
    buttons.current.get(optionKey(choice))?.focus();
  }

  return (
    <section className={styles.templateGallery}>
      <h3 className={styles.templateGalleryTitle}>{strings.sitesChooseTemplate}</h3>
      {catalogError !== null && (
        <p className={styles.templateGalleryNotice} role="status">
          {catalogError}
        </p>
      )}
      {templates === null ? (
        <p className={styles.templateGalleryNotice} role="status">
          <Spinner size={14} /> {strings.sitesTemplatesLoading}
        </p>
      ) : (
        <div
          className={styles.templateGrid}
          role="radiogroup"
          aria-label={strings.sitesChooseTemplate}
        >
          {choices.map((choice, index) => {
            const key = optionKey(choice);
            const isSelected =
              choice.kind === "blank" ? selected === null : choice.template.id === selected;
            const name =
              choice.kind === "blank" ? strings.sitesBlankTemplate : choice.template.name;
            const summary =
              choice.kind === "blank"
                ? strings.sitesBlankTemplateSummary
                : choice.template.summary;
            return (
              <button
                key={key}
                type="button"
                role="radio"
                aria-checked={isSelected}
                tabIndex={isSelected ? 0 : -1}
                ref={(node) => {
                  if (node === null) buttons.current.delete(key);
                  else buttons.current.set(key, node);
                }}
                className={
                  isSelected
                    ? `${styles.templateCard} ${styles.templateCardActive}`
                    : styles.templateCard
                }
                onClick={() => choose(choice)}
                onKeyDown={(event) => onKeyDown(event, index)}
              >
                <span className={styles.templateCardName}>
                  {choice.kind === "blank" && <LayoutTemplate aria-hidden="true" />}
                  {name}
                </span>
                <span className={styles.templateCardSummary}>{summary}</span>
                {choice.kind === "template" && (
                  <span className={styles.templateCardPages}>
                    <span className={styles.templateCardCount}>
                      {strings.sitesTemplatePageCount(choice.template.pages.length)}
                    </span>
                    {choice.template.pages.map((templatePage) => (
                      <span key={templatePage.path} className={styles.templateCardPage}>
                        {templatePage.title}
                      </span>
                    ))}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      )}

      {chosen === null ? (
        <p className={styles.templatePreviewNote}>{strings.sitesBlankPreviewNote}</p>
      ) : (
        <div className={styles.templatePreview}>
          <div className={styles.templatePreviewTabs} aria-label={strings.sitesTemplatePreviewPages} role="group">
            {chosen.pages.map((templatePage) => {
              const active = (shownPage ?? "") === templatePage.slug;
              return (
                <button
                  key={templatePage.path}
                  type="button"
                  aria-pressed={active}
                  className={
                    active
                      ? `${styles.templatePreviewTab} ${styles.templatePreviewTabActive}`
                      : styles.templatePreviewTab
                  }
                  onClick={() => setPage(templatePage.slug === "" ? null : templatePage.slug)}
                >
                  {templatePage.title}
                </button>
              );
            })}
          </div>
          {previewError !== null && (
            <p className={styles.templateGalleryNotice} role="status">
              {previewError}
            </p>
          )}
          {previewHtml === null && previewError === null ? (
            <p className={styles.templateGalleryNotice} role="status">
              <Spinner size={14} /> {strings.sitesTemplatePreviewLoading}
            </p>
          ) : (
            previewHtml !== null && (
              <div className={styles.templatePreviewViewport}>
                {/* Sandboxed and inert: the document may run its own menu
                    script, but it never reaches this origin and no click in it
                    can navigate anything. */}
                <iframe
                  className={styles.templatePreviewFrame}
                  title={strings.sitesTemplatePreviewTitle(chosen.name)}
                  sandbox="allow-scripts"
                  srcDoc={previewHtml}
                />
              </div>
            )
          )}
          <p className={styles.templatePreviewNote}>{strings.sitesTemplatePreviewNote}</p>
        </div>
      )}
    </section>
  );
}
