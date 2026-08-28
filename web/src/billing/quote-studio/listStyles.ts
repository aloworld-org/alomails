// The catalogue of list styles a quotation list can use — the six numbering
// schemes and seven bullet schemes of a word processor's list library, each
// defined for three nesting levels.
//
// This is the single source of what a marker looks like. The editor, the
// read-only renderer and the gallery tiles all ask here, so a style can never
// look one way in the picker and another way in the document.

/** Nesting depth of a list item. Three levels, like the library it mirrors. */
export type ListLevel = 0 | 1 | 2;
export const MAX_LIST_LEVEL: ListLevel = 2;

export type NumberingStyleId =
  | "decimal"
  | "parenthesis"
  | "outline"
  | "upper-alpha"
  | "roman"
  | "leading-zero";

export type BulletStyleId =
  | "disc"
  | "diamond"
  | "square"
  | "arrow"
  | "star"
  | "chevron"
  | "checkbox";

export type ListStyleId = NumberingStyleId | BulletStyleId;

export const NUMBERING_STYLES: readonly NumberingStyleId[] = [
  "decimal",
  "parenthesis",
  "outline",
  "upper-alpha",
  "roman",
  "leading-zero",
];

export const BULLET_STYLES: readonly BulletStyleId[] = [
  "disc",
  "diamond",
  "square",
  "arrow",
  "star",
  "chevron",
  "checkbox",
];

const BULLET_GLYPHS: Record<BulletStyleId, readonly [string, string, string]> = {
  disc: ["●", "○", "■"],
  diamond: ["❖", "➢", "■"],
  square: ["❑", "❑", "❑"],
  arrow: ["➔", "◆", "●"],
  star: ["★", "○", "■"],
  chevron: ["➢", "○", "■"],
  checkbox: ["☐", "☐", "☐"],
};

/** 1 → A, 26 → Z, 27 → AA — the spreadsheet column sequence. */
function upperAlpha(n: number): string {
  if (n < 1) return String(n);
  let out = "";
  let rest = n;
  while (rest > 0) {
    out = String.fromCharCode(65 + ((rest - 1) % 26)) + out;
    rest = Math.floor((rest - 1) / 26);
  }
  return out;
}

const lowerAlpha = (n: number) => upperAlpha(n).toLowerCase();

const ROMAN: ReadonlyArray<readonly [number, string]> = [
  [1000, "M"], [900, "CM"], [500, "D"], [400, "CD"], [100, "C"], [90, "XC"],
  [50, "L"], [40, "XL"], [10, "X"], [9, "IX"], [5, "V"], [4, "IV"], [1, "I"],
];

function upperRoman(n: number): string {
  if (n < 1 || n > 3999) return String(n);
  let out = "";
  let rest = n;
  for (const [value, numeral] of ROMAN) {
    while (rest >= value) {
      out += numeral;
      rest -= value;
    }
  }
  return out;
}

const lowerRoman = (n: number) => upperRoman(n).toLowerCase();
const leadingZero = (n: number) => String(n).padStart(2, "0");

/** `counters[level]` is the position of the item at that level; shallower
 *  entries are its ancestors' positions (the outline scheme needs them). */
type Marker = (counters: readonly number[]) => string;

const at = (counters: readonly number[], level: number) => counters[level] ?? 0;

const NUMBERING: Record<NumberingStyleId, readonly [Marker, Marker, Marker]> = {
  decimal: [
    (c) => `${at(c, 0)}.`,
    (c) => `${lowerAlpha(at(c, 1))}.`,
    (c) => `${lowerRoman(at(c, 2))}.`,
  ],
  parenthesis: [
    (c) => `${at(c, 0)})`,
    (c) => `${lowerAlpha(at(c, 1))})`,
    (c) => `${lowerRoman(at(c, 2))})`,
  ],
  outline: [
    (c) => `${at(c, 0)}.`,
    (c) => `${at(c, 0)}.${at(c, 1)}.`,
    (c) => `${at(c, 0)}.${at(c, 1)}.${at(c, 2)}.`,
  ],
  "upper-alpha": [
    (c) => `${upperAlpha(at(c, 0))}.`,
    (c) => `${lowerAlpha(at(c, 1))}.`,
    (c) => `${lowerRoman(at(c, 2))}.`,
  ],
  roman: [
    (c) => `${upperRoman(at(c, 0))}.`,
    (c) => `${upperAlpha(at(c, 1))}.`,
    (c) => `${at(c, 2)}.`,
  ],
  "leading-zero": [
    (c) => `${leadingZero(at(c, 0))}.`,
    (c) => `${lowerAlpha(at(c, 1))}.`,
    (c) => `${lowerRoman(at(c, 2))}.`,
  ],
};

export function isNumberingStyle(style: ListStyleId): style is NumberingStyleId {
  return (NUMBERING_STYLES as readonly string[]).includes(style);
}

/** The marker text for an item at `level`, given the running counters. */
export function listMarker(
  style: ListStyleId,
  level: ListLevel,
  counters: readonly number[],
): string {
  return isNumberingStyle(style)
    ? NUMBERING[style][level](counters)
    : BULLET_GLYPHS[style][level];
}

/** What a list looked like before styles existed: plain numbers, round bullets. */
export function defaultListStyle(ordered: boolean): ListStyleId {
  return ordered ? "decimal" : "disc";
}

/** Any saved value → a style valid for this kind of list. A missing value,
 *  an unknown id, or a bullet scheme on a numbered list (possible after the
 *  list's kind was toggled in storage) all fall back to the default, so an
 *  old or hand-edited design always renders. */
export function resolveListStyle(value: unknown, ordered: boolean): ListStyleId {
  const known = ordered ? NUMBERING_STYLES : BULLET_STYLES;
  return typeof value === "string" && (known as readonly string[]).includes(value)
    ? (value as ListStyleId)
    : defaultListStyle(ordered);
}
