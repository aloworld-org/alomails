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
} from "react";
import { Sparkles, X } from "lucide-react";
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
      </header>
      <div className={styles.body}>
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
