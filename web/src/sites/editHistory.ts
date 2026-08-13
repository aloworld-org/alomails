// One undo history for every direct-manipulation gesture on the page editor
// (ADR 0042).
//
// The ADR's promise is that the human path and the assistant path produce the
// same kind of change, "so they share the same preview, the same diff and the
// same undo". A history per gesture would break the third of those the moment
// there were two gestures: typing in a headline and dragging the section it
// sits in would take turns on ⌘Z, in an order nobody could predict. So there
// is one stack of steps, and a step says which gesture it was.
//
// Every step is **invertible by construction** — that is what makes it a step
// rather than a snapshot. Undoing is not a special code path that restores a
// saved copy of the page: it is the inverse gesture, sent through the same
// door as the original, and therefore validated, refused and stored exactly
// like anything else a person does.

/** One reversible change made on the page. */
export type EditStep =
  /** Text rewritten at one coordinate: `before` is what the page had when the
   *  gesture started, `after` what replaced it. */
  | { kind: "text"; key: string; before: string; after: string }
  /** A section moved from one position to another. Splice semantics: the
   *  section is taken out at `from` and put back in at `to`, which is what
   *  both doors that can move one already do. */
  | { kind: "move"; from: number; to: number }
  /** A section resized to another of the values its type declares: `key`
   *  names the control, `before` and `after` are two of its declared values
   *  and never anything between them. */
  | {
      kind: "layout";
      index: number;
      key: string;
      before: string;
      after: string;
    };

/** Undo and redo as two stacks of the same steps: the past is what has been
 *  applied, the future what has been taken back and could return. */
export interface EditHistory {
  past: EditStep[];
  future: EditStep[];
}

export const emptyEditHistory: EditHistory = { past: [], future: [] };

/** How deep undo goes. Long enough to cover a session of editing, short
 *  enough that the editor never holds an unbounded copy of the page. */
const HISTORY_LIMIT = 50;

/** The gesture that takes `step` back.
 *
 *  A text step swaps its two strings; a resize swaps its two declared values;
 *  a move swaps its two positions, because a splice out of `from` and into
 *  `to` is undone exactly by a splice out of `to` and into `from`. All three
 *  are ordinary gestures — there is no "undo" request the server has to
 *  understand. */
export function invertEdit(step: EditStep): EditStep {
  switch (step.kind) {
    case "text":
      return { kind: "text", key: step.key, before: step.after, after: step.before };
    case "layout":
      return {
        kind: "layout",
        index: step.index,
        key: step.key,
        before: step.after,
        after: step.before,
      };
    default:
      return { kind: "move", from: step.to, to: step.from };
  }
}

/** Records an applied change and drops the redo branch, which is what any new
 *  change after an undo means. */
export function recordEdit(history: EditHistory, step: EditStep): EditHistory {
  return { past: [...history.past, step].slice(-HISTORY_LIMIT), future: [] };
}

/** The step undo would take back, with the history that follows it. The
 *  caller applies `invertEdit(step)`. */
export function undoEdit(
  history: EditHistory,
): { history: EditHistory; step: EditStep } | null {
  const step = history.past.at(-1);
  if (step === undefined) return null;
  return {
    history: { past: history.past.slice(0, -1), future: [step, ...history.future] },
    step,
  };
}

/** The step redo would put back, with the history that follows it. The caller
 *  applies `step` itself. */
export function redoEdit(
  history: EditHistory,
): { history: EditHistory; step: EditStep } | null {
  const [step, ...rest] = history.future;
  if (step === undefined) return null;
  return {
    history: { past: [...history.past, step].slice(-HISTORY_LIMIT), future: rest },
    step,
  };
}
