// Custom blocks for the alo Doc (BlockNote 0.52). Code blocks come from
// BlockNote's defaults; here we add a KaTeX **equation** block. `docSchema` is
// the default block set plus this one, consumed by DocEditor.
//
// createReactBlockSpec's `render` is a real function component, so the hooks
// inside it run in a valid component context.
import { useState } from "react";
import { Check, MessageSquareText, RotateCcw } from "lucide-react";
import { BlockNoteSchema, defaultBlockSpecs } from "@blocknote/core";
import { createReactBlockSpec } from "@blocknote/react";
import katex from "katex";
import "katex/dist/katex.min.css";

import { strings } from "../i18n";
import styles from "./docBlocks.module.css";

/** A math-formula block: shows KaTeX-rendered output; click it to edit the
 *  LaTeX. `createReactBlockSpec` returns a factory — call it to get the spec. */
export const EquationBlock = createReactBlockSpec(
  { type: "equation", propSchema: { latex: { default: "" } }, content: "none" },
  {
    render: ({ block, editor }) => {
      const latex = block.props.latex;
      const [editing, setEditing] = useState(latex.trim() === "");
      const [draft, setDraft] = useState(latex);

      function commit() {
        editor.updateBlock(block, { props: { latex: draft.trim() } });
        setEditing(false);
      }

      if (editing) {
        return (
          <div className={styles.edit} contentEditable={false}>
            <span className={styles.tag}>Σ</span>
            <input
              className={styles.input}
              autoFocus
              value={draft}
              placeholder="LaTeX — e.g. E = mc^2  or  \frac{a}{b}"
              onChange={(e) => setDraft(e.target.value)}
              onBlur={commit}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  commit();
                }
              }}
            />
          </div>
        );
      }

      let html: string;
      try {
        html = katex.renderToString(latex.length > 0 ? latex : "\\,", {
          throwOnError: false,
          displayMode: true,
        });
      } catch {
        html = latex;
      }
      return (
        <div
          className={styles.eq}
          contentEditable={false}
          role="button"
          tabIndex={0}
          onClick={() => {
            setDraft(latex);
            setEditing(true);
          }}
          dangerouslySetInnerHTML={{ __html: html }}
        />
      );
    },
  },
);

/** A document-native review comment. Its text and resolved state are part of
 * the same block tree as the document, so comments survive reloads and exports
 * without requiring a separate collaboration service. */
export const CommentBlock = createReactBlockSpec(
  {
    type: "comment",
    propSchema: { resolved: { default: false }, createdAt: { default: "" } },
    content: "inline",
  },
  {
    render: ({ block, editor, contentRef }) => (
      <aside className={`${styles.comment} ${block.props.resolved ? styles.commentResolved : ""}`}>
        <div className={styles.commentHead} contentEditable={false}>
          <MessageSquareText size={15} />
          <strong>{strings.docComment}</strong>
          {block.props.createdAt !== "" && <time>{block.props.createdAt}</time>}
          <button
            type="button"
            onClick={() => editor.updateBlock(block, { props: { resolved: !block.props.resolved } })}
            aria-label={block.props.resolved ? strings.docReopenComment : strings.docResolveComment}
            title={block.props.resolved ? strings.docReopenComment : strings.docResolveComment}
          >
            {block.props.resolved ? <RotateCcw size={14} /> : <Check size={14} />}
          </button>
        </div>
        <div ref={contentRef} className={styles.commentBody} data-placeholder={strings.docCommentPlaceholder} />
      </aside>
    ),
  },
);

/** The alo Doc schema: every default block (paragraph, headings, lists, code,
 *  quote, table, …) plus the equation block.
 *
 *  The argument is cast to `BlockNoteSchema.create`'s own parameter type: under
 *  `exactOptionalPropertyTypes` BlockNote's `defaultBlockSpecs` don't structurally
 *  satisfy its `BlockSpecs` index signature (optional props typed `T | undefined`),
 *  a library-vs-tsconfig mismatch, not a defect in this object. The runtime shape
 *  is exactly right; the assertion is scoped to this one boundary. */
export const docSchema = BlockNoteSchema.create({
  blockSpecs: { ...defaultBlockSpecs, equation: EquationBlock(), comment: CommentBlock() },
} as Parameters<typeof BlockNoteSchema.create>[0]);
