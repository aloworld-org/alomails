// alo Doc — the block editor (ADR 0031), built on BlockNote. A doc's content is
// a BlockNote block tree stored as the node's blob in Drive; opening loads it,
// editing auto-saves a new version (debounced).
//
// Document AI (ADR 0029 §3) is propose-then-approve: "Ask AI" returns a *draft*
// shown in a panel; nothing enters the document until the user clicks Insert.
// The AI never writes silently — that is the product's trust promise.
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ComponentProps,
  type CSSProperties,
  type SyntheticEvent,
} from "react";
import { AlignCenter, AlignLeft, AlignRight, Bold, Bot, Code2, FileText, Highlighter, ImagePlus, IndentDecrease, IndentIncrease, Italic, LayoutTemplate, Link2, List, ListChecks, ListOrdered, MessageSquarePlus, Minus, Plus, Printer, Redo2, Search, Sigma, Sparkles, Strikethrough, Table2, Underline, Undo2, X } from "lucide-react";
import {
  useCreateBlockNote,
  SuggestionMenuController,
  getDefaultReactSlashMenuItems,
} from "@blocknote/react";
import { filterSuggestionItems } from "@blocknote/core";
import { BlockNoteView } from "@blocknote/mantine";
import "@blocknote/core/fonts/inter.css";
import "@blocknote/mantine/style.css";

import { RecordAgentPanel, type RecordOrigin } from "../agents";
import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { ColorPicker, Spinner } from "../ds";
import { docSchema } from "./docBlocks";
import { driveErrorReason } from "./parts";
import styles from "./DocEditor.module.css";

type SaveState = "idle" | "saving" | "saved";
type DocViewMode = "canvas" | "page";
type PageSize = "a4" | "letter";
type PageOrientation = "portrait" | "landscape";
type PageMargins = "normal" | "narrow" | "wide";
type DocFont = "inter" | "arial" | "georgia" | "garamond";

/** Cap on the current-document context sent to the AI (characters). */
const CONTEXT_CAP = 12000;

function documentText(value: unknown): string {
  if (Array.isArray(value)) return value.map(documentText).join(" ");
  if (typeof value !== "object" || value === null) return "";
  const record = value as Record<string, unknown>;
  if (record.type === "text" && typeof record.text === "string") return record.text;
  return `${documentText(record.content)} ${documentText(record.children)}`.trim();
}

function documentCounts(blocks: unknown[]): { words: number; characters: number } {
  const text = documentText(blocks).replace(/\s+/g, " ").trim();
  return {
    words: text === "" ? 0 : text.split(" ").length,
    characters: text.replace(/\s/g, "").length,
  };
}

function fileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => typeof reader.result === "string" ? resolve(reader.result) : reject(new Error("Invalid image"));
    reader.onerror = () => reject(reader.error ?? new Error("Image read failed"));
    reader.readAsDataURL(file);
  });
}

function replaceText(value: unknown, search: string, replacement: string): unknown {
  if (Array.isArray(value)) return value.map((item) => replaceText(item, search, replacement));
  if (typeof value !== "object" || value === null) return value;
  const record = value as Record<string, unknown>;
  if (record.type === "text" && typeof record.text === "string") {
    return { ...record, text: record.text.split(search).join(replacement) };
  }
  return Object.fromEntries(Object.entries(record).map(([key, item]) => [key, replaceText(item, search, replacement)]));
}

function documentSettings(blocks: unknown[]): Record<string, unknown> | undefined {
  const settings = blocks.find((block) => typeof block === "object" && block !== null && (block as Record<string, unknown>).type === "docSettings") as Record<string, unknown> | undefined;
  return settings?.props as Record<string, unknown> | undefined;
}

