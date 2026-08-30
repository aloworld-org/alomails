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
import { Check, LayoutTemplate, Plus } from "lucide-react";

import { strings } from "../i18n";
import { Spinner } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import type { SiteTemplate } from "./types";

/** What the gallery is showing: the blank start, or one shipped template. */
export type TemplateChoice =
  { kind: "blank" } | { kind: "template"; template: SiteTemplate };

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

  const chosen =
    (templates ?? []).find((template) => template.id === selected) ?? null;
  const chosenId = chosen?.id ?? null;
  // The slug the preview is showing; `null` means the template's home page.
  const shownPage =
    chosen?.pages.some((p) => p.slug === page) === true ? page : null;

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
          setPreviewError(
            sitesMessage(err, strings.sitesTemplatePreviewFailed),
          );
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api, chosenId, shownPage]);

  const choices: TemplateChoice[] = [
    { kind: "blank" },
    ...(templates ?? []).map((template) => ({
      kind: "template" as const,
      template,
    })),
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
    <section className="flex min-w-0 flex-col gap-3">
      <h3 className="m-0 text-sm font-semibold text-primary">
        {strings.sitesChooseTemplate}
      </h3>
      {catalogError !== null && (
        <p
          className="m-0 flex items-center gap-2 text-sm text-secondary"
          role="status"
        >
          {catalogError}
        </p>
      )}
      {templates === null ? (
        <p
          className="m-0 flex items-center gap-2 text-sm text-secondary"
          role="status"
        >
          <Spinner size={14} /> {strings.sitesTemplatesLoading}
        </p>
      ) : (
        <div
          className="grid grid-cols-2 gap-2 sm:grid-cols-3"
          role="radiogroup"
          aria-label={strings.sitesChooseTemplate}
        >
          {choices.map((choice, index) => {
            const key = optionKey(choice);
            const isSelected =
              choice.kind === "blank"
                ? selected === null
                : choice.template.id === selected;
            const name =
              choice.kind === "blank"
                ? strings.sitesBlankTemplate
                : choice.template.name;
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
                className={`group relative flex min-w-0 flex-col items-stretch gap-2 rounded-xl border p-2.5 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${isSelected ? "border-accent bg-accent-soft" : "border-default bg-surface hover:border-accent/50 hover:bg-raised"}`}
                onClick={() => choose(choice)}
                onKeyDown={(event) => onKeyDown(event, index)}
              >
                <span
                  className={`relative block aspect-[16/9] overflow-hidden rounded-lg border bg-surface ${choice.kind === "blank" ? "border-dashed border-strong" : "border-subtle"}`}
                  aria-hidden="true"
                >
                  {choice.kind === "blank" ? (
                    <span className="absolute inset-0 grid place-items-center">
                      <span className="grid size-9 place-items-center rounded-full bg-accent-soft text-accent">
                        <Plus className="size-4" />
                      </span>
                    </span>
                  ) : (
                    <>
                      <span className="absolute inset-x-0 top-0 flex h-4 items-center gap-1 border-b border-subtle bg-raised px-2">
                        <span className="size-1 rounded-full bg-accent" />
                        <span className="size-1 rounded-full bg-strong" />
                        <span className="size-1 rounded-full bg-strong" />
                      </span>
                      <span className="absolute inset-x-2 top-6 h-5 rounded bg-accent-soft" />
                      <span className="absolute left-2 right-1/3 top-13 h-1.5 rounded-full bg-strong" />
                      <span className="absolute left-2 right-1/2 top-16 h-1.5 rounded-full bg-subtle" />
                      <span className="absolute bottom-2 left-2 right-2 grid h-5 grid-cols-3 gap-1">
                        <span className="rounded bg-raised" />
                        <span className="rounded bg-raised" />
                        <span className="rounded bg-raised" />
                      </span>
                    </>
                  )}
                  {isSelected && (
                    <span className="absolute right-1.5 top-1.5 grid size-5 place-items-center rounded-full bg-accent text-on-accent">
                      <Check className="size-3" />
                    </span>
                  )}
                </span>
                <span className="flex min-w-0 items-center gap-2 text-sm font-semibold text-primary">
                  {choice.kind === "blank" && (
                    <LayoutTemplate
                      className="size-4 shrink-0 text-accent"
                      aria-hidden="true"
                    />
                  )}
                  <span className="truncate">{name}</span>
                </span>
                <span className="line-clamp-2 text-xs leading-relaxed text-secondary">
                  {summary}
                </span>
                {choice.kind === "template" && (
                  <span className="flex flex-wrap items-center gap-1">
                    <span className="text-xs text-tertiary">
                      {strings.sitesTemplatePageCount(
                        choice.template.pages.length,
                      )}
                    </span>
                    {choice.template.pages.map((templatePage) => (
                      <span
                        key={templatePage.path}
                        className="rounded-full bg-raised px-2 py-0.5 text-xs text-tertiary"
                      >
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
        <p className="m-0 text-xs leading-relaxed text-tertiary">
          {strings.sitesBlankPreviewNote}
        </p>
      ) : (
        <div className="flex flex-col gap-2">
          <div
            className="flex flex-wrap gap-1"
            aria-label={strings.sitesTemplatePreviewPages}
            role="group"
          >
            {chosen.pages.map((templatePage) => {
              const active = (shownPage ?? "") === templatePage.slug;
              return (
                <button
                  key={templatePage.path}
                  type="button"
                  aria-pressed={active}
                  className={`rounded-lg border px-3 py-1.5 text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${active ? "border-accent bg-accent-soft text-primary" : "border-default bg-surface text-secondary hover:bg-raised"}`}
                  onClick={() =>
                    setPage(templatePage.slug === "" ? null : templatePage.slug)
                  }
                >
                  {templatePage.title}
                </button>
              );
            })}
          </div>
          {previewError !== null && (
            <p
              className="m-0 flex items-center gap-2 text-sm text-secondary"
              role="status"
            >
              {previewError}
            </p>
          )}
          {previewHtml === null && previewError === null ? (
            <p
              className="m-0 flex items-center gap-2 text-sm text-secondary"
              role="status"
            >
              <Spinner size={14} /> {strings.sitesTemplatePreviewLoading}
            </p>
          ) : (
            previewHtml !== null && (
              <div className="rounded-xl border border-subtle bg-raised p-2">
                {/* Sandboxed and inert: the document may run its own menu
                    script, but it never reaches this origin and no click in it
                    can navigate anything. */}
                <iframe
                  className="block h-[min(46vh,26rem)] w-full rounded-lg border-0 bg-white pointer-events-none"
                  title={strings.sitesTemplatePreviewTitle(chosen.name)}
                  sandbox="allow-scripts"
                  srcDoc={previewHtml}
                />
              </div>
            )
          )}
          <p className="m-0 text-xs leading-relaxed text-tertiary">
            {strings.sitesTemplatePreviewNote}
          </p>
        </div>
      )}
    </section>
  );
}
