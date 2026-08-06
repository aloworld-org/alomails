// Custom blocks for the alo Doc (BlockNote 0.52). Code blocks come from
// BlockNote's defaults; here we add a KaTeX **equation** block. `docSchema` is
// the default block set plus this one, consumed by DocEditor.
//
// createReactBlockSpec's `render` is a real function component, so the hooks
// inside it run in a valid component context.
import { useState } from "react";
import { BlockNoteSchema, defaultBlockSpecs } from "@blocknote/core";
import { createReactBlockSpec } from "@blocknote/react";
import katex from "katex";
import "katex/dist/katex.min.css";

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

/** The alo Doc schema: every default block (paragraph, headings, lists, code,
 *  quote, table, …) plus the equation block.
 *
 *  The argument is cast to `BlockNoteSchema.create`'s own parameter type: under
 *  `exactOptionalPropertyTypes` BlockNote's `defaultBlockSpecs` don't structurally
 *  satisfy its `BlockSpecs` index signature (optional props typed `T | undefined`),
 *  a library-vs-tsconfig mismatch, not a defect in this object. The runtime shape
 *  is exactly right; the assertion is scoped to this one boundary. */
export const docSchema = BlockNoteSchema.create({
  blockSpecs: { ...defaultBlockSpecs, equation: EquationBlock() },
} as Parameters<typeof BlockNoteSchema.create>[0]);