export function DocEditor({
  nodeId,
  name,
  origin = null,
  onClose,
}: {
  nodeId: string;
  name: string;
  /** Where this document came from, as Drive carries it; `null` when it does
   *  not say. Passed in rather than read again — the file list already had
   *  the node (A8.4). */
  origin?: RecordOrigin | null;
  onClose: () => void;
}) {
  const client = useJmapClient();
  const editor = useCreateBlockNote({ schema: docSchema, uploadFile: fileAsDataUrl });
  const [ready, setReady] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [saveError, setSaveError] = useState("");
  const [saveState, setSaveState] = useState<SaveState>("idle");
  const [viewMode, setViewMode] = useState<DocViewMode>(() =>
    window.localStorage.getItem(`alo-doc-view:${nodeId}`) === "page" ? "page" : "canvas",
  );
  const [zoom, setZoom] = useState(100);
  // The document's agent, in a side area beside the writing surface. Closed
  // until asked for (ADR 0057): opening it is what makes its two reads.
  const [agentOpen, setAgentOpen] = useState(false);
  const [pageSize, setPageSize] = useState<PageSize>(() => window.localStorage.getItem(`alo-doc-page-size:${nodeId}`) === "letter" ? "letter" : "a4");
  const [pageOrientation, setPageOrientation] = useState<PageOrientation>(() => window.localStorage.getItem(`alo-doc-page-orientation:${nodeId}`) === "landscape" ? "landscape" : "portrait");
  const [pageMargins, setPageMargins] = useState<PageMargins>(() => {
    const stored = window.localStorage.getItem(`alo-doc-page-margins:${nodeId}`);
    return stored === "narrow" || stored === "wide" ? stored : "normal";
  });
  const [pageHeader, setPageHeader] = useState(() => window.localStorage.getItem(`alo-doc-page-header:${nodeId}`) ?? "");
  const [pageFooter, setPageFooter] = useState(() => window.localStorage.getItem(`alo-doc-page-footer:${nodeId}`) ?? "");
  const [showPageNumber, setShowPageNumber] = useState(() => window.localStorage.getItem(`alo-doc-page-number:${nodeId}`) === "true");
  const [docFont, setDocFont] = useState<DocFont>(() => {
    const stored = window.localStorage.getItem(`alo-doc-font:${nodeId}`);
    return stored === "arial" || stored === "georgia" || stored === "garamond" ? stored : "inter";
  });
  const [docFontSize, setDocFontSize] = useState(() => Number(window.localStorage.getItem(`alo-doc-font-size:${nodeId}`)) || 14);
  const [lineSpacing, setLineSpacing] = useState(() => Number(window.localStorage.getItem(`alo-doc-line-spacing:${nodeId}`)) || 1.5);
  const [activeStyles, setActiveStyles] = useState<Record<string, boolean | string>>({});
  const [activeBlockType, setActiveBlockType] = useState("paragraph");
  const [counts, setCounts] = useState({ words: 0, characters: 0 });
  const [findOpen, setFindOpen] = useState(false);
  const [findText, setFindText] = useState("");
  const [replaceWith, setReplaceWith] = useState("");
  const pending = useRef<unknown[] | null>(null);
  const timer = useRef<number | null>(null);
  const loaded = useRef(false);
  const imageInputRef = useRef<HTMLInputElement>(null);
  const menuBarRef = useRef<HTMLDivElement>(null);

  // AI propose-then-approve state.
  const [aiOpen, setAiOpen] = useState(false);
  const [instruction, setInstruction] = useState("");
  const [proposing, setProposing] = useState(false);
  const [proposal, setProposal] = useState<string | null>(null);
  const [aiError, setAiError] = useState(false);

  const closeMenus = useCallback(() => {
    menuBarRef.current?.querySelectorAll("details[open]").forEach((menu) => {
      menu.removeAttribute("open");
    });
  }, []);

  const handleMenuToggle = useCallback((event: SyntheticEvent<HTMLDetailsElement>) => {
    if (!event.currentTarget.open) return;
    menuBarRef.current?.querySelectorAll("details[open]").forEach((menu) => {
      if (menu !== event.currentTarget) menu.removeAttribute("open");
    });
  }, []);

  useEffect(() => {
    const closeOutside = (event: PointerEvent) => {
      if (!menuBarRef.current?.contains(event.target as Node)) closeMenus();
    };
    document.addEventListener("pointerdown", closeOutside);
    return () => document.removeEventListener("pointerdown", closeOutside);
  }, [closeMenus]);

  useEffect(() => {
    let live = true;
    setReady(false);
    setLoadError(null);
    void client
      .driveDocContent(nodeId)
      .then((c) => {
        if (!live) return;
        const blocks = c as unknown[];
        const settings = documentSettings(blocks);
        if (settings !== undefined) {
          if (settings.pageSize === "a4" || settings.pageSize === "letter") setPageSize(settings.pageSize);
          if (settings.orientation === "portrait" || settings.orientation === "landscape") setPageOrientation(settings.orientation);
          if (settings.margins === "normal" || settings.margins === "narrow" || settings.margins === "wide") setPageMargins(settings.margins);
          if (typeof settings.header === "string") setPageHeader(settings.header);
          if (typeof settings.footer === "string") setPageFooter(settings.footer);
          if (typeof settings.pageNumber === "boolean") setShowPageNumber(settings.pageNumber);
          if (settings.font === "inter" || settings.font === "arial" || settings.font === "georgia" || settings.font === "garamond") setDocFont(settings.font);
          if (typeof settings.fontSize === "number") setDocFontSize(settings.fontSize);
          if (typeof settings.lineSpacing === "number") setLineSpacing(settings.lineSpacing);
        }
        if (!loaded.current && blocks.length > 0) {
          loaded.current = true;
          editor.replaceBlocks(
            editor.document,
            blocks as Parameters<typeof editor.replaceBlocks>[1],
          );
        }
        setCounts(documentCounts(blocks));
        setReady(true);
      })
      .catch((error: unknown) => {
        if (live) setLoadError(driveErrorReason(error) ?? strings.driveUnknownError);
      });
    return () => {
      live = false;
    };
  }, [client, nodeId, editor, loadAttempt]);

  useEffect(() => {
    window.localStorage.setItem(`alo-doc-view:${nodeId}`, viewMode);
  }, [nodeId, viewMode]);

  useEffect(() => {
    window.localStorage.setItem(`alo-doc-page-size:${nodeId}`, pageSize);
    window.localStorage.setItem(`alo-doc-page-orientation:${nodeId}`, pageOrientation);
    window.localStorage.setItem(`alo-doc-page-margins:${nodeId}`, pageMargins);
    window.localStorage.setItem(`alo-doc-page-header:${nodeId}`, pageHeader);
    window.localStorage.setItem(`alo-doc-page-footer:${nodeId}`, pageFooter);
    window.localStorage.setItem(`alo-doc-page-number:${nodeId}`, String(showPageNumber));
    window.localStorage.setItem(`alo-doc-font:${nodeId}`, docFont);
    window.localStorage.setItem(`alo-doc-font-size:${nodeId}`, String(docFontSize));
    window.localStorage.setItem(`alo-doc-line-spacing:${nodeId}`, String(lineSpacing));
  }, [docFont, docFontSize, lineSpacing, nodeId, pageFooter, pageHeader, pageMargins, pageOrientation, pageSize, showPageNumber]);

  useEffect(() => editor.onSelectionChange(() => {
    setActiveStyles(editor.getActiveStyles() as Record<string, boolean | string>);
    setActiveBlockType(editor.getTextCursorPosition().block.type);
  }), [editor]);

  const changeBlockType = (type: string) => {
    const block = editor.getTextCursorPosition().block;
    const update = type.startsWith("heading-")
      ? { type: "heading", props: { level: Number(type.slice(-1)) } }
      : { type };
    editor.updateBlock(block, update as Parameters<typeof editor.updateBlock>[1]);
    setActiveBlockType(type);
    onChange();
  };

  const insertBlock = (type: "table" | "divider" | "pageBreak" | "equation" | "aloCode") => {
    const anchor = editor.getTextCursorPosition().block;
    editor.insertBlocks([{ type }] as Parameters<typeof editor.insertBlocks>[0], anchor, "after");
    onChange();
  };

  const insertComment = () => {
    const anchor = editor.getTextCursorPosition().block;
    editor.insertBlocks(
      [{ type: "comment", props: { createdAt: new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date()) } }] as unknown as Parameters<typeof editor.insertBlocks>[0],
      anchor,
      "after",
    );
    onChange();
  };

  const createLink = () => {
    const url = window.prompt(strings.docLinkPrompt, editor.getSelectedLinkUrl() ?? "https://");
    if (url === null || url.trim() === "") return;
    editor.createLink(url.trim());
    onChange();
  };

  const insertImage = async (file: File) => {
    const url = await fileAsDataUrl(file);
    const anchor = editor.getTextCursorPosition().block;
    editor.insertBlocks(
      [{ type: "image", props: { url, name: file.name } }] as Parameters<typeof editor.insertBlocks>[0],
      anchor,
      "after",
    );
    onChange();
  };

  const replaceAll = () => {
    if (findText === "") return;
    const blocks = replaceText(editor.document, findText, replaceWith) as Parameters<typeof editor.replaceBlocks>[1];
    editor.replaceBlocks(editor.document, blocks);
    onChange();
  };

  const align = (textAlignment: "left" | "center" | "right") => {
    const block = editor.getTextCursorPosition().block;
    editor.updateBlock(block, { props: { textAlignment } } as Parameters<typeof editor.updateBlock>[1]);
    onChange();
  };

  const toggleStyle = (style: "bold" | "italic" | "underline" | "strike") => {
    editor.toggleStyles({ [style]: true });
    setActiveStyles(editor.getActiveStyles() as Record<string, boolean | string>);
    onChange();
  };

  const setColorStyle = (style: "textColor" | "backgroundColor", value: string) => {
    if (value === "default") editor.removeStyles({ [style]: activeStyles[style] ?? "default" });
    else editor.addStyles({ [style]: value });
    setActiveStyles(editor.getActiveStyles() as Record<string, boolean | string>);
    onChange();
  };

  const changeIndent = (direction: "in" | "out") => {
    if (direction === "in" && editor.canNestBlock()) editor.nestBlock();
    else if (direction === "out" && editor.canUnnestBlock()) editor.unnestBlock();
    onChange();
  };

  const save = useCallback(
    async (blocks: unknown[]) => {
      setSaveState("saving");
      try {
        await client.driveSaveDoc(nodeId, blocks);
        pending.current = null;
        setSaveError("");
        setSaveState("saved");
      } catch (error: unknown) {
        setSaveError(strings.docSaveFailed(driveErrorReason(error) ?? strings.driveUnknownError));
        setSaveState("idle");
      }
    },
    [client, nodeId],
  );

  const onChange = useCallback(() => {
    const blocks = editor.document;
    setCounts(documentCounts(blocks as unknown[]));
    pending.current = blocks;
    setSaveState("saving");
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      if (pending.current) void save(pending.current);
    }, 1200);
  }, [editor, save]);

  useEffect(() => {
    if (!ready) return;
    const props = {
      pageSize,
      orientation: pageOrientation,
      margins: pageMargins,
      header: pageHeader,
      footer: pageFooter,
      pageNumber: showPageNumber,
      font: docFont,
      fontSize: docFontSize,
      lineSpacing,
    };
    const settingsBlock = (editor.document as unknown as Array<Record<string, unknown>>).find((block) => block.type === "docSettings");
    if (settingsBlock === undefined) {
      const first = editor.document[0];
      if (first !== undefined) {
        editor.insertBlocks([{ type: "docSettings", props }] as unknown as Parameters<typeof editor.insertBlocks>[0], first, "before");
        onChange();
      }
      return;
    }
    const current = settingsBlock.props as Record<string, unknown> | undefined;
    if (Object.entries(props).some(([key, value]) => current?.[key] !== value)) {
      editor.updateBlock(settingsBlock as unknown as Parameters<typeof editor.updateBlock>[0], { props } as Parameters<typeof editor.updateBlock>[1]);
      onChange();
    }
  }, [docFont, docFontSize, editor, lineSpacing, onChange, pageFooter, pageHeader, pageMargins, pageOrientation, pageSize, ready, showPageNumber]);

  async function close() {
    if (timer.current !== null) window.clearTimeout(timer.current);
    if (pending.current) await save(pending.current);
    onClose();
  }

  /** Send a debounced edit now, without waiting for it. Used when something
   *  else is about to navigate away from the editor — the agent panel opening
   *  a conversation — where there is no turn left to await the save in. */
  function flushSave() {
    if (timer.current !== null) window.clearTimeout(timer.current);
    if (pending.current) void save(pending.current);
  }

  async function propose() {
    const ask = instruction.trim();
    if (ask === "" || proposing) return;
    setProposing(true);
    setAiError(false);
    setProposal(null);
    try {
      const context = (await editor.blocksToMarkdownLossy(editor.document)).slice(0, CONTEXT_CAP);
      const text = await client.composeDoc(ask, context);
      setProposal(text);
    } catch {
      setAiError(true);
    } finally {
      setProposing(false);
    }
  }

  /** Approve: turn the proposal into blocks and append them — the only path that
   *  writes AI text into the document. */
  async function insertProposal() {
    if (!proposal) return;
    const blocks = await editor.tryParseMarkdownToBlocks(proposal);
    const doc = editor.document;
    const anchor = doc[doc.length - 1];
    if (anchor) {
      editor.insertBlocks(
        blocks as Parameters<typeof editor.insertBlocks>[0],
        anchor,
        "after",
      );
    } else {
      editor.replaceBlocks(
        editor.document,
        blocks as Parameters<typeof editor.replaceBlocks>[1],
      );
    }
    onChange();
    discardProposal();
  }

  function discardProposal() {
    setProposal(null);
    setInstruction("");
    setAiOpen(false);
    setAiError(false);
  }

  const editorProp = editor as unknown as ComponentProps<typeof BlockNoteView>["editor"];

  return (
    <div className={styles.overlay}>
      <header className={styles.head}>
        <button type="button" className={styles.back} onClick={() => void close()} aria-label={strings.close}>
          <X size={18} />
        </button>
        <span className={styles.name}>{name}</span>
        <span className={styles.save}>
          {saveState === "saving" ? strings.docSaving : saveState === "saved" ? strings.docSaved : ""}
        </span>
        <div className={styles.viewSwitch} role="group" aria-label={strings.docViewMode}>
          <button
            type="button"
            className={viewMode === "canvas" ? styles.viewOptionActive : styles.viewOption}
            aria-pressed={viewMode === "canvas"}
            onClick={() => setViewMode("canvas")}
            title={strings.docCanvasViewHint}
          >
            <LayoutTemplate size={15} />
            <span>{strings.docCanvasView}</span>
          </button>
          <button
            type="button"
            className={viewMode === "page" ? styles.viewOptionActive : styles.viewOption}
            aria-pressed={viewMode === "page"}
            onClick={() => setViewMode("page")}
            title={strings.docPageViewHint}
          >
            <FileText size={15} />
            <span>{strings.docPageView}</span>
          </button>
        </div>
        <button
          type="button"
          className={agentOpen ? styles.viewOptionActive : styles.viewOption}
          aria-pressed={agentOpen}
          // The words are hidden at phone widths (the view switch's rule), so
          // the name has to be on the button rather than only inside it.
          aria-label={strings.recordAgentPanelToggle}
          onClick={() => setAgentOpen((open) => !open)}
          title={strings.recordAgentTitle}
        >
          <Bot size={15} />
          <span>{strings.recordAgentPanelToggle}</span>
        </button>
      </header>
      {viewMode === "page" && <div className={styles.docCommands} aria-label={strings.docFormattingToolbar}>
        <div ref={menuBarRef} className={styles.docMenus} onClick={(event) => { if ((event.target as HTMLElement).closest("button") !== null) closeMenus(); }}>
          <details onToggle={handleMenuToggle}><summary>{strings.docMenuFile}</summary><div className={styles.docMenuPanel}><button type="button" onClick={() => window.print()}><Printer size={15} />{strings.docPrint}</button><button type="button" onClick={() => window.print()}><FileText size={15} />{strings.docSavePdf}</button></div></details>
          <details onToggle={handleMenuToggle}><summary>{strings.docMenuEdit}</summary><div className={styles.docMenuPanel}><button type="button" onClick={() => editor.undo()}><Undo2 size={15} />{strings.sheetUndo}</button><button type="button" onClick={() => editor.redo()}><Redo2 size={15} />{strings.sheetRedo}</button></div></details>
          <details onToggle={handleMenuToggle}><summary>{strings.docMenuInsert}</summary><div className={styles.docMenuPanel}><button type="button" onClick={createLink}><Link2 size={15} />{strings.docInsertLink}</button><button type="button" onClick={() => imageInputRef.current?.click()}><ImagePlus size={15} />{strings.docInsertImage}</button><button type="button" onClick={() => insertBlock("table")}><Table2 size={15} />{strings.sheetInsertTable}</button><button type="button" onClick={() => insertBlock("aloCode")}><Code2 size={15} />{strings.composeInsertCode}</button><button type="button" onClick={() => insertBlock("equation")}><Sigma size={15} />{strings.docEquation}</button><button type="button" onClick={insertComment}><MessageSquarePlus size={15} />{strings.docAddComment}</button><button type="button" onClick={() => insertBlock("divider")}><Minus size={15} />{strings.docInsertDivider}</button><button type="button" onClick={() => insertBlock("pageBreak")}><FileText size={15} />{strings.docInsertPageBreak}</button></div></details>
          <details onToggle={handleMenuToggle}><summary>{strings.docMenuFormat}</summary><div className={styles.docMenuPanel}><button type="button" onClick={() => toggleStyle("bold")}><Bold size={15} />{strings.sheetBold}</button><button type="button" onClick={() => toggleStyle("italic")}><Italic size={15} />{strings.sheetItalic}</button><button type="button" onClick={() => toggleStyle("underline")}><Underline size={15} />{strings.sheetUnderline}</button></div></details>
          <details onToggle={handleMenuToggle}><summary>{strings.docPageSetup}</summary><div className={`${styles.docMenuPanel} ${styles.pageSetupPanel}`}>
            <label>{strings.docPageSize}<select value={pageSize} onChange={(event) => setPageSize(event.target.value as PageSize)}><option value="a4">A4</option><option value="letter">{strings.docPageLetter}</option></select></label>
            <label>{strings.docPageOrientation}<select value={pageOrientation} onChange={(event) => setPageOrientation(event.target.value as PageOrientation)}><option value="portrait">{strings.docPagePortrait}</option><option value="landscape">{strings.docPageLandscape}</option></select></label>
            <label>{strings.docPageMargins}<select value={pageMargins} onChange={(event) => setPageMargins(event.target.value as PageMargins)}><option value="normal">{strings.docMarginsNormal}</option><option value="narrow">{strings.docMarginsNarrow}</option><option value="wide">{strings.docMarginsWide}</option></select></label>
            <label>{strings.docHeader}<input value={pageHeader} onChange={(event) => setPageHeader(event.target.value)} placeholder={strings.docHeaderPlaceholder} /></label>
            <label>{strings.docFooter}<input value={pageFooter} onChange={(event) => setPageFooter(event.target.value)} placeholder={strings.docFooterPlaceholder} /></label>
            <label className={styles.pageNumberOption}><input type="checkbox" checked={showPageNumber} onChange={(event) => setShowPageNumber(event.target.checked)} />{strings.docPageNumbers}</label>
          </div></details>
        </div>
        <div className={styles.formattingRow}>
        <div className={styles.formattingScroller}>
        <div className={styles.commandDivider} />
        <select className={styles.blockTypeSelect} aria-label={strings.docParagraphStyle} value={activeBlockType === "heading" ? "heading-1" : activeBlockType} onChange={(event) => changeBlockType(event.target.value)}>
          <option value="paragraph">{strings.docStyleParagraph}</option>
          <option value="heading-1">{strings.docStyleHeading1}</option>
          <option value="heading-2">{strings.docStyleHeading2}</option>
          <option value="heading-3">{strings.docStyleHeading3}</option>
          <option value="bulletListItem">{strings.docStyleBulletList}</option>
          <option value="numberedListItem">{strings.docStyleNumberedList}</option>
          <option value="checkListItem">{strings.docStyleChecklist}</option>
        </select>
        <select className={styles.fontSelect} aria-label={strings.docFontFamily} value={docFont} onChange={(event) => setDocFont(event.target.value as DocFont)}><option value="inter">Inter</option><option value="arial">Arial</option><option value="georgia">Georgia</option><option value="garamond">Garamond</option></select>
        <select className={styles.fontSizeSelect} aria-label={strings.docFontSize} value={docFontSize} onChange={(event) => setDocFontSize(Number(event.target.value))}>{[10, 11, 12, 14, 16, 18, 20, 24].map((size) => <option key={size} value={size}>{size}</option>)}</select>
        <select className={styles.lineSpacingSelect} aria-label={strings.docLineSpacing} value={lineSpacing} onChange={(event) => setLineSpacing(Number(event.target.value))}><option value="1">1.0</option><option value="1.15">1.15</option><option value="1.5">1.5</option><option value="2">2.0</option></select>
        <DocColorPicker label={strings.docTextColor} resetLabel={strings.docColorDefault} value={typeof activeStyles.textColor === "string" ? activeStyles.textColor : "default"} fallback="#102a43" variant="text" onPick={(value) => setColorStyle("textColor", value)} />
        <DocColorPicker label={strings.docHighlightColor} resetLabel={strings.docHighlightNone} value={typeof activeStyles.backgroundColor === "string" ? activeStyles.backgroundColor : "default"} fallback="#ffffff" variant="highlight" onPick={(value) => setColorStyle("backgroundColor", value)} />
        <button type="button" className={styles.commandIcon} onClick={() => editor.undo()} aria-label={strings.sheetUndo} title={strings.sheetUndo}><Undo2 size={17} /></button>
        <button type="button" className={styles.commandIcon} onClick={() => editor.redo()} aria-label={strings.sheetRedo} title={strings.sheetRedo}><Redo2 size={17} /></button>
        <div className={styles.commandDivider} />
        <button type="button" className={activeStyles.bold ? styles.commandIconActive : styles.commandIcon} onClick={() => toggleStyle("bold")} aria-label={strings.sheetBold}><Bold size={17} /></button>
        <button type="button" className={activeStyles.italic ? styles.commandIconActive : styles.commandIcon} onClick={() => toggleStyle("italic")} aria-label={strings.sheetItalic}><Italic size={17} /></button>
        <button type="button" className={activeStyles.underline ? styles.commandIconActive : styles.commandIcon} onClick={() => toggleStyle("underline")} aria-label={strings.sheetUnderline}><Underline size={17} /></button>
        <button type="button" className={activeStyles.strike ? styles.commandIconActive : styles.commandIcon} onClick={() => toggleStyle("strike")} aria-label={strings.strikethrough}><Strikethrough size={17} /></button>
        <div className={styles.commandDivider} />
        <button type="button" className={styles.commandIcon} onClick={() => align("left")} aria-label={strings.sheetAlignLeft}><AlignLeft size={17} /></button>
        <button type="button" className={styles.commandIcon} onClick={() => align("center")} aria-label={strings.sheetAlignCenter}><AlignCenter size={17} /></button>
        <button type="button" className={styles.commandIcon} onClick={() => align("right")} aria-label={strings.sheetAlignRight}><AlignRight size={17} /></button>
        <button type="button" className={styles.commandIcon} onClick={() => changeIndent("out")} aria-label={strings.docOutdent}><IndentDecrease size={17} /></button>
        <button type="button" className={styles.commandIcon} onClick={() => changeIndent("in")} aria-label={strings.docIndent}><IndentIncrease size={17} /></button>
        <button type="button" className={activeBlockType === "bulletListItem" ? styles.commandIconActive : styles.commandIcon} onClick={() => changeBlockType("bulletListItem")} aria-label={strings.docStyleBulletList} title={strings.docStyleBulletList}><List size={17} /></button>
        <button type="button" className={activeBlockType === "numberedListItem" ? styles.commandIconActive : styles.commandIcon} onClick={() => changeBlockType("numberedListItem")} aria-label={strings.docStyleNumberedList} title={strings.docStyleNumberedList}><ListOrdered size={17} /></button>
        <button type="button" className={activeBlockType === "checkListItem" ? styles.commandIconActive : styles.commandIcon} onClick={() => changeBlockType("checkListItem")} aria-label={strings.docStyleChecklist} title={strings.docStyleChecklist}><ListChecks size={17} /></button>
        <button type="button" className={styles.commandIcon} onClick={() => insertBlock("aloCode")} aria-label={strings.composeInsertCode}><Code2 size={17} /></button>
        <button type="button" className={styles.commandIcon} onClick={() => insertBlock("equation")} aria-label={strings.docEquation}><Sigma size={17} /></button>
        <button type="button" className={styles.commandIcon} onClick={insertComment} aria-label={strings.docAddComment}><MessageSquarePlus size={17} /></button>
        <button type="button" className={styles.commandIcon} onClick={() => insertBlock("divider")} aria-label={strings.docInsertDivider}><Minus size={17} /></button>
        <span className={styles.wordCount} title={`${counts.characters} ${strings.docCharacters}`}>{counts.words} {strings.docWords}</span>
        <div className={styles.zoomControl} aria-label={strings.docZoom}>
          <button type="button" onClick={() => setZoom((value) => Math.max(50, value - 10))} aria-label={strings.docZoomOut}><Minus size={15} /></button>
          <button type="button" onClick={() => setZoom(100)}>{zoom}%</button>
          <button type="button" onClick={() => setZoom((value) => Math.min(200, value + 10))} aria-label={strings.docZoomIn}><Plus size={15} /></button>
        </div>
        </div>
        <div className={styles.primaryActions}>
          <button type="button" className={styles.commandIcon} onClick={createLink} aria-label={strings.docInsertLink}><Link2 size={17} /></button>
          <button type="button" className={styles.commandIcon} onClick={() => imageInputRef.current?.click()} aria-label={strings.docInsertImage}><ImagePlus size={17} /></button>
          <button type="button" className={styles.commandIcon} onClick={() => insertBlock("table")} aria-label={strings.sheetInsertTable}><Table2 size={17} /></button>
          <button type="button" className={styles.commandIcon} onClick={() => setFindOpen((open) => !open)} aria-label={strings.docFindReplace}><Search size={17} /></button>
          <button type="button" className={styles.printButton} onClick={() => window.print()}><Printer size={16} /><span>{strings.docPrint}</span></button>
        </div>
        </div>
        <input ref={imageInputRef} className={styles.hiddenFileInput} type="file" accept="image/*" onChange={(event) => { const file = event.target.files?.[0]; if (file) void insertImage(file); event.target.value = ""; }} />
      </div>}
      {viewMode === "page" && findOpen && <div className={styles.findPanel}>
        <div className={styles.findPanelHead}><strong>{strings.docFindReplace}</strong><button type="button" onClick={() => setFindOpen(false)} aria-label={strings.close}><X size={16} /></button></div>
        <label>{strings.docFind}<input value={findText} onChange={(event) => setFindText(event.target.value)} autoFocus /></label>
        <label>{strings.docReplaceWith}<input value={replaceWith} onChange={(event) => setReplaceWith(event.target.value)} /></label>
        <div className={styles.findActions}><button type="button" onClick={() => { if (findText !== "") (window as Window & { find?: (text: string) => boolean }).find?.(findText); }}>{strings.docFindNext}</button><button type="button" onClick={replaceAll} disabled={findText === ""}>{strings.docReplaceAll}</button></div>
      </div>}
      <div className={styles.workArea}>
      <div
        className={`${styles.body} ${viewMode === "page" ? styles.pageMode : styles.canvasMode}`}
        style={{
          "--doc-zoom": viewMode === "page" ? zoom / 100 : 1,
          "--doc-page-width": pageOrientation === "portrait" ? (pageSize === "a4" ? "210mm" : "216mm") : (pageSize === "a4" ? "297mm" : "279mm"),
          "--doc-page-height": pageOrientation === "portrait" ? (pageSize === "a4" ? "297mm" : "279mm") : (pageSize === "a4" ? "210mm" : "216mm"),
          "--doc-page-margin-x": pageMargins === "narrow" ? "12.7mm" : pageMargins === "wide" ? "31.7mm" : "25.4mm",
          "--doc-page-margin-y": pageMargins === "narrow" ? "12.7mm" : pageMargins === "wide" ? "31.7mm" : "25.4mm",
          "--doc-font-family": docFont === "arial" ? "Arial, sans-serif" : docFont === "georgia" ? "Georgia, serif" : docFont === "garamond" ? "EB Garamond, Garamond, serif" : "Inter, sans-serif",
          "--doc-font-size": `${docFontSize}px`,
          "--doc-line-spacing": lineSpacing,
        } as CSSProperties}
      >
        {viewMode === "page" && (pageHeader !== "" || pageFooter !== "" || showPageNumber) && <div className={styles.pageFurniture} aria-hidden="true">
          <span className={styles.pageHeader}>{pageHeader}</span>
          <span className={styles.pageFooter}>{pageFooter}</span>
          {showPageNumber && <span className={styles.pageNumber}>1</span>}
        </div>}
        {loadError !== null ? (
          <div className={styles.docLoadError} role="alert">
            <h2>{strings.docLoadFailedTitle}</h2>
            <p>{strings.driveLoadFailed(loadError)}</p>
            <button type="button" onClick={() => setLoadAttempt((value) => value + 1)}>{strings.driveRetry}</button>
          </div>
        ) : !ready ? (
          <DocSkeleton viewMode={viewMode} />
        ) : (
          <BlockNoteView
            editor={editorProp}
            onChange={onChange}
            slashMenu={false}
            sideMenu={viewMode === "canvas"}
            theme={{
              borderRadius: 6,
              colors: {
                editor: { background: "var(--bg-surface)", text: "var(--text-primary)" },
                menu: { background: "var(--bg-surface)", text: "var(--text-primary)" },
                hovered: {
                  background: "color-mix(in srgb, var(--accent) 10%, var(--bg-surface))",
                  text: "var(--accent)",
                },
                selected: { background: "var(--accent)", text: "var(--on-accent, #fff)" },
                tooltip: {
                  background: "color-mix(in srgb, var(--accent) 12%, var(--bg-surface))",
                  text: "var(--accent)",
                },
                disabled: { background: "var(--bg-raised)", text: "var(--text-tertiary)" },
                border: "var(--border-default)",
                shadow: "var(--shadow-lg)",
                sideMenu: "var(--text-tertiary)",
              },
            }}
          >
            <SuggestionMenuController
              triggerCharacter="/"
              getItems={async (query) =>
                filterSuggestionItems(
                  [
                    // Cast at the boundary: exactOptionalPropertyTypes makes even
                    // the default editor fail this generic's constraint — a
                    // BlockNote/tsconfig mismatch, not an unsafe conversion.
                    ...getDefaultReactSlashMenuItems(
                      editor as unknown as Parameters<typeof getDefaultReactSlashMenuItems>[0],
                    ),
                    {
                      title: strings.docEquation,
                      subtext: strings.docEquationHint,
                      aliases: ["equation", "formula", "math", "latex", "katex"],
                      group: strings.docBlockGroupAdvanced,
                      icon: <span aria-hidden>Σ</span>,
                      onItemClick: () => {
                        const ref = editor.getTextCursorPosition().block;
                        editor.insertBlocks(
                          [{ type: "equation" }] as unknown as Parameters<
                            typeof editor.insertBlocks
                          >[0],
                          ref,
                          "after",
                        );
                      },
                    },
                  ],
                  query,
                )
              }
            />
          </BlockNoteView>
        )}
        {saveError !== "" && (
          <div className={styles.docSaveError} role="alert">
            <span>{saveError}</span>
            <button type="button" onClick={() => { if (pending.current !== null) void save(pending.current); }}>{strings.driveRetry}</button>
          </div>
        )}
      </div>
      {agentOpen && (
        <aside className={styles.agentPane} aria-label={strings.recordAgentTitle}>
          <RecordAgentPanel
            product="docs"
            recordKind="doc"
            recordId={nodeId}
            recordLabel={name}
            origin={origin}
            onBeforeNavigate={flushSave}
          />
        </aside>
      )}
      </div>

      {/* AI: a compact, centred dock at the bottom — a button until opened, then
          a half-width composer with the proposal shown above it. */}
      <div className={styles.aiDock}>
        {proposal !== null && (
          <div className={styles.aiPanel}>
            <div className={styles.aiPanelLabel}>{strings.docAiProposalLabel}</div>
            <div className={styles.aiProposal}>{proposal}</div>
            <div className={styles.aiActions}>
              <button type="button" className={styles.aiInsert} onClick={() => void insertProposal()}>
                {strings.docAiInsert}
              </button>
              <button type="button" className={styles.aiDiscard} onClick={discardProposal}>
                {strings.docAiDiscard}
              </button>
            </div>
          </div>
        )}
        {aiError && <p className={styles.aiErr}>{strings.docAiUnavailable}</p>}
        {aiOpen ? (
          <div className={styles.aiInputRow}>
            <Sparkles size={16} className={styles.aiIcon} />
            <input
              className={styles.aiInput}
              autoFocus
              value={instruction}
              placeholder={strings.docAiPlaceholder}
              onChange={(e) => setInstruction(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void propose();
                else if (e.key === "Escape") setAiOpen(false);
              }}
            />
            <button
              type="button"
              className={styles.aiGo}
              onClick={() => void propose()}
              disabled={proposing || instruction.trim() === ""}
            >
              {proposing ? <Spinner size={14} /> : strings.docAiPropose}
            </button>
            <button type="button" className={styles.aiClose} onClick={() => setAiOpen(false)} aria-label={strings.close}>
              <X size={16} />
            </button>
          </div>
        ) : (
          <button type="button" className={styles.aiFab} onClick={() => setAiOpen(true)}>
            <Sparkles size={16} />
            {strings.docAskAi}
          </button>
        )}
      </div>
    </div>
  );
}

