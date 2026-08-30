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
//
// A section is moved where it lives too (S3.01b): dragging one in the preview
// reflows the page under the pointer and reports where it landed, and `move`
// below — the same function the stack's own buttons call — sends it. So the
// gesture on the page, the buttons in the stack and the assistant's
// `reorder_section` are three ways to ask for one change, all of which come
// back as the server's canonical envelope and all of which one ⌘Z takes back.
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import type {
  CSSProperties,
  KeyboardEvent as ReactKeyboardEvent,
  PointerEvent as ReactPointerEvent,
} from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import {
  ArrowLeft,
  Copy,
  ChevronDown,
  ChevronUp,
  GripVertical,
  Eye,
  Layers,
  LayoutGrid,
  Lock,
  Monitor,
  Palette,
  Pencil,
  Plus,
  Redo2,
  SearchCheck,
  Smartphone,
  Trash2,
  Undo2,
  X,
} from "lucide-react";

import { strings } from "../i18n";
import { Button, IconButton, Modal, Spinner } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import { kindLabel, layoutValueLabel, sectionSummary } from "./sectionInfo";
import { SectionFormDialog } from "./SectionForm";
import { SectionPalette } from "./SectionPalette";
import { ThemeDialog } from "./ThemeDialog";
import { PageSeoDialog } from "./PageSeoDialog";
import { PageAiEditPanel } from "./PageAiEditPanel";
import { PagePassword } from "./PagePassword";
import {
  keyTarget,
  readTextEditMessage,
  textEditEnvelope,
  textEditOperation,
} from "./inlineText";
import {
  emptyEditHistory,
  invertEdit,
  recordEdit,
  redoEdit,
  undoEdit,
  type EditHistory,
  type EditStep,
} from "./editHistory";
import {
  moveDestination,
  readSectionMoveMessage,
  withSectionMoved,
} from "./sectionMove";
import {
  controlsFor,
  currentValue,
  layoutOperation,
  readLayoutStepMessage,
  readSectionLayouts,
  steppedValue,
  type SectionLayouts,
} from "./sectionLayout";
import { SectionLayoutControls } from "./SectionLayoutControls";
import { ErrorBanner } from "./parts";
import { insertionIndex, type PaletteTile } from "./palette";
import type { Section, SectionKind, SectionsEnvelope } from "./sections";
import type { SitePageDetail } from "./types";
import styles from "./SitesModule.module.css";

/** Which section the prop form is editing: a fresh one of `kind` when
 *  `index` is null, the stored one at `index` otherwise. `insertAt` is where a
 *  fresh one lands — the position the palette was dropped at, or the end. */
