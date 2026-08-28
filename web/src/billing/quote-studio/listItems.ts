// The items of a list block and their nesting.
//
// A list block stores its items as one newline-separated string — that shape
// predates nesting and every saved design carries it. Nesting is encoded in
// the same string as leading tabs on a line: no tab is a top-level item, one
// tab a sub-item, two a sub-sub-item. An old design has no tabs and reads as
// it always did; a new one still round-trips through the same field.
import {
  MAX_LIST_LEVEL,
  listMarker,
  type ListLevel,
  type ListStyleId,
} from "./listStyles";

export interface ListItem {
  level: ListLevel;
  text: string;
}

export interface NumberedListItem extends ListItem {
  /** The marker the item is shown with — "1.", "b)", "1.2.1.", "●" … */
  marker: string;
}

function clampLevel(level: number): ListLevel {
  return Math.max(0, Math.min(MAX_LIST_LEVEL, level)) as ListLevel;
}

/** The stored string → items. An empty string is an empty list. Tabs beyond
 *  the deepest level are dropped rather than shown as text. */
export function parseListItems(items: string): ListItem[] {
  if (items === "") return [];
  return items.split("\n").map((line) => {
    const text = line.replace(/^\t+/, "");
    return { level: clampLevel(line.length - text.length), text };
  });
}

export function serializeListItems(items: readonly ListItem[]): string {
  return items.map((item) => "\t".repeat(item.level) + item.text).join("\n");
}

/** Indent (`+1`) or outdent (`-1`) one item. An item can nest at most one
 *  level under the item above it — there is no sub-item without an item —
 *  and the first item is always top-level. A move that would break that is
 *  ignored and the list returned unchanged. */
export function shiftListItemLevel(
  items: readonly ListItem[],
  index: number,
  step: -1 | 1,
): ListItem[] {
  const item = items[index];
  if (item === undefined) return [...items];
  const previous = index > 0 ? items[index - 1] : undefined;
  const ceiling = previous === undefined ? 0 : clampLevel(previous.level + 1);
  const next = clampLevel(Math.min(ceiling, item.level + step));
  if (next === item.level) return [...items];
  return items.map((candidate, candidateIndex) =>
    candidateIndex === index ? { ...candidate, level: next } : candidate,
  );
}

/** Whether `items[index]` may be indented one level further. */
export function canIndentListItem(items: readonly ListItem[], index: number): boolean {
  return shiftListItemLevel(items, index, 1)[index]?.level !== items[index]?.level;
}

/** Number every item in document order. A counter restarts when a shallower
 *  item appears, so "1. / a. / b. / 2. / a." counts the way a reader expects. */
export function numberListItems(
  items: readonly ListItem[],
  style: ListStyleId,
): NumberedListItem[] {
  const counters = [0, 0, 0];
  return items.map((item) => {
    counters[item.level] = (counters[item.level] ?? 0) + 1;
    for (let deeper = item.level + 1; deeper <= MAX_LIST_LEVEL; deeper++) {
      counters[deeper] = 0;
    }
    return { ...item, marker: listMarker(style, item.level, counters) };
  });
}
