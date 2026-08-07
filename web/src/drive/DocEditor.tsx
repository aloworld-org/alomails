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
} from "react";
import { AlignCenter, AlignLeft, AlignRight, Bold, FileText, Italic, LayoutTemplate, List, Minus, Plus, Printer, Redo2, Sparkles, Strikethrough, Table2, Underline, Undo2, X } from "lucide-react";
import {
  useCreateBlockNote,
  SuggestionMenuController,
  getDefaultReactSlashMenuItems,
} from "@blocknote/react";
import { filterSuggestionItems } from "@blocknote/core";
import { BlockNoteView } from "@blocknote/mantine";
import "@blocknote/core/fonts/inter.css";
import "@blocknote/mantine/style.css";

import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { Spinner } from "../ds";
import { docSchema } from "./docBlocks";
import styles from "./DocEditor.module.css";

type SaveState = "idle" | "saving" | "saved";
type DocViewMode = "canvas" | "page";

/** Cap on the current-document context sent to the AI (characters). */
const CONTEXT_CAP = 12000;

export function DocEditor({
  nodeId,
  name,
  onClose,
}: {
  nodeId: string;
  name: string;
  onClose: () => void;
}) {
  const client = useJmapClient();
  const editor = useCreateBlockNote({ schema: docSchema });
  const [ready, setReady] = useState(false);
  const [saveState, setSaveState] = useState<SaveState>("idle");
  const [viewMode, setViewMode] = useState<DocViewMode>(() =>
    window.localStorage.getItem(`alo-doc-view:${nodeId}`) === "page" ? "page" : "canvas",
  );
  const [zoom, setZoom] = useState(100);
  const [activeStyles, setActiveStyles] = useState<Record<string, boolean | string>>({});
  const pending = useRef<unknown[] | null>(null);
  const timer = useRef<number | null>(null);
  const loaded = useRef(false);

  // AI propose-then-approve state.
  const [aiOpen, setAiOpen] = useState(false);
  const [instruction, setInstruction] = useState("");
  const [proposing, setProposing] = useState(false);
  const [proposal, setProposal] = useState<string | null>(null);
  const [aiError, setAiError] = useState(false);

  useEffect(() => {
    let live = true;
    void client
      .driveDocContent(nodeId)
      .then((c) => {
        if (!live) return;
        const blocks = c as unknown[];
        if (!loaded.current && blocks.length > 0) {
          loaded.current = true;
          editor.replaceBlocks(
            editor.document,
            blocks as Parameters<typeof editor.replaceBlocks>[1],
          );
        }
        setReady(true);
      })
      .catch(() => live && setReady(true));
    return () => {
      live = false;
    };
  }, [client, nodeId, editor]);

  useEffect(() => {
    window.localStorage.setItem(`alo-doc-view:${nodeId}`, viewMode);
  }, [nodeId, viewMode]);

  useEffect(() => editor.onSelectionChange(() => {
    setActiveStyles(editor.getActiveStyles() as Record<string, boolean | string>);
  }), [editor]);

  const insertBlock = (type: "table" | "divider" | "pageBreak") => {
    const anchor = editor.getTextCursorPosition().block;
    editor.insertBlocks([{ type }] as Parameters<typeof editor.insertBlocks>[0], anchor, "after");
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

  const save = useCallback(
    async (blocks: unknown[]) => {
      setSaveState("saving");
      try {
        await client.driveSaveDoc(nodeId, blocks);
        pending.current = null;
        setSaveState("saved");
      } catch {
        setSaveState("idle");
      }
    },
    [client, nodeId],
  );

  const onChange = useCallback(() => {
    const blocks = editor.document;
    pending.current = blocks;
    setSaveState("saving");
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      if (pending.current) void save(pending.current);
    }, 1200);
  }, [editor, save]);

  async function close() {
    if (timer.current !== null) window.clearTimeout(timer.current);
    if (pending.current) await save(pending.current);
    onClose();
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
      </header>
      {viewMode === "page" && <div className={styles.docCommands} aria-label={strings.docFormattingToolbar}>
        <div className={styles.docMenus}>
          <details><summary>{strings.docMenuFile}</summary><div className={styles.docMenuPanel}><button type="button" onClick={() => window.print()}><Printer size={15} />{strings.docPrint}</button></div></details>
          <details><summary>{strings.docMenuEdit}</summary><div className={styles.docMenuPanel}><button type="button" onClick={() => editor.undo()}><Undo2 size={15} />{strings.sheetUndo}</button><button type="button" onClick={() => editor.redo()}><Redo2 size={15} />{strings.sheetRedo}</button></div></details>
          <details><summary>{strings.docMenuInsert}</summary><div className={styles.docMenuPanel}><button type="button" onClick={() => insertBlock("table")}><Table2 size={15} />{strings.sheetInsertTable}</button><button type="button" onClick={() => insertBlock("divider")}><Minus size={15} />{strings.docInsertDivider}</button><button type="button" onClick={() => insertBlock("pageBreak")}><FileText size={15} />{strings.docInsertPageBreak}</button></div></details>
          <details><summary>{strings.docMenuFormat}</summary><div className={styles.docMenuPanel}><button type="button" onClick={() => toggleStyle("bold")}><Bold size={15} />{strings.sheetBold}</button><button type="button" onClick={() => toggleStyle("italic")}><Italic size={15} />{strings.sheetItalic}</button><button type="button" onClick={() => toggleStyle("underline")}><Underline size={15} />{strings.sheetUnderline}</button></div></details>
        </div>
        <div className={styles.commandDivider} />
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
        <button type="button" className={styles.commandIcon} onClick={() => insertBlock("table")} aria-label={strings.sheetInsertTable}><Table2 size={17} /></button>
        <button type="button" className={styles.commandIcon} onClick={() => insertBlock("divider")} aria-label={strings.docInsertDivider}><List size={17} /></button>
        <div className={styles.commandSpacer} />
        <div className={styles.zoomControl} aria-label={strings.docZoom}>
          <button type="button" onClick={() => setZoom((value) => Math.max(50, value - 10))} aria-label={strings.docZoomOut}><Minus size={15} /></button>
          <button type="button" onClick={() => setZoom(100)}>{zoom}%</button>
          <button type="button" onClick={() => setZoom((value) => Math.min(200, value + 10))} aria-label={strings.docZoomIn}><Plus size={15} /></button>
        </div>
        <button type="button" className={styles.printButton} onClick={() => window.print()}><Printer size={16} /><span>{strings.docPrint}</span></button>
      </div>}
      <div
        className={`${styles.body} ${viewMode === "page" ? styles.pageMode : styles.canvasMode}`}
        style={{ "--doc-zoom": viewMode === "page" ? zoom / 100 : 1 } as CSSProperties}
      >
        {!ready ? (
          <div className={styles.center}>
            <Spinner size={22} />
          </div>
        ) : (
          <BlockNoteView
            editor={editorProp}
            onChange={onChange}
            slashMenu={false}
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