function DocSkeleton({ viewMode }: { viewMode: DocViewMode }) {
  return (
    <div className={`${styles.docSkeleton} ${viewMode === "page" ? styles.docSkeletonPage : ""}`} role="status" aria-label={strings.docLoading} aria-busy="true">
      <span className={styles.docSkeletonLineWide} />
      <span className={styles.docSkeletonLine} />
      <span className={styles.docSkeletonLineShort} />
      <span className={styles.docSkeletonBlock} />
    </div>
  );
}

const NAMED_COLORS: Record<string, string> = {
  red: "#e03131",
  orange: "#eb6f4b",
  yellow: "#f5c451",
  green: "#2f9e66",
  blue: "#3478f6",
  purple: "#7950f2",
};

function DocColorPicker({ label, resetLabel, value, fallback, variant, onPick }: {
  label: string;
  resetLabel: string;
  value: string;
  fallback: string;
  variant: "text" | "highlight";
  onPick: (value: string) => void;
}) {
  const resolved = value === "default" ? fallback : NAMED_COLORS[value] ?? value;
  const color = /^#[0-9a-f]{6}/i.test(resolved) ? resolved.slice(0, 7) : fallback;

  return (
    <ColorPicker
      label={label}
      value={color}
      onChange={onPick}
      triggerIcon={variant === "text" ? <span className="text-sm font-semibold">A</span> : <Highlighter size={16} />}
      triggerClassName="!size-8 !rounded-md !border-0 !bg-transparent"
      resetLabel={resetLabel}
      onReset={() => onPick("default")}
    />
  );
}
