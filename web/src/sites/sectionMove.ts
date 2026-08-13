// Moving a section by dragging it on the page (ADR 0042, S3.01b), turned into
// the one change the page editor already makes: take the section out at one
// index, put it back at another.
//
// The preview document reports a **neighbour**, not a destination:
// `{alo:"site-section-move", from, before}`, where `before` is the index the
// moved section now sits above and `null` means it was dropped at the end.
// That is all a document can honestly know — it holds elements, not the
// sections array — and it keeps the index arithmetic here, in one tested
// function, instead of inside a string of JavaScript in a renderer.
//
// Nothing here trusts the frame. Its document has no origin, it is built from
// tenant content, and a message is whatever somebody typed: both indices are
// checked against the stack this app is already holding, and a message that
// does not resolve produces `null` rather than a move aimed at whatever has
// since taken that position.
import type { Section } from "./sections";

/** What the preview frame reports when a section is dropped, or moved with
 *  `Alt` + an arrow key. */
export interface SectionMove {
  /** The section's index in the page envelope when the gesture started. */
  from: number;
  /** The index of the section it now sits above, or `null` at the end. */
  before: number | null;
}

/** Reads the message the preview frame posted, or `null` for anything else.
 *
 *  `event.origin` is `"null"` for a sandboxed `srcdoc` document and therefore
 *  proves nothing, so the caller proves the *sender* instead by passing the
 *  frame's own window; that is the check this app can actually rely on. */
export function readSectionMoveMessage(
  data: unknown,
  fromPreview: boolean,
): SectionMove | null {
  if (!fromPreview || typeof data !== "object" || data === null) return null;
  const message = data as Record<string, unknown>;
  if (message["alo"] !== "site-section-move") return null;
  const from = message["from"];
  const before = message["before"];
  if (!Number.isInteger(from)) return null;
  if (before !== null && !Number.isInteger(before)) return null;
  return { from: from as number, before: before as number | null };
}

/**
 * Where a section dropped above `before` has to be inserted.
 *
 * The two doors that move a section — `POST …/sections/{index}/move` and the
 * assistant's `reorder_section` — both splice: the section is *removed* first
 * and then inserted, so every index after it has already shifted down by one
 * by the time the destination is read. Dropping above a section that sits
 * below the moved one therefore lands at `before - 1`, and dropping at the end
 * lands at the last position of the shortened list.
 *
 * `null` means there is nothing to do: the gesture ended where it started, or
 * it named a position this page does not have.
 */
export function moveDestination(
  count: number,
  from: number,
  before: number | null,
): number | null {
  if (!Number.isInteger(from) || from < 0 || from >= count) return null;
  if (before !== null && (before < 0 || before >= count)) return null;
  const to = before === null ? count - 1 : before > from ? before - 1 : before;
  return to === from ? null : to;
}

/** The stack as it looks after that move — the same splice both doors do,
 *  used for the optimistic answer on a localized page (where sections are
 *  saved as a whole draft rather than one operation at a time). */
export function withSectionMoved(
  sections: Section[],
  from: number,
  to: number,
): Section[] | null {
  if (from < 0 || from >= sections.length) return null;
  if (to < 0 || to >= sections.length) return null;
  const next = [...sections];
  const [moved] = next.splice(from, 1);
  if (moved === undefined) return null;
  next.splice(to, 0, moved);
  return next;
}