interface FormTarget {
  kind: SectionKind;
  index: number | null;
  insertAt?: number;
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
  // The palette pulls the caret onto its first tile once its seeded tiles
  // arrive — which can land AFTER a close was asked for (Escape before the
  // server answered). The tile then unmounts under the caret and jsdom and
  // browsers alike drop focus on the document. When the palette leaves, a
  // caret it took down with it goes back to the control that opened it.
  useEffect(() => {
    if (!picking) return undefined;
    return () => {
      if (
        document.activeElement === null ||
        document.activeElement === document.body
      ) {
        document
          .querySelector<HTMLButtonElement>("[data-add-section]")
          ?.focus();
      }
    };
  }, [picking]);
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
  const [history, setHistory] = useState<EditHistory>(emptyEditHistory);
  const [textNotice, setTextNotice] = useState("");
  const [previewHtml, setPreviewHtml] = useState<string | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewOpen, setPreviewOpen] = useState(false);
  const [previewMobile, setPreviewMobile] = useState(false);
  const [sectionsPanelWidth, setSectionsPanelWidth] = useState(34);
  const [resizingWorkspace, setResizingWorkspace] = useState(false);
  const workspaceRef = useRef<HTMLDivElement | null>(null);
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
  // What each section type may be resized to (ADR 0042). Declared by the
  // server and read once: the editor offers exactly what is in here, so a
  // ratio it has never been told about is one it cannot produce. An older
  // server, or a request that fails, simply means no resize affordance.
  const [layouts, setLayouts] = useState<SectionLayouts>({});
  const layoutsRef = useRef<SectionLayouts>({});

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

  useEffect(() => {
    let live = true;
    void api
      .config()
      .then((config) => {
        if (live) setLayouts(readSectionLayouts(config.sectionLayouts));
      })
      .catch(() => {
        /* No declaration, no handles — the rest of the editor is unaffected. */
      });
    return () => {
      live = false;
    };
  }, [api]);

  useLayoutEffect(() => {
    layoutsRef.current = layouts;
  }, [layouts]);

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
    setHistory(emptyEditHistory);
    setTextNotice("");
  }, [siteId, pageId, locale]);

  // The live preview: the server renders the draft with the exact renderer
  // publishing uses. `sections` is always the envelope the last op answered,
  // so keying on it refreshes the pane after every successful save — and
  // not after a refused one.
  useEffect(() => {
    if (page === null || !previewOpen) return undefined;
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
  }, [api, siteId, pageId, locale, page, sections, previewEpoch, previewOpen]);

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

  /** Runs one stack op and renders the envelope the server answered. Answers
   *  whether it was stored, which is what decides if it joins the history. */
  async function run(op: Promise<SectionsEnvelope>): Promise<boolean> {
    setWorking(true);
    setConfirmDelete(null);
    try {
      setSections((await op).sections);
      setError(null);
      return true;
    } catch (err) {
      focusAfterMove.current = null;
      setError(sitesMessage(err, strings.sitesSaveFailed));
      return false;
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

  /** The one move on this screen.
   *
   *  A stack button, a section dragged on the page and the inverse of either
   *  under ⌘Z all arrive here, so there is a single set of rules about what a
   *  move announces, what it focuses and what it stores — and a single request
   *  behind all of them.
   *
   *  `control` refocuses a stack button after the list is replaced;
   *  `inPreview` asks the preview frame to put focus back on the section
   *  itself, which is what a keyboard move made on the page needs, since
   *  applying it re-renders the whole document. `replay` is set when undo or
   *  redo is re-applying a known step — the one case that must not push a new
   *  entry onto the history. */
  async function move(
    from: number,
    to: number,
    options: { control?: string; inPreview?: boolean; replay?: boolean } = {},
  ): Promise<boolean> {
    // Against the ref, not the render's copy: a message from the preview frame
    // arrives whenever the person decides, and has to be answered against the
    // page as it is now.
    const current = sectionsRef.current;
    if (from < 0 || from >= current.length) return false;
    if (to < 0 || to >= current.length || from === to) return false;
    const moving = current[from];
    if (moving !== undefined) {
      // Announced as well as done: a reorder is invisible to a reader who
      // cannot see the stack reflow.
      setMoveNotice(
        strings.sitesSectionMoved(
          kindLabel(moving.type),
          to + 1,
          current.length,
        ),
      );
    }
    if (options.control !== undefined)
      focusAfterMove.current = {
        index: to,
        control: options.control,
        before: current,
      };
    if (options.inPreview === true) focusInPreview.current = to;
    let stored: boolean;
    if (locale !== null) {
      const reordered = withSectionMoved(current, from, to);
      if (reordered === null) return false;
      stored = await saveLocalized(reordered);
    } else {
      stored = await run(api.moveSection(siteId, pageId, from, to));
    }
    if (stored && options.replay !== true && locale === null) {
      setHistory((entries) => recordEdit(entries, { kind: "move", from, to }));
    }
    return stored;
  }

  /** Which section the preview frame should put focus on after the document
   *  it is showing has been replaced. Null unless a move was made inside it. */
  const focusInPreview = useRef<number | null>(null);

  /** The editor chrome the preview document cannot write for itself: what each
   *  section is called, and where focus goes after a move.
   *
   *  These are the *editor's* words, so they are in the language of the person
   *  editing — which is not necessarily the language of the site being edited,
   *  and is why they are posted in rather than rendered by `alo-sites`. The
   *  target origin can only be `"*"`: the document is a sandboxed `srcdoc` and
   *  has no origin to name. Nothing secret travels — a section kind and a
   *  position, both of which are already on the screen the message is sent
   *  from. */
  function postPreviewChrome() {
    const frame = previewFrame.current?.contentWindow ?? null;
    if (frame === null || locale !== null) return;
    const current = sectionsRef.current;
    frame.postMessage(
      {
        alo: "site-edit-chrome",
        labels: current.map((section, index) =>
          strings.sitesSectionOnPage(
            kindLabel(section.type),
            index + 1,
            current.length,
          ),
        ),
        focus: focusInPreview.current,
      },
      "*",
    );
    focusInPreview.current = null;
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
    async (key: string, text: string, replay = false): Promise<boolean> => {
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
        if (!replay) {
          setHistory((entries) =>
            recordEdit(entries, {
              kind: "text",
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

  /** Resizes the section at `index` to another of the values its type
   *  declares (ADR 0042, S3.01c).
   *
   *  The value is checked against the *server's* declaration before anything
   *  is sent, and travels as the same `set_prop` operation an approved AI
   *  proposal carries — so a ratio and a rewritten headline are one kind of
   *  change, with one diff, one door and one undo.
   *
   *  `replay` is set when undo or redo is re-applying a known step, the one
   *  case that must not push a new entry onto the history. */
  const applyLayout = useCallback(
    async (
      index: number,
      key: string,
      value: string,
      replay = false,
    ): Promise<boolean> => {
      const current = sectionsRef.current;
      const section = current[index];
      const control = controlsFor(layoutsRef.current, section).find(
        (c) => c.key === key,
      );
      const operation = layoutOperation(
        current,
        layoutsRef.current,
        index,
        key,
        value,
      );
      if (
        section === undefined ||
        control === undefined ||
        operation === null
      ) {
        setError(strings.sitesInlineTextStale);
        setPreviewEpoch((epoch) => epoch + 1);
        return false;
      }
      const before = currentValue(section, control);
      if (before === value) return true;
      setWorking(true);
      try {
        const envelope = await api.applyPageEdit(
          siteId,
          pageId,
          textEditEnvelope(operation),
        );
        setSections(envelope.sections);
        setError(null);
        if (!replay) {
          setHistory((entries) =>
            recordEdit(entries, {
              kind: "layout",
              index,
              key,
              before,
              after: value,
            }),
          );
        }
        setTextNotice(
          strings.sitesSectionResized(
            kindLabel(section.type),
            layoutValueLabel(key, value),
          ),
        );
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

  /** One place along the first control the section declares — the keyboard
   *  gesture on the page. The frame reports a *direction*; which value that
   *  is, is resolved here against the declaration, because the frame is
   *  deliberately never told what the values are. */
  function stepLayout(index: number, step: -1 | 1) {
    const section = sectionsRef.current[index];
    const control = controlsFor(layoutsRef.current, section)[0];
    if (section === undefined || control === undefined) return;
    const next = steppedValue(control, currentValue(section, control), step);
    if (next === null) return;
    focusInPreview.current = index;
    void applyLayout(index, control.key, next);
  }

  // The frame's half of direct manipulation. Its document has an opaque
  // origin, so `event.origin` proves nothing and the sender is proven
  // instead: only this editor's own preview window is listened to.
  //
  // No dependency array: the move handler must answer against the page as it
  // is at the moment the gesture lands, and re-subscribing on every render is
  // how a handler stays current without a second copy of the state to keep in
  // sync.
  useEffect(() => {
    if (locale !== null) return undefined;
    function onMessage(event: MessageEvent) {
      const frame = previewFrame.current;
      const own = frame !== null && event.source === frame.contentWindow;
      const edit = readTextEditMessage(event.data, own);
      if (edit !== null) {
        void applyInlineText(edit.key, edit.text);
        return;
      }
      const resized = readLayoutStepMessage(event.data, own);
      if (resized !== null) {
        stepLayout(resized.index, resized.step);
        return;
      }
      const moved = readSectionMoveMessage(event.data, own);
      if (moved === null) return;
      const current = sectionsRef.current;
      if (current[moved.from] === undefined) {
        // The page moved under the gesture: refuse it rather than aim it at
        // whatever now sits at that position, and show the truth again.
        setError(strings.sitesInlineTextStale);
        setPreviewEpoch((epoch) => epoch + 1);
        return;
      }
      const to = moveDestination(current.length, moved.from, moved.before);
      if (to === null) return;
      void move(moved.from, to, { inPreview: true });
    }
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  });

  /** Takes back the last change, whichever gesture made it, by applying its
   *  inverse through that gesture's own door. */
  async function undoEdits() {
    const undone = undoEdit(history);
    if (undone === null) return;
    if (!(await applyStep(invertEdit(undone.step)))) return;
    setHistory(undone.history);
    if (undone.step.kind === "text")
      setTextNotice(strings.sitesInlineTextUndone);
  }

  /** Puts back the last change undo took away. */
  async function redoEdits() {
    const redone = redoEdit(history);
    if (redone === null) return;
    if (!(await applyStep(redone.step))) return;
    setHistory(redone.history);
    if (redone.step.kind === "text")
      setTextNotice(strings.sitesInlineTextRedone);
  }

  /** Re-applies one known step without recording it again. A move announces
   *  itself through the same live region as any other move, so only text
   *  needs a word from the caller. */
  function applyStep(step: EditStep): Promise<boolean> {
    switch (step.kind) {
      case "text":
        return applyInlineText(step.key, step.after, true);
      case "layout":
        return applyLayout(step.index, step.key, step.after, true);
      default:
        return move(step.from, step.to, { replay: true });
    }
  }

  // ⌘Z / Ctrl+Z, with shift to redo. Keys pressed inside the preview stay
  // inside the frame, so this never competes with typing on the page; a field
  // in the app around it keeps its own native undo.
  useEffect(() => {
    if (locale !== null) return undefined;
    function onKeyDown(event: KeyboardEvent) {
      if (
        !(event.metaKey || event.ctrlKey) ||
        event.key.toLowerCase() !== "z"
      ) {
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
      void (event.shiftKey ? redoEdits() : undoEdits());
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
        if (target.index === null) {
          nextSections.splice(
            target.insertAt ?? nextSections.length,
            0,
            section,
          );
        } else nextSections[target.index] = section;
        const saved = await saveLocalized(nextSections);
        if (!saved) return;
        setForm(null);
        setFormError(null);
        return;
      }
      const envelope =
        target.index === null
          ? await api.addSection(siteId, pageId, section, target.insertAt)
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

  /** Puts one palette tile onto the page at `at` (ADR 0042 §4, S3.01d).
   *
   *  A seeded tile is stored exactly as the palette showed it — every word in
   *  it is already this tenant's — through the same `POST …/sections` the prop
   *  form saves through, so a dragged block, a typed block and one the
   *  assistant proposes are one kind of change with one validation behind
   *  them. A tile the palette could not fill from the website opens that form
   *  at the same position instead, which is the pre-palette behaviour and the
   *  reason no block is ever unreachable.
   *
   *  Where it landed is announced and focused: a stack that grows in silence
   *  is invisible to a reader who cannot see it reflow, and a caret left on a
   *  palette tile makes the next keystroke land somewhere surprising. */
  async function addTile(tile: PaletteTile, at: number) {
    const current = sectionsRef.current;
    const index = insertionIndex(current.length, at);
    if (tile.section === null) {
      openForm({ kind: tile.kind, index: null, insertAt: index });
      return;
    }
    const section = tile.section;
    let stored: boolean;
    if (locale !== null) {
      stored = await saveLocalized([
        ...current.slice(0, index),
        section,
        ...current.slice(index),
      ]);
    } else {
      focusAfterMove.current = { index, control: "edit", before: current };
      stored = await run(api.addSection(siteId, pageId, section, index));
    }
    if (stored) {
      setMoveNotice(
        strings.sitesSectionAdded(
          kindLabel(tile.kind),
          index + 1,
          current.length + 1,
        ),
      );
    }
  }

  /** Closes the palette and puts the caret back on the control that opened it
   *  — the disclosure contract every other panel on this screen keeps. */
  function closePalette() {
    setPicking(false);
    document.querySelector<HTMLButtonElement>("[data-add-section]")?.focus();
  }

  const empty = sections.length === 0;
  const requestedLanguage = locale === null ? defaultLocale : locale;

  function setWorkspaceWidthFromPointer(clientX: number) {
    const bounds = workspaceRef.current?.getBoundingClientRect();
    if (bounds === undefined || bounds.width === 0) return;
    const next = ((clientX - bounds.left) / bounds.width) * 100;
    setSectionsPanelWidth(Math.min(65, Math.max(25, next)));
  }

  function startWorkspaceResize(event: ReactPointerEvent<HTMLButtonElement>) {
    event.currentTarget.setPointerCapture(event.pointerId);
    setResizingWorkspace(true);
    setWorkspaceWidthFromPointer(event.clientX);
  }

  function moveWorkspaceResize(event: ReactPointerEvent<HTMLButtonElement>) {
    if (!resizingWorkspace) return;
    setWorkspaceWidthFromPointer(event.clientX);
  }

  function finishWorkspaceResize(event: ReactPointerEvent<HTMLButtonElement>) {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setResizingWorkspace(false);
  }

  function resizeWorkspaceWithKeyboard(
    event: ReactKeyboardEvent<HTMLButtonElement>,
  ) {
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      setSectionsPanelWidth((width) => Math.max(25, width - 4));
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      setSectionsPanelWidth((width) => Math.min(65, width + 4));
    } else if (event.key === "Home") {
      event.preventDefault();
      setSectionsPanelWidth(25);
    } else if (event.key === "End") {
      event.preventDefault();
      setSectionsPanelWidth(65);
    }
  }

  async function copyFallback() {
    if (page === null || locale === null) return;
    await saveLocalized(sections);
  }

  return (
    <div className="flex min-h-full flex-col bg-bg-app px-4 py-4 text-text-primary sm:px-6 lg:px-8">
      <header className="mx-auto flex w-full max-w-[1600px] flex-wrap items-center gap-3 pb-4">
        <Link
          to={`/sites/${siteId}`}
          className="inline-flex min-h-10 items-center gap-2 rounded-xl px-3 font-semibold text-text-primary no-underline transition-colors hover:bg-surface-raised"
        >
          <ArrowLeft size={16} aria-hidden="true" />
          {strings.sitesBackToSite}
        </Link>
        {page !== null && (
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <h1 className="truncate text-xl font-bold tracking-tight sm:text-2xl">
                {page.title}
              </h1>
              <span className="font-mono text-sm text-text-secondary">
                /{page.slug}
              </span>
              {page.home && (
                <span className="rounded-full bg-accent-soft px-2.5 py-1 text-xs font-semibold text-accent">
                  {strings.sitesHomeBadge}
                </span>
              )}
            </div>
          </div>
        )}
        {loading && <Spinner size={16} />}
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {page !== null && (
        <>
          <section className="mx-auto w-full max-w-[1600px] overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm">
            <div className="flex flex-wrap items-center gap-2 px-4 py-3 sm:px-5">
              {!empty && (
                <Button
                  size="sm"
                  icon={<Plus size={16} />}
                  data-add-section=""
                  aria-expanded={picking}
                  onClick={() => (picking ? closePalette() : setPicking(true))}
                  disabled={working || translationBusy || translationFallback}
                >
                  {strings.sitesAddSection}
                </Button>
              )}
              {locale === null && (
                <>
                  <IconButton
                    size="sm"
                    label={strings.sitesUndoEdit}
                    icon={<Undo2 size={15} />}
                    disabled={working || history.past.length === 0}
                    onClick={() => void undoEdits()}
                  />
                  <IconButton
                    size="sm"
                    label={strings.sitesRedoEdit}
                    icon={<Redo2 size={15} />}
                    disabled={working || history.future.length === 0}
                    onClick={() => void redoEdits()}
                  />
                </>
              )}
              <span
                className="mx-1 hidden h-6 w-px bg-border-subtle sm:block"
                aria-hidden="true"
              />
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
              {locale === null && (
                <PageAiEditPanel
                  navigation
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
              <PagePassword
                navigation
                siteId={siteId}
                pageId={pageId}
                multilingual={enabledLocales.length > 1}
                onChange={setPageProtected}
              />
              <Button
                variant="ghost"
                size="sm"
                icon={<Eye size={14} />}
                className={
                  previewOpen
                    ? "!border-accent/20 !bg-accent-soft !text-accent"
                    : undefined
                }
                aria-label={
                  previewOpen
                    ? strings.sitesHidePreview
                    : strings.sitesShowPreview
                }
                aria-pressed={previewOpen}
                onClick={() => setPreviewOpen((open) => !open)}
              >
                {strings.sitesPreview}
              </Button>
              <nav
                className="ml-auto flex items-center gap-2"
                aria-label={strings.sitesLanguagesLabel}
              >
                <span className="hidden text-sm font-medium text-secondary sm:inline">
                  {strings.sitesEditingLanguage}
                </span>
                <div className="flex items-center gap-1 rounded-xl bg-raised p-1">
                  {enabledLocales.map((enabledLocale) => (
                    <Button
                      key={enabledLocale}
                      variant="ghost"
                      size="sm"
                      className={
                        requestedLanguage === enabledLocale
                          ? "bg-surface text-accent shadow-sm"
                          : "text-secondary"
                      }
                      aria-pressed={requestedLanguage === enabledLocale}
                      onClick={() => chooseLocale(enabledLocale)}
                    >
                      {enabledLocale.toUpperCase()}
                    </Button>
                  ))}
                </div>
              </nav>
            </div>
          </section>

          {picking && (
            <Modal
              title={strings.sitesPaletteTitle}
              wide="extra"
              tall
              icon={<LayoutGrid size="var(--icon-size-control)" />}
              actions={
                <IconButton
                  label={strings.close}
                  icon={<X size="var(--icon-size-control)" />}
                  onClick={closePalette}
                />
              }
              onClose={closePalette}
            >
              <SectionPalette
                siteId={siteId}
                pageId={pageId}
                seeded={locale === null}
                sections={sections}
                busy={working || translationBusy}
                onChoose={(tile, index) => {
                  // A picker is a single-choice task. Return the page to the
                  // workspace as soon as a block is chosen; an unseeded block
                  // opens its focused details dialog next.
                  setPicking(false);
                  void addTile(tile, index);
                }}
              />
            </Modal>
          )}

          {locale !== null && translationFallback && (
            <section
              className="mx-auto mt-4 flex w-full max-w-[1600px] flex-wrap items-center justify-between gap-4 rounded-2xl border border-accent/20 bg-accent-soft px-5 py-4"
              aria-live="polite"
            >
              <div>
                <h2 className="font-semibold text-text-primary">
                  {strings.sitesTranslationMissingTitle(locale.toUpperCase())}
                </h2>
                <p className="mt-1 max-w-3xl text-sm text-text-secondary">
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
            <details className="mx-auto mt-4 w-full max-w-[1600px] rounded-2xl border border-subtle bg-surface shadow-sm">
              <summary className="cursor-pointer list-none px-5 py-4 marker:content-none">
                <h2 className="font-semibold text-text-primary">
                  {strings.sitesTranslationDetails}
                </h2>
                <p className="mt-1 text-sm text-text-secondary">
                  {strings.sitesTranslationDetailsHint(locale.toUpperCase())}
                </p>
              </summary>
              <div className="grid gap-4 border-t border-subtle px-5 py-5 sm:grid-cols-2">
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
                <div className="sm:col-span-2 flex justify-end">
                  <Button
                    size="sm"
                    disabled={translationBusy}
                    onClick={() => void saveIdentity()}
                  >
                    {strings.sitesSaveTranslation}
                  </Button>
                </div>
              </div>
            </details>
          )}

          {translationError !== null && (
            <ErrorBanner message={translationError} />
          )}

          <div
            ref={workspaceRef}
            style={
              {
                "--sections-panel-width": `${sectionsPanelWidth}%`,
              } as CSSProperties
            }
            className={
              previewOpen
                ? "mx-auto mt-4 grid w-full max-w-[1600px] min-w-0 flex-1 gap-4 xl:grid-cols-[minmax(320px,var(--sections-panel-width))_12px_minmax(0,1fr)] xl:gap-0"
                : "mx-auto mt-4 grid w-full max-w-[1600px] min-w-0 flex-1"
            }
          >
            <section
              className={`min-w-0 overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm ${
                empty && !loading ? "flex h-full flex-col" : "self-start"
              }`}
              aria-labelledby="sites-sections-title"
            >
              <div className="flex min-h-16 items-center justify-between gap-3 border-b border-subtle px-4 py-3 sm:px-5">
                <div>
                  <h2
                    id="sites-sections-title"
                    className="font-semibold text-text-primary"
                  >
                    {strings.sitesSections}
                  </h2>
                  <p className="mt-1 text-sm text-text-secondary">
                    {sections.length} {strings.sitesSections.toLowerCase()}
                  </p>
                </div>
              </div>

              {empty && !loading && !picking && (
                <div className="flex flex-1 flex-col items-center justify-center px-6 py-12 text-center">
                  <span
                    className="inline-flex size-12 items-center justify-center rounded-2xl bg-accent-soft text-accent"
                    aria-hidden="true"
                  >
                    <Layers size={24} />
                  </span>
                  <h3 className="mt-4 text-lg font-semibold text-primary">
                    {strings.sitesNoSectionsTitle}
                  </h3>
                  <p className="mt-1 max-w-sm text-sm leading-6 text-secondary">
                    {strings.sitesNoSectionsBody}
                  </p>
                  <Button
                    className="mt-5"
                    icon={<Plus size={16} />}
                    data-add-section=""
                    aria-expanded={picking}
                    disabled={working || translationBusy || translationFallback}
                    onClick={() => setPicking(true)}
                  >
                    {strings.sitesAddSection}
                  </Button>
                </div>
              )}

              {/* Reordering is the one edit on this screen with no visible
                  result outside the stack itself. */}
              <p className={styles.srOnly} role="status">
                {moveNotice}
                {textNotice !== "" && ` ${textNotice}`}
              </p>

              {!empty && (
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
                          if (dragFrom !== null) void move(dragFrom, i);
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
                          {locale === null && (
                            <SectionLayoutControls
                              section={section}
                              index={i}
                              layouts={layouts}
                              disabled={working || translationBusy}
                              onChoose={(at, key, value) => {
                                void applyLayout(at, key, value);
                              }}
                            />
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
                            onClick={() =>
                              void move(i, i - 1, { control: "up" })
                            }
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
                            onClick={() =>
                              void move(i, i + 1, { control: "down" })
                            }
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

            </section>

            {previewOpen && (
              <div
                className="relative z-10 hidden items-start justify-center xl:flex"
                aria-hidden="false"
              >
                <button
                  type="button"
                  role="separator"
                  aria-label={strings.sitesResizeWorkspace}
                  aria-orientation="vertical"
                  aria-valuemin={25}
                  aria-valuemax={65}
                  aria-valuenow={Math.round(sectionsPanelWidth)}
                  className="group flex h-full min-h-16 w-10 shrink-0 touch-none cursor-col-resize items-start justify-center rounded-lg pt-4 focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/15"
                  onPointerDown={startWorkspaceResize}
                  onPointerMove={moveWorkspaceResize}
                  onPointerUp={finishWorkspaceResize}
                  onPointerCancel={finishWorkspaceResize}
                  onKeyDown={resizeWorkspaceWithKeyboard}
                  onDoubleClick={() => setSectionsPanelWidth(34)}
                >
                  <span className="sticky top-20 inline-flex h-12 w-6 items-center justify-center rounded-full border border-subtle bg-surface text-tertiary shadow-sm transition-colors group-hover:border-accent/30 group-hover:text-accent group-focus-visible:border-accent/30 group-focus-visible:text-accent">
                    <GripVertical size={15} aria-hidden="true" />
                  </span>
                </button>
              </div>
            )}

            {previewOpen && (
              <aside
                className="min-w-0 self-start overflow-hidden rounded-2xl border border-subtle bg-raised/60 shadow-sm xl:sticky xl:top-4"
                aria-label={strings.sitesPreview}
              >
                <div className="flex min-h-16 flex-wrap items-center justify-between gap-3 border-b border-subtle bg-surface px-4 py-3 sm:px-5">
                  <div>
                    <h2 className="font-semibold text-text-primary">
                      {strings.sitesPreview}
                    </h2>
                    <p className="mt-0.5 text-xs text-text-secondary">
                      /{page.slug}
                    </p>
                  </div>
                  <div className="flex flex-wrap items-center gap-2">
                    {proposedPreviewHtml !== null && (
                      <div
                        className="flex items-center gap-1 rounded-xl bg-raised p-1"
                        role="group"
                        aria-label={strings.sitesAiPreviewCompare}
                      >
                        <Button
                          variant="ghost"
                          className={
                            previewVersion === "before"
                              ? "!bg-surface !text-primary shadow-sm"
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
                              ? "!bg-surface !text-primary shadow-sm"
                              : undefined
                          }
                          aria-pressed={previewVersion === "after"}
                          onClick={() => setPreviewVersion("after")}
                        >
                          {strings.sitesAiPreviewAfter}
                        </Button>
                      </div>
                    )}
                    <div className="flex items-center gap-1 rounded-xl bg-raised p-1">
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
                {locale === null && !empty && (
                  <p className="flex items-center gap-2 border-b border-subtle bg-surface px-4 py-2 text-xs leading-5 text-secondary sm:px-5">
                    <Pencil
                      size={13}
                      className="shrink-0 text-accent"
                      aria-hidden="true"
                    />
                    {strings.sitesInlineTextHint}
                  </p>
                )}
                {pageProtected && (
                  <p className="flex items-center gap-2 border-b border-subtle bg-surface px-4 py-2 text-xs text-secondary sm:px-5">
                    <Lock size={13} aria-hidden="true" />
                    {strings.sitesPagePasswordPreviewNote}
                  </p>
                )}
                {previewError !== null && (
                  <ErrorBanner message={previewError} />
                )}
                <div className="p-3 sm:p-5">
                  {empty && !loading ? (
                    <div className="flex min-h-[32rem] flex-col overflow-hidden rounded-xl border border-default bg-surface shadow-sm">
                      <div
                        className="flex h-12 items-center gap-2 border-b border-subtle px-4"
                        aria-hidden="true"
                      >
                        <span className="size-2 rounded-full bg-border-default" />
                        <span className="size-2 rounded-full bg-border-default" />
                        <span className="size-2 rounded-full bg-border-default" />
                        <span className="ml-3 h-2 w-24 rounded-full bg-raised" />
                      </div>
                      <div className="flex flex-1 flex-col items-center justify-center px-8 text-center">
                        <span
                          className="inline-flex size-14 items-center justify-center rounded-2xl bg-accent-soft text-accent"
                          aria-hidden="true"
                        >
                          <Layers size={27} />
                        </span>
                        <h3 className="mt-4 text-lg font-semibold text-primary">
                          {strings.sitesNoSectionsTitle}
                        </h3>
                        <p className="mt-1 max-w-sm text-sm leading-6 text-secondary">
                          {strings.sitesNoSectionsBody}
                        </p>
                      </div>
                    </div>
                  ) : (
                    <div
                      className={
                        previewMobile
                          ? "mx-auto max-w-[391px] rounded-xl bg-sunken p-2"
                          : "rounded-xl bg-sunken p-2"
                      }
                    >
                      {/* Sandboxed: scripts may run (the menu toggle), but the draft
                      document never touches this origin or navigates the app. */}
                      <iframe
                        ref={previewFrame}
                        onLoad={postPreviewChrome}
                        className="block h-[min(70vh,48rem)] w-full rounded-lg border-0 bg-surface"
                        title={strings.sitesPreviewTitle}
                        sandbox="allow-scripts"
                        srcDoc={
                          previewVersion === "after" &&
                          proposedPreviewHtml !== null
                            ? proposedPreviewHtml
                            : (previewHtml ?? "")
                        }
                      />
                    </div>
                  )}
                </div>
              </aside>
            )}
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
